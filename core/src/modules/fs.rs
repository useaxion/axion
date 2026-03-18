/// `fs` native module — sandboxed filesystem access (issue #13).
///
/// Default sandbox: `%APPDATA%\axion\<app_name>\` (maps to
/// `AppData\Roaming\axion\<app_name>\` on Windows).
///
/// Path traversal attacks are blocked by canonicalising the resolved path
/// and verifying it remains under the sandbox root.
///
/// Methods:
/// - `fs.read`          — read a file as a UTF-8 string
/// - `fs.write`         — write content to a file (creates dirs as needed)
/// - `fs.delete`        — delete a file
/// - `fs.pickDirectory` — open the native Windows folder-picker dialog
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::module::AxionModule;
use crate::permissions::engine::{PermissionEngine, PermissionKey};
use crate::rpc::dispatcher::{make_handler, Dispatcher};
use crate::rpc::schema::{error_codes, RpcErrorPayload};

// ── Sandbox root helper ───────────────────────────────────────────────────────

/// Resolve the sandbox root for `app_name`.
///
/// Returns `%APPDATA%\axion\<app_name>` (the `Roaming` AppData folder).
/// Falls back to the current directory if the env var is unavailable.
pub fn sandbox_root(app_name: &str) -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("axion").join(app_name)
}

/// Resolve `rel_path` inside `root`, blocking path traversal.
///
/// Returns the canonical absolute path if it is inside `root`.
/// Returns `Err` if the path would escape the sandbox.
pub fn resolve_sandboxed(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
    // Reject absolute paths early (e.g. `C:\Windows\System32\...`).
    let p = Path::new(rel_path);
    if p.is_absolute() {
        return Err(format!(
            "absolute paths are not allowed inside the sandbox: {rel_path}"
        ));
    }

    // Build the joined path and create parent dirs so canonicalize works.
    let candidate = root.join(p);

    // If the file doesn't exist yet, canonicalize the parent and re-append.
    let canonical = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|e| format!("cannot resolve path '{rel_path}': {e}"))?
    } else {
        // Canonicalize the deepest existing ancestor.
        let mut existing = candidate.clone();
        let mut suffix = vec![];
        loop {
            if existing.exists() || existing == root {
                break;
            }
            suffix.push(existing.file_name().unwrap_or_default().to_owned());
            existing = existing
                .parent()
                .unwrap_or(root)
                .to_path_buf();
        }
        let mut base = existing
            .canonicalize()
            .unwrap_or_else(|_| existing.clone());
        for part in suffix.iter().rev() {
            base = base.join(part);
        }
        base
    };

    // Canonicalize root (create it if missing for test environments).
    let canonical_root = if root.exists() {
        root.canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
    } else {
        root.to_path_buf()
    };

    if canonical.starts_with(&canonical_root) {
        Ok(canonical)
    } else {
        Err(format!(
            "path traversal detected: '{rel_path}' resolves outside the sandbox"
        ))
    }
}

// ── FsModule ──────────────────────────────────────────────────────────────────

/// The `fs` module. Initialise with the app sandbox root.
pub struct FsModule {
    /// Absolute path to the app's sandbox directory.
    sandbox: Arc<PathBuf>,
}

impl FsModule {
    /// Create an `FsModule` sandboxed to `sandbox_path`.
    pub fn new(sandbox_path: impl Into<PathBuf>) -> Self {
        Self {
            sandbox: Arc::new(sandbox_path.into()),
        }
    }

    /// Create an `FsModule` using the default sandbox for `app_name`.
    pub fn for_app(app_name: &str) -> Self {
        Self::new(sandbox_root(app_name))
    }
}

impl AxionModule for FsModule {
    fn name(&self) -> &'static str {
        "fs"
    }

    fn register_handlers(&self, dispatcher: &mut Dispatcher) {
        let sandbox = self.sandbox.clone();

        // ── fs.read ───────────────────────────────────────────────────────────
        {
            let sandbox = sandbox.clone();
            dispatcher.register(
                "fs.read",
                make_handler(move |params: Value| {
                    let sandbox = sandbox.clone();
                    async move {
                        let path_str = params
                            .get("path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'path' (string) is required",
                                )
                            })?;

                        let abs_path = resolve_sandboxed(&sandbox, path_str)
                            .map_err(|e| RpcErrorPayload::new(error_codes::PERMISSION_DENIED, e))?;

                        let content =
                            tokio::fs::read_to_string(&abs_path).await.map_err(|e| {
                                RpcErrorPayload::new(
                                    error_codes::INTERNAL_ERROR,
                                    format!("fs.read failed: {e}"),
                                )
                            })?;

                        Ok(json!({ "content": content }))
                    }
                }),
            );
        }

        // ── fs.write ──────────────────────────────────────────────────────────
        {
            let sandbox = sandbox.clone();
            dispatcher.register(
                "fs.write",
                make_handler(move |params: Value| {
                    let sandbox = sandbox.clone();
                    async move {
                        let path_str = params
                            .get("path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'path' (string) is required",
                                )
                            })?;
                        let content = params
                            .get("content")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'content' (string) is required",
                                )
                            })?
                            .to_string();

                        let abs_path = resolve_sandboxed(&sandbox, path_str)
                            .map_err(|e| RpcErrorPayload::new(error_codes::PERMISSION_DENIED, e))?;

                        // Create all parent directories.
                        if let Some(parent) = abs_path.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                                RpcErrorPayload::new(
                                    error_codes::INTERNAL_ERROR,
                                    format!("fs.write: cannot create directories: {e}"),
                                )
                            })?;
                        }

                        tokio::fs::write(&abs_path, content.as_bytes())
                            .await
                            .map_err(|e| {
                                RpcErrorPayload::new(
                                    error_codes::INTERNAL_ERROR,
                                    format!("fs.write failed: {e}"),
                                )
                            })?;

                        Ok(json!({}))
                    }
                }),
            );
        }

        // ── fs.delete ─────────────────────────────────────────────────────────
        {
            let sandbox = sandbox.clone();
            dispatcher.register(
                "fs.delete",
                make_handler(move |params: Value| {
                    let sandbox = sandbox.clone();
                    async move {
                        let path_str = params
                            .get("path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'path' (string) is required",
                                )
                            })?;

                        let abs_path = resolve_sandboxed(&sandbox, path_str)
                            .map_err(|e| RpcErrorPayload::new(error_codes::PERMISSION_DENIED, e))?;

                        tokio::fs::remove_file(&abs_path).await.map_err(|e| {
                            RpcErrorPayload::new(
                                error_codes::INTERNAL_ERROR,
                                format!("fs.delete failed: {e}"),
                            )
                        })?;

                        Ok(json!({}))
                    }
                }),
            );
        }

        // ── fs.pickDirectory ─────────────────────────────────────────────────
        dispatcher.register(
            "fs.pickDirectory",
            make_handler(|_params: Value| async move { pick_directory_impl().await }),
        );
    }

    fn declare_permissions(&self, engine: &mut PermissionEngine) {
        engine.require("fs.read", PermissionKey::FsAppData);
        engine.require("fs.write", PermissionKey::FsAppData);
        engine.require("fs.delete", PermissionKey::FsAppData);
        engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
    }
}

// ── pickDirectory implementation ──────────────────────────────────────────────

async fn pick_directory_impl() -> Result<Value, RpcErrorPayload> {
    // Spawn blocking because Win32 COM dialog is synchronous.
    tokio::task::spawn_blocking(pick_directory_blocking)
        .await
        .map_err(|_| RpcErrorPayload::new(error_codes::INTERNAL_ERROR, "pickDirectory task failed"))?
}

#[cfg(windows)]
fn pick_directory_blocking() -> Result<Value, RpcErrorPayload> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};

    unsafe {
        // Initialize COM for this thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| {
                RpcErrorPayload::new(
                    error_codes::INTERNAL_ERROR,
                    format!("CoCreateInstance failed: {e}"),
                )
            })?;

        // Set the dialog to folder-picker mode.
        let mut options = dialog.GetOptions().map_err(|e| {
            RpcErrorPayload::new(error_codes::INTERNAL_ERROR, format!("GetOptions failed: {e}"))
        })?;
        options |= FOS_PICKFOLDERS;
        dialog.SetOptions(options).map_err(|e| {
            RpcErrorPayload::new(error_codes::INTERNAL_ERROR, format!("SetOptions failed: {e}"))
        })?;

        // Show the dialog. If the user cancels, return null.
        if dialog.Show(None).is_err() {
            CoUninitialize();
            return Ok(json!({ "path": null }));
        }

        let item = dialog.GetResult().map_err(|e| {
            RpcErrorPayload::new(error_codes::INTERNAL_ERROR, format!("GetResult failed: {e}"))
        })?;

        let display_name = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| {
            RpcErrorPayload::new(
                error_codes::INTERNAL_ERROR,
                format!("GetDisplayName failed: {e}"),
            )
        })?;

        let path = display_name.to_string().map_err(|e| {
            RpcErrorPayload::new(
                error_codes::INTERNAL_ERROR,
                format!("path encoding error: {e}"),
            )
        })?;

        CoUninitialize();
        Ok(json!({ "path": path }))
    }
}

#[cfg(not(windows))]
fn pick_directory_blocking() -> Result<Value, RpcErrorPayload> {
    // On non-Windows platforms, return a graceful error.
    Err(RpcErrorPayload::new(
        error_codes::INTERNAL_ERROR,
        "fs.pickDirectory is only supported on Windows",
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_sandbox() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    // ── resolve_sandboxed ────────────────────────────────────────────────────

    #[test]
    fn resolve_sandboxed_allows_simple_relative_path() {
        let dir = make_sandbox();
        let result = resolve_sandboxed(dir.path(), "notes.txt");
        assert!(result.is_ok(), "simple relative path must be allowed: {:?}", result);
        let resolved = result.unwrap();
        // Use canonicalized dir.path() for comparison to handle short vs long
        // path aliases on Windows.
        let canonical_dir = dir.path().canonicalize().unwrap_or_else(|_| dir.path().to_path_buf());
        assert!(
            resolved.starts_with(&canonical_dir) || resolved.starts_with(dir.path()),
            "resolved path must be inside sandbox: resolved={resolved:?}, sandbox={canonical_dir:?}"
        );
    }

    #[test]
    fn resolve_sandboxed_allows_nested_relative_path() {
        let dir = make_sandbox();
        let result = resolve_sandboxed(dir.path(), "data/cache/file.json");
        assert!(result.is_ok(), "nested relative path must be allowed");
    }

    #[test]
    fn resolve_sandboxed_blocks_path_traversal() {
        let dir = make_sandbox();
        let result = resolve_sandboxed(dir.path(), "../../etc/passwd");
        assert!(result.is_err(), "path traversal must be blocked");
        let err = result.unwrap_err();
        assert!(
            err.contains("traversal") || err.contains("outside"),
            "error must mention traversal: {err}"
        );
    }

    #[test]
    fn resolve_sandboxed_blocks_absolute_path() {
        let dir = make_sandbox();
        #[cfg(windows)]
        let abs = r"C:\Windows\System32\secret.dll";
        #[cfg(not(windows))]
        let abs = "/etc/passwd";
        let result = resolve_sandboxed(dir.path(), abs);
        assert!(result.is_err(), "absolute path must be blocked inside sandbox");
    }

    #[test]
    fn resolve_sandboxed_blocks_encoded_traversal() {
        let dir = make_sandbox();
        // A literal ".." component — not URL-encoded but still a traversal.
        let result = resolve_sandboxed(dir.path(), "foo/../../../secret");
        assert!(
            result.is_err(),
            "traversal via foo/../../../ must be blocked"
        );
    }

    // ── fs.read / fs.write / fs.delete round-trip ────────────────────────────

    #[tokio::test]
    async fn read_write_delete_round_trip_inside_sandbox() {
        let dir = make_sandbox();
        let module = FsModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                fs: Some(crate::permissions::FsPermissions {
                    app_data: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        // Write.
        let write_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "fs.write".into(),
                params: serde_json::json!({ "path": "test.txt", "content": "hello world" }),
            })
            .await;
        assert!(write_resp.is_ok(), "fs.write must succeed: {:?}", write_resp);

        // Read back.
        let read_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 2,
                method: "fs.read".into(),
                params: serde_json::json!({ "path": "test.txt" }),
            })
            .await;
        assert!(read_resp.is_ok(), "fs.read must succeed");
        if let crate::rpc::schema::RpcResponse::Success { result, .. } = read_resp {
            assert_eq!(result["content"], "hello world");
        }

        // Delete.
        let del_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 3,
                method: "fs.delete".into(),
                params: serde_json::json!({ "path": "test.txt" }),
            })
            .await;
        assert!(del_resp.is_ok(), "fs.delete must succeed");

        // Read after delete must fail.
        let read_after_del = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 4,
                method: "fs.read".into(),
                params: serde_json::json!({ "path": "test.txt" }),
            })
            .await;
        assert!(
            read_after_del.is_err(),
            "fs.read on deleted file must fail"
        );
    }

    #[tokio::test]
    async fn write_creates_intermediate_directories() {
        let dir = make_sandbox();
        let module = FsModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                fs: Some(crate::permissions::FsPermissions {
                    app_data: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "fs.write".into(),
                params: serde_json::json!({
                    "path": "a/b/c/deep.txt",
                    "content": "deep content"
                }),
            })
            .await;
        assert!(resp.is_ok(), "fs.write must create parent directories");
        assert!(dir.path().join("a/b/c/deep.txt").exists());
    }

    #[tokio::test]
    async fn path_traversal_is_blocked_via_rpc() {
        let dir = make_sandbox();
        let module = FsModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                fs: Some(crate::permissions::FsPermissions {
                    app_data: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "fs.read".into(),
                params: serde_json::json!({ "path": "../../secret.txt" }),
            })
            .await;
        assert!(
            resp.is_err(),
            "path traversal attack must be blocked by the fs module"
        );
    }

    // ── Permission checks ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn fs_denied_when_app_data_not_granted() {
        let dir = make_sandbox();
        let module = FsModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions::default(), // nothing granted
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "fs.write".into(),
                params: serde_json::json!({ "path": "x.txt", "content": "y" }),
            })
            .await;
        assert!(resp.is_err());
        if let crate::rpc::schema::RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, crate::rpc::schema::error_codes::PERMISSION_DENIED);
        }
    }
}
