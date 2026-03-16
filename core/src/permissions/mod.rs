/// Axion permission system — schema, parser, and runtime checks.
///
/// Each Axion app declares the native capabilities it needs in a
/// `permissions.json` file at the project root. The runtime loads this
/// file at startup and rejects any RPC call whose module has not been
/// explicitly granted.
///
/// # Example `permissions.json`
/// ```json
/// {
///   "fs": { "appData": true, "userSelected": true, "absolutePath": false },
///   "storage": true,
///   "notifications": true,
///   "system": true,
///   "window": true
/// }
/// ```
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors that can occur while loading or validating `permissions.json`.
#[derive(Debug, Error)]
pub enum PermissionError {
    /// The file does not exist or could not be read.
    #[error("permissions.json not found at '{path}': {source}")]
    NotFound {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The file exists but its contents are not valid JSON or do not match
    /// the expected schema.
    #[error("permissions.json is invalid: {0}")]
    Invalid(#[from] serde_json::Error),
}

// ── Schema ────────────────────────────────────────────────────────────────────

/// Granular filesystem permission flags.
///
/// Omitting `fs` entirely from `permissions.json` denies all filesystem access.
/// Including `fs` with all flags `false` is equivalent to omitting it.
///
/// ```json
/// "fs": { "appData": true, "userSelected": false, "absolutePath": false }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FsPermissions {
    /// Read/write access to the app's AppData sandbox:
    /// `AppData/Local/Axion/<AppName>/`.
    ///
    /// This is the default safe storage location for all Axion apps.
    /// Required by `fs.read`, `fs.write`, and `fs.delete`.
    #[serde(default, rename = "appData")]
    pub app_data: bool,

    /// Access to directories explicitly chosen by the user via a system
    /// directory picker (`fs.pickDirectory`).
    ///
    /// Grants access only to the directory the user selected — not the
    /// whole filesystem.
    #[serde(default, rename = "userSelected")]
    pub user_selected: bool,

    /// Read/write access to arbitrary absolute paths on the filesystem.
    ///
    /// This is the most powerful (and dangerous) filesystem permission.
    /// Defaults to `false`. Only grant if strictly required.
    #[serde(default, rename = "absolutePath")]
    pub absolute_path: bool,
}

impl FsPermissions {
    /// Returns `true` if at least one filesystem flag is enabled.
    pub fn any_enabled(&self) -> bool {
        self.app_data || self.user_selected || self.absolute_path
    }
}

/// The full set of permissions an Axion app may declare.
///
/// Every field defaults to `false` / `None` when omitted from
/// `permissions.json`. Unrecognised keys are rejected to catch typos early.
///
/// See [`FsPermissions`] for the granular filesystem flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Filesystem access. Omit to deny all `fs.*` RPC calls.
    ///
    /// Granular flags control which paths and operations are permitted.
    #[serde(default)]
    pub fs: Option<FsPermissions>,

    /// Persistent key-value storage (`storage.get`, `storage.set`,
    /// `storage.remove`).
    #[serde(default)]
    pub storage: bool,

    /// System desktop notifications (`notifications.show`).
    #[serde(default)]
    pub notifications: bool,

    /// Read-only system information: OS name, platform, version
    /// (`system.info`, `system.platform`, `system.version`).
    #[serde(default)]
    pub system: bool,

    /// Window management: minimize, maximize, close, set title
    /// (`window.minimize`, `window.maximize`, `window.close`,
    /// `window.setTitle`).
    #[serde(default)]
    pub window: bool,
}

// ── Parser ────────────────────────────────────────────────────────────────────

impl Permissions {
    /// Load and validate `permissions.json` from `path`.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::NotFound`] — file is missing or unreadable.
    /// - [`PermissionError::Invalid`] — file is not valid JSON or contains
    ///   unknown keys (typos are caught here before runtime).
    ///
    /// # Panics
    ///
    /// Does not panic. All failure modes are returned as `Err`.
    pub fn load(path: &Path) -> Result<Self, PermissionError> {
        let contents = std::fs::read_to_string(path).map_err(|source| PermissionError::NotFound {
            path: path.display().to_string(),
            source,
        })?;

        let permissions: Self = serde_json::from_str(&contents)?;
        Ok(permissions)
    }

    /// Returns `true` if the RPC `method` is permitted by this configuration.
    ///
    /// Method names are dot-namespaced: `"<module>.<capability>"`.
    /// This is a coarse module-level check. Granular checks (e.g. which
    /// filesystem paths are accessible) are enforced by the Permission Engine
    /// (issue #9).
    ///
    /// Any method whose module is not one of the five built-in modules is
    /// denied.
    pub fn allows(&self, method: &str) -> bool {
        match method.split_once('.') {
            Some(("fs", _)) => self.fs.as_ref().map_or(false, |f| f.any_enabled()),
            Some(("storage", _)) => self.storage,
            Some(("notifications", _)) => self.notifications,
            Some(("system", _)) => self.system,
            Some(("window", _)) => self.window,
            _ => false,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(json: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(json.as_bytes()).unwrap();
        f
    }

    // ── Deserialization ───────────────────────────────────────────────────────

    #[test]
    fn full_permissions_deserialize_correctly() {
        let json = r#"{
            "fs": { "appData": true, "userSelected": true, "absolutePath": false },
            "storage": true,
            "notifications": true,
            "system": true,
            "window": true
        }"#;
        let p: Permissions = serde_json::from_str(json).unwrap();

        assert_eq!(p.fs, Some(FsPermissions { app_data: true, user_selected: true, absolute_path: false }));
        assert!(p.storage);
        assert!(p.notifications);
        assert!(p.system);
        assert!(p.window);
    }

    #[test]
    fn empty_object_deserializes_to_all_denied() {
        let p: Permissions = serde_json::from_str("{}").unwrap();
        assert_eq!(p, Permissions::default());
        assert!(p.fs.is_none());
        assert!(!p.storage);
        assert!(!p.notifications);
        assert!(!p.system);
        assert!(!p.window);
    }

    #[test]
    fn partial_permissions_deserialize_with_defaults() {
        let json = r#"{ "storage": true }"#;
        let p: Permissions = serde_json::from_str(json).unwrap();
        assert!(p.storage);
        assert!(p.fs.is_none());
        assert!(!p.notifications);
        assert!(!p.system);
        assert!(!p.window);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let json = r#"{ "unknownModule": true }"#;
        let result: Result<Permissions, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown keys must be rejected");
    }

    #[test]
    fn fs_with_only_app_data_deserializes_correctly() {
        let json = r#"{ "fs": { "appData": true } }"#;
        let p: Permissions = serde_json::from_str(json).unwrap();
        let fs = p.fs.unwrap();
        assert!(fs.app_data);
        assert!(!fs.user_selected);
        assert!(!fs.absolute_path);
    }

    // ── File loader ───────────────────────────────────────────────────────────

    #[test]
    fn load_reads_valid_file() {
        let f = write_temp(r#"{ "storage": true, "window": true }"#);
        let p = Permissions::load(f.path()).unwrap();
        assert!(p.storage);
        assert!(p.window);
    }

    #[test]
    fn load_returns_not_found_for_missing_file() {
        let result = Permissions::load(Path::new("/nonexistent/permissions.json"));
        assert!(matches!(result, Err(PermissionError::NotFound { .. })));
    }

    #[test]
    fn load_returns_invalid_for_bad_json() {
        let f = write_temp("{ invalid json }");
        let result = Permissions::load(f.path());
        assert!(matches!(result, Err(PermissionError::Invalid(_))));
    }

    #[test]
    fn load_returns_invalid_for_unknown_key() {
        let f = write_temp(r#"{ "unknownModule": true }"#);
        let result = Permissions::load(f.path());
        assert!(matches!(result, Err(PermissionError::Invalid(_))));
    }

    // ── allows() ─────────────────────────────────────────────────────────────

    #[test]
    fn allows_returns_true_for_granted_modules() {
        let p = Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            storage: true,
            notifications: true,
            system: true,
            window: true,
        };
        assert!(p.allows("fs.read"));
        assert!(p.allows("fs.write"));
        assert!(p.allows("storage.get"));
        assert!(p.allows("storage.set"));
        assert!(p.allows("notifications.show"));
        assert!(p.allows("system.info"));
        assert!(p.allows("window.minimize"));
    }

    #[test]
    fn allows_returns_false_for_denied_modules() {
        let p = Permissions::default();
        assert!(!p.allows("fs.read"));
        assert!(!p.allows("storage.get"));
        assert!(!p.allows("notifications.show"));
        assert!(!p.allows("system.info"));
        assert!(!p.allows("window.close"));
    }

    #[test]
    fn allows_returns_false_for_fs_when_all_flags_disabled() {
        let p = Permissions {
            fs: Some(FsPermissions::default()), // all false
            ..Default::default()
        };
        assert!(!p.allows("fs.read"), "fs with all flags false must be denied");
    }

    #[test]
    fn allows_returns_false_for_unknown_module() {
        let p = Permissions { storage: true, ..Default::default() };
        assert!(!p.allows("unknown.method"));
        assert!(!p.allows("fs"));          // no dot — not a valid method name
        assert!(!p.allows(""));
    }

    #[test]
    fn allows_fs_only_when_app_data_granted() {
        let p = Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        };
        assert!(p.allows("fs.read"));
        assert!(p.allows("fs.write"));
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    #[test]
    fn full_permissions_round_trips() {
        let original = Permissions {
            fs: Some(FsPermissions { app_data: true, user_selected: true, absolute_path: false }),
            storage: true,
            notifications: false,
            system: true,
            window: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: Permissions = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }
}
