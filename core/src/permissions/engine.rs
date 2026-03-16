/// Permission Engine — validates every RPC call against the app's declared
/// permissions before the handler is invoked.
///
/// # How it works
///
/// 1. At startup, each native module calls [`PermissionEngine::require`] to
///    declare which [`PermissionKey`] each of its RPC methods needs.
/// 2. On every incoming RPC request, the dispatcher calls
///    [`PermissionEngine::check`] *before* routing to the handler.
/// 3. A denied call returns [`PermissionDenied`] — never silently dropped.
///
/// There is **no dev-mode bypass**. The engine behaves identically in
/// development and production.
///
/// # Example
///
/// ```rust,ignore
/// let mut engine = PermissionEngine::load(Path::new("permissions.json"))?;
///
/// // Each module registers its methods at startup.
/// engine.require("fs.read",   PermissionKey::FsAppData);
/// engine.require("fs.write",  PermissionKey::FsAppData);
/// engine.require("storage.get", PermissionKey::Storage);
///
/// // Dispatcher calls check() before invoking any handler.
/// engine.check("fs.read")?;       // Ok(()) if fs.appData is granted
/// engine.check("notifications.show")?; // Err(PermissionDenied { ... })
/// ```
use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

use super::{PermissionError, Permissions};

// ── Permission key ────────────────────────────────────────────────────────────

/// A specific, grantable permission flag from `permissions.json`.
///
/// Each native module method maps to exactly one `PermissionKey`. The engine
/// checks whether the app's loaded permissions grant that key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PermissionKey {
    /// `fs.appData` — read/write access to the app's AppData sandbox.
    FsAppData,
    /// `fs.userSelected` — access to user-picked directories.
    FsUserSelected,
    /// `fs.absolutePath` — access to arbitrary absolute paths.
    FsAbsolutePath,
    /// `storage` — persistent key-value storage.
    Storage,
    /// `notifications` — desktop notification display.
    Notifications,
    /// `system` — read-only system information.
    System,
    /// `window` — window management (minimize, maximize, close, setTitle).
    Window,
}

impl PermissionKey {
    /// The human-readable key name as it appears in `permissions.json`.
    ///
    /// Used in error messages and documentation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FsAppData => "fs.appData",
            Self::FsUserSelected => "fs.userSelected",
            Self::FsAbsolutePath => "fs.absolutePath",
            Self::Storage => "storage",
            Self::Notifications => "notifications",
            Self::System => "system",
            Self::Window => "window",
        }
    }
}

impl std::fmt::Display for PermissionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── PermissionDenied ──────────────────────────────────────────────────────────

/// Returned by [`PermissionEngine::check`] when a call is not permitted.
///
/// Contains all the context needed to build a clear RPC error response and
/// to help the app developer fix their `permissions.json`.
#[derive(Debug, Error, PartialEq, Clone)]
#[error("Permission denied for '{method}': requires '{required}' — {message}")]
pub struct PermissionDenied {
    /// The RPC method that was denied, e.g. `"fs.write"`.
    pub method: String,
    /// The permission key that was missing, e.g. `"fs.appData"`.
    pub required: String,
    /// A user-facing description of what needs to be added to `permissions.json`.
    pub message: String,
}

impl PermissionDenied {
    fn not_granted(method: &str, key: &PermissionKey) -> Self {
        Self {
            method: method.to_string(),
            required: key.as_str().to_string(),
            message: format!(
                "add \"{key}: true\" to your permissions.json to enable '{method}'"
            ),
        }
    }

    fn not_registered(method: &str) -> Self {
        Self {
            method: method.to_string(),
            required: String::from("(none registered)"),
            message: format!(
                "'{method}' has not been registered by any module — \
                 ensure the module is initialised before the first RPC call"
            ),
        }
    }
}

// ── PermissionEngine ──────────────────────────────────────────────────────────

/// The security enforcement layer for the Axion runtime.
///
/// Holds the app's loaded [`Permissions`] config and a registry of
/// `method → required PermissionKey` mappings contributed by each native module.
///
/// See the [module-level documentation](self) for the full usage pattern.
pub struct PermissionEngine {
    permissions: Permissions,
    /// method name → the single permission key it requires.
    requirements: HashMap<String, PermissionKey>,
}

impl PermissionEngine {
    /// Create a new engine from an already-loaded [`Permissions`] config.
    ///
    /// No method requirements are registered yet. Call [`require`](Self::require)
    /// for each method that native modules will expose.
    pub fn new(permissions: Permissions) -> Self {
        Self {
            permissions,
            requirements: HashMap::new(),
        }
    }

    /// Load `permissions.json` from `path` and return a new engine.
    ///
    /// # Errors
    ///
    /// Propagates [`PermissionError`] if the file is missing or malformed.
    /// The caller (typically `main`) should treat this as a fatal startup error.
    pub fn load(path: &Path) -> Result<Self, PermissionError> {
        let permissions = Permissions::load(path)?;
        Ok(Self::new(permissions))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    /// Register the [`PermissionKey`] required to invoke `method`.
    ///
    /// Returns `true` on success. Returns `false` — without overwriting the
    /// existing entry — if `method` is already registered. First registration
    /// wins, preventing modules from hijacking each other's methods.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// engine.require("fs.read",  PermissionKey::FsAppData);
    /// engine.require("fs.write", PermissionKey::FsAppData);
    /// engine.require("storage.get", PermissionKey::Storage);
    /// ```
    pub fn require(&mut self, method: impl Into<String>, key: PermissionKey) -> bool {
        let method = method.into();
        if self.requirements.contains_key(&method) {
            return false;
        }
        self.requirements.insert(method, key);
        true
    }

    // ── Enforcement ───────────────────────────────────────────────────────────

    /// Check whether `method` may be invoked under the loaded permissions.
    ///
    /// Returns `Ok(())` if the call is permitted.
    ///
    /// Returns `Err(PermissionDenied)` if:
    /// - `method` has no registered requirement (module not initialised).
    /// - The required [`PermissionKey`] is not granted in `permissions.json`.
    ///
    /// **No environment bypass exists.** Dev and production are identical.
    pub fn check(&self, method: &str) -> Result<(), PermissionDenied> {
        let key = self
            .requirements
            .get(method)
            .ok_or_else(|| PermissionDenied::not_registered(method))?;

        if self.is_granted(key) {
            Ok(())
        } else {
            Err(PermissionDenied::not_granted(method, key))
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Return a reference to the loaded [`Permissions`] config.
    pub fn permissions(&self) -> &Permissions {
        &self.permissions
    }

    /// Return the number of registered method→permission mappings.
    pub fn registered_count(&self) -> usize {
        self.requirements.len()
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn is_granted(&self, key: &PermissionKey) -> bool {
        let p = &self.permissions;
        match key {
            PermissionKey::FsAppData => p.fs.as_ref().is_some_and(|f| f.app_data),
            PermissionKey::FsUserSelected => p.fs.as_ref().is_some_and(|f| f.user_selected),
            PermissionKey::FsAbsolutePath => p.fs.as_ref().is_some_and(|f| f.absolute_path),
            PermissionKey::Storage => p.storage,
            PermissionKey::Notifications => p.notifications,
            PermissionKey::System => p.system,
            PermissionKey::Window => p.window,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    use crate::permissions::FsPermissions;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn full_permissions() -> Permissions {
        Permissions {
            fs: Some(FsPermissions {
                app_data: true,
                user_selected: true,
                absolute_path: true,
            }),
            storage: true,
            notifications: true,
            system: true,
            window: true,
        }
    }

    fn engine_with(permissions: Permissions) -> PermissionEngine {
        PermissionEngine::new(permissions)
    }

    fn write_temp(json: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(json.as_bytes()).unwrap();
        f
    }

    // ── PermissionKey display ─────────────────────────────────────────────────

    #[test]
    fn permission_key_as_str_returns_correct_names() {
        assert_eq!(PermissionKey::FsAppData.as_str(), "fs.appData");
        assert_eq!(PermissionKey::FsUserSelected.as_str(), "fs.userSelected");
        assert_eq!(PermissionKey::FsAbsolutePath.as_str(), "fs.absolutePath");
        assert_eq!(PermissionKey::Storage.as_str(), "storage");
        assert_eq!(PermissionKey::Notifications.as_str(), "notifications");
        assert_eq!(PermissionKey::System.as_str(), "system");
        assert_eq!(PermissionKey::Window.as_str(), "window");
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn require_returns_true_for_new_method() {
        let mut engine = engine_with(Permissions::default());
        assert!(engine.require("fs.read", PermissionKey::FsAppData));
    }

    #[test]
    fn require_returns_false_for_duplicate_method() {
        let mut engine = engine_with(Permissions::default());
        engine.require("fs.read", PermissionKey::FsAppData);
        assert!(!engine.require("fs.read", PermissionKey::FsAbsolutePath),
            "duplicate registration must be rejected");
    }

    #[test]
    fn first_registration_wins_on_duplicate() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.read", PermissionKey::FsAppData);
        // Second attempt with a different key must be silently ignored.
        engine.require("fs.read", PermissionKey::FsAbsolutePath);
        // Should succeed because appData is granted, not absolutePath.
        assert!(engine.check("fs.read").is_ok());
    }

    #[test]
    fn registered_count_reflects_registrations() {
        let mut engine = engine_with(Permissions::default());
        assert_eq!(engine.registered_count(), 0);
        engine.require("fs.read", PermissionKey::FsAppData);
        engine.require("storage.get", PermissionKey::Storage);
        assert_eq!(engine.registered_count(), 2);
    }

    // ── check() — Ok paths ───────────────────────────────────────────────────

    #[test]
    fn check_returns_ok_for_granted_fs_app_data() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.read", PermissionKey::FsAppData);
        engine.require("fs.write", PermissionKey::FsAppData);
        engine.require("fs.delete", PermissionKey::FsAppData);
        assert!(engine.check("fs.read").is_ok());
        assert!(engine.check("fs.write").is_ok());
        assert!(engine.check("fs.delete").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_fs_user_selected() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { user_selected: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
        assert!(engine.check("fs.pickDirectory").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_fs_absolute_path() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { absolute_path: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.readAbsolute", PermissionKey::FsAbsolutePath);
        assert!(engine.check("fs.readAbsolute").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_storage() {
        let mut engine = engine_with(Permissions { storage: true, ..Default::default() });
        engine.require("storage.get", PermissionKey::Storage);
        engine.require("storage.set", PermissionKey::Storage);
        engine.require("storage.remove", PermissionKey::Storage);
        assert!(engine.check("storage.get").is_ok());
        assert!(engine.check("storage.set").is_ok());
        assert!(engine.check("storage.remove").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_notifications() {
        let mut engine = engine_with(Permissions { notifications: true, ..Default::default() });
        engine.require("notifications.show", PermissionKey::Notifications);
        assert!(engine.check("notifications.show").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_system() {
        let mut engine = engine_with(Permissions { system: true, ..Default::default() });
        engine.require("system.info", PermissionKey::System);
        engine.require("system.platform", PermissionKey::System);
        engine.require("system.version", PermissionKey::System);
        assert!(engine.check("system.info").is_ok());
        assert!(engine.check("system.platform").is_ok());
        assert!(engine.check("system.version").is_ok());
    }

    #[test]
    fn check_returns_ok_for_granted_window() {
        let mut engine = engine_with(Permissions { window: true, ..Default::default() });
        engine.require("window.minimize", PermissionKey::Window);
        engine.require("window.maximize", PermissionKey::Window);
        engine.require("window.close", PermissionKey::Window);
        engine.require("window.setTitle", PermissionKey::Window);
        assert!(engine.check("window.minimize").is_ok());
        assert!(engine.check("window.maximize").is_ok());
        assert!(engine.check("window.close").is_ok());
        assert!(engine.check("window.setTitle").is_ok());
    }

    // ── check() — denied paths ───────────────────────────────────────────────

    #[test]
    fn check_returns_denied_for_unregistered_method() {
        let engine = engine_with(full_permissions());
        let err = engine.check("fs.read").unwrap_err();
        assert_eq!(err.method, "fs.read");
        assert!(err.message.contains("not been registered"));
    }

    #[test]
    fn check_returns_denied_when_fs_app_data_not_granted() {
        let mut engine = engine_with(Permissions::default()); // fs = None
        engine.require("fs.read", PermissionKey::FsAppData);
        let err = engine.check("fs.read").unwrap_err();
        assert_eq!(err.method, "fs.read");
        assert_eq!(err.required, "fs.appData");
        assert!(err.message.contains("fs.appData"));
    }

    #[test]
    fn check_returns_denied_when_fs_user_selected_not_granted() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
        let err = engine.check("fs.pickDirectory").unwrap_err();
        assert_eq!(err.required, "fs.userSelected");
    }

    #[test]
    fn check_returns_denied_when_fs_absolute_path_not_granted() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.readAbsolute", PermissionKey::FsAbsolutePath);
        let err = engine.check("fs.readAbsolute").unwrap_err();
        assert_eq!(err.required, "fs.absolutePath");
    }

    #[test]
    fn check_returns_denied_when_storage_not_granted() {
        let mut engine = engine_with(Permissions::default());
        engine.require("storage.get", PermissionKey::Storage);
        let err = engine.check("storage.get").unwrap_err();
        assert_eq!(err.method, "storage.get");
        assert_eq!(err.required, "storage");
    }

    #[test]
    fn check_returns_denied_when_notifications_not_granted() {
        let mut engine = engine_with(Permissions::default());
        engine.require("notifications.show", PermissionKey::Notifications);
        let err = engine.check("notifications.show").unwrap_err();
        assert_eq!(err.required, "notifications");
    }

    #[test]
    fn check_returns_denied_when_system_not_granted() {
        let mut engine = engine_with(Permissions::default());
        engine.require("system.info", PermissionKey::System);
        let err = engine.check("system.info").unwrap_err();
        assert_eq!(err.required, "system");
    }

    #[test]
    fn check_returns_denied_when_window_not_granted() {
        let mut engine = engine_with(Permissions::default());
        engine.require("window.close", PermissionKey::Window);
        let err = engine.check("window.close").unwrap_err();
        assert_eq!(err.required, "window");
    }

    #[test]
    fn check_denied_error_contains_method_name_and_required_key() {
        let mut engine = engine_with(Permissions::default());
        engine.require("storage.set", PermissionKey::Storage);
        let err = engine.check("storage.set").unwrap_err();
        let display = err.to_string();
        assert!(display.contains("storage.set"));
        assert!(display.contains("storage"));
    }

    // ── Granular fs flag isolation ────────────────────────────────────────────

    #[test]
    fn app_data_flag_does_not_grant_user_selected() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { app_data: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
        assert!(engine.check("fs.pickDirectory").is_err(),
            "appData must not grant userSelected");
    }

    #[test]
    fn user_selected_flag_does_not_grant_app_data() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { user_selected: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.read", PermissionKey::FsAppData);
        assert!(engine.check("fs.read").is_err(),
            "userSelected must not grant appData");
    }

    #[test]
    fn absolute_path_flag_does_not_grant_app_data() {
        let mut engine = engine_with(Permissions {
            fs: Some(FsPermissions { absolute_path: true, ..Default::default() }),
            ..Default::default()
        });
        engine.require("fs.read", PermissionKey::FsAppData);
        assert!(engine.check("fs.read").is_err(),
            "absolutePath must not grant appData");
    }

    // ── All five modules granted simultaneously ───────────────────────────────

    #[test]
    fn all_modules_pass_with_full_permissions() {
        let mut engine = engine_with(full_permissions());
        engine.require("fs.read", PermissionKey::FsAppData);
        engine.require("fs.write", PermissionKey::FsAppData);
        engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
        engine.require("fs.readAbsolute", PermissionKey::FsAbsolutePath);
        engine.require("storage.get", PermissionKey::Storage);
        engine.require("notifications.show", PermissionKey::Notifications);
        engine.require("system.info", PermissionKey::System);
        engine.require("window.close", PermissionKey::Window);

        assert!(engine.check("fs.read").is_ok());
        assert!(engine.check("fs.write").is_ok());
        assert!(engine.check("fs.pickDirectory").is_ok());
        assert!(engine.check("fs.readAbsolute").is_ok());
        assert!(engine.check("storage.get").is_ok());
        assert!(engine.check("notifications.show").is_ok());
        assert!(engine.check("system.info").is_ok());
        assert!(engine.check("window.close").is_ok());
    }

    // ── load() ────────────────────────────────────────────────────────────────

    #[test]
    fn load_from_valid_file_succeeds() {
        let f = write_temp(r#"{ "storage": true, "window": true }"#);
        let engine = PermissionEngine::load(f.path()).unwrap();
        assert!(engine.permissions().storage);
        assert!(engine.permissions().window);
    }

    #[test]
    fn load_from_missing_file_returns_not_found_error() {
        let result = PermissionEngine::load(Path::new("/nonexistent/permissions.json"));
        assert!(matches!(result, Err(PermissionError::NotFound { .. })));
    }

    #[test]
    fn load_from_invalid_json_returns_invalid_error() {
        let f = write_temp("{ invalid json }");
        let result = PermissionEngine::load(f.path());
        assert!(matches!(result, Err(PermissionError::Invalid(_))));
    }

    #[test]
    fn load_from_file_with_unknown_key_returns_invalid_error() {
        let f = write_temp(r#"{ "unknownModule": true }"#);
        let result = PermissionEngine::load(f.path());
        assert!(matches!(result, Err(PermissionError::Invalid(_))));
    }
}
