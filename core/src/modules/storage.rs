/// `storage` native module — persistent key-value store (issue #14).
///
/// Backed by a JSON file at `<sandbox_root>/storage.json`.
/// Values are stored as strings; callers serialize/deserialize as needed.
///
/// Concurrent read/write safety is provided by a `tokio::sync::Mutex` wrapping
/// the in-memory `HashMap`. Writes are flushed to disk atomically via a
/// temp-file + rename pattern to prevent corruption.
///
/// Methods:
/// - `storage.get`    — `{ key: string } → { value: string | null }`
/// - `storage.set`    — `{ key: string, value: string } → {}`
/// - `storage.remove` — `{ key: string } → {}`
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::module::AxionModule;
use crate::permissions::engine::{PermissionEngine, PermissionKey};
use crate::rpc::dispatcher::{make_handler, Dispatcher};
use crate::rpc::schema::{error_codes, RpcErrorPayload};

// ── Store ─────────────────────────────────────────────────────────────────────

/// In-process key-value store backed by a JSON file.
///
/// Wrap in `Arc<Mutex<Store>>` to share across async handler closures.
pub struct Store {
    path: PathBuf,
    data: HashMap<String, String>,
}

impl Store {
    /// Load `path` from disk. Creates an empty store if the file does not exist.
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let data = if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| format!("storage: failed to read store: {e}"))?;
            serde_json::from_str::<HashMap<String, String>>(&contents)
                .map_err(|e| format!("storage: failed to parse store: {e}"))?
        } else {
            HashMap::new()
        };
        Ok(Self { path, data })
    }

    /// Persist the store to disk using a temp-file + rename for atomicity.
    fn flush(&self) -> Result<(), String> {
        // Ensure the parent directory exists.
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("storage: failed to create directory: {e}"))?;
        }

        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("storage: failed to serialize store: {e}"))?;

        // Write to a sibling temp file then rename for atomicity.
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|e| format!("storage: failed to write temp file: {e}"))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("storage: failed to rename temp file: {e}"))?;

        Ok(())
    }

    /// Get the value for `key`, or `None` if absent.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    /// Set `key` to `value` and flush to disk.
    pub fn set(&mut self, key: String, value: String) -> Result<(), String> {
        self.data.insert(key, value);
        self.flush()
    }

    /// Remove `key` and flush to disk.
    pub fn remove(&mut self, key: &str) -> Result<(), String> {
        self.data.remove(key);
        self.flush()
    }
}

// ── StorageModule ─────────────────────────────────────────────────────────────

/// The `storage` module. Initialise with the app sandbox root.
pub struct StorageModule {
    store_path: PathBuf,
}

impl StorageModule {
    /// Create a `StorageModule` using `<sandbox>/storage.json`.
    pub fn new(sandbox_root: impl Into<PathBuf>) -> Self {
        Self {
            store_path: sandbox_root.into().join("storage.json"),
        }
    }
}

impl AxionModule for StorageModule {
    fn name(&self) -> &'static str {
        "storage"
    }

    fn register_handlers(&self, dispatcher: &mut Dispatcher) {
        // Load (or create) the store and wrap it in an Arc<Mutex<_>> so that
        // the three handlers share the same in-memory view.
        let store = Arc::new(Mutex::new(
            Store::load(self.store_path.clone())
                .unwrap_or_else(|_| Store {
                    path: self.store_path.clone(),
                    data: HashMap::new(),
                }),
        ));

        // ── storage.get ───────────────────────────────────────────────────────
        {
            let store = store.clone();
            dispatcher.register(
                "storage.get",
                make_handler(move |params: Value| {
                    let store = store.clone();
                    async move {
                        let key = params
                            .get("key")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'key' (string) is required",
                                )
                            })?
                            .to_string();

                        let guard = store.lock().await;
                        let value = guard.get(&key).cloned();
                        drop(guard);

                        Ok(json!({ "value": value }))
                    }
                }),
            );
        }

        // ── storage.set ───────────────────────────────────────────────────────
        {
            let store = store.clone();
            dispatcher.register(
                "storage.set",
                make_handler(move |params: Value| {
                    let store = store.clone();
                    async move {
                        let key = params
                            .get("key")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'key' (string) is required",
                                )
                            })?
                            .to_string();
                        let value = params
                            .get("value")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'value' (string) is required",
                                )
                            })?
                            .to_string();

                        let mut guard = store.lock().await;
                        guard.set(key, value).map_err(|e| {
                            RpcErrorPayload::new(error_codes::INTERNAL_ERROR, e)
                        })?;
                        drop(guard);

                        Ok(json!({}))
                    }
                }),
            );
        }

        // ── storage.remove ────────────────────────────────────────────────────
        {
            let store = store.clone();
            dispatcher.register(
                "storage.remove",
                make_handler(move |params: Value| {
                    let store = store.clone();
                    async move {
                        let key = params
                            .get("key")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'key' (string) is required",
                                )
                            })?
                            .to_string();

                        let mut guard = store.lock().await;
                        guard.remove(&key).map_err(|e| {
                            RpcErrorPayload::new(error_codes::INTERNAL_ERROR, e)
                        })?;
                        drop(guard);

                        Ok(json!({}))
                    }
                }),
            );
        }
    }

    fn declare_permissions(&self, engine: &mut PermissionEngine) {
        engine.require("storage.get", PermissionKey::Storage);
        engine.require("storage.set", PermissionKey::Storage);
        engine.require("storage.remove", PermissionKey::Storage);
    }
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

    fn make_dispatcher_with_storage(sandbox: &TempDir) -> crate::rpc::dispatcher::Dispatcher {
        let module = StorageModule::new(sandbox.path().to_path_buf());
        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                storage: true,
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        dispatcher.with_engine(Arc::new(engine))
    }

    // ── Store unit tests ─────────────────────────────────────────────────────

    #[test]
    fn store_get_returns_none_for_missing_key() {
        let dir = make_sandbox();
        let store = Store::load(dir.path().join("storage.json")).unwrap();
        assert!(store.get("missing").is_none());
    }

    #[test]
    fn store_set_and_get_round_trip() {
        let dir = make_sandbox();
        let mut store = Store::load(dir.path().join("storage.json")).unwrap();
        store.set("theme".into(), "dark".into()).unwrap();
        assert_eq!(store.get("theme").unwrap(), "dark");
    }

    #[test]
    fn store_remove_deletes_key() {
        let dir = make_sandbox();
        let mut store = Store::load(dir.path().join("storage.json")).unwrap();
        store.set("x".into(), "1".into()).unwrap();
        store.remove("x").unwrap();
        assert!(store.get("x").is_none());
    }

    #[test]
    fn store_persists_to_disk_and_reloads() {
        let dir = make_sandbox();
        let path = dir.path().join("storage.json");

        {
            let mut store = Store::load(path.clone()).unwrap();
            store.set("color".into(), "blue".into()).unwrap();
            store.set("size".into(), "large".into()).unwrap();
        }

        // Reload from disk — simulates app restart.
        let store = Store::load(path).unwrap();
        assert_eq!(store.get("color").unwrap(), "blue");
        assert_eq!(store.get("size").unwrap(), "large");
    }

    #[test]
    fn store_remove_does_not_panic_on_missing_key() {
        let dir = make_sandbox();
        let mut store = Store::load(dir.path().join("storage.json")).unwrap();
        // Removing a non-existent key must succeed silently.
        assert!(store.remove("never_set").is_ok());
    }

    // ── RPC handler round-trip ────────────────────────────────────────────────

    #[tokio::test]
    async fn storage_get_set_remove_round_trip_via_rpc() {
        let dir = make_sandbox();
        let dispatcher = make_dispatcher_with_storage(&dir);

        // Set.
        let set_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "storage.set".into(),
                params: json!({ "key": "theme", "value": "dark" }),
            })
            .await;
        assert!(set_resp.is_ok(), "storage.set must succeed");

        // Get.
        let get_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 2,
                method: "storage.get".into(),
                params: json!({ "key": "theme" }),
            })
            .await;
        assert!(get_resp.is_ok(), "storage.get must succeed");
        if let crate::rpc::schema::RpcResponse::Success { result, .. } = get_resp {
            assert_eq!(result["value"], "dark");
        }

        // Remove.
        let rem_resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 3,
                method: "storage.remove".into(),
                params: json!({ "key": "theme" }),
            })
            .await;
        assert!(rem_resp.is_ok(), "storage.remove must succeed");

        // Get after remove — must return null value.
        let get_after_rem = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 4,
                method: "storage.get".into(),
                params: json!({ "key": "theme" }),
            })
            .await;
        assert!(get_after_rem.is_ok());
        if let crate::rpc::schema::RpcResponse::Success { result, .. } = get_after_rem {
            assert!(result["value"].is_null(), "value must be null after remove");
        }
    }

    #[tokio::test]
    async fn storage_get_missing_key_returns_null_value() {
        let dir = make_sandbox();
        let dispatcher = make_dispatcher_with_storage(&dir);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "storage.get".into(),
                params: json!({ "key": "not_set" }),
            })
            .await;
        assert!(resp.is_ok());
        if let crate::rpc::schema::RpcResponse::Success { result, .. } = resp {
            assert!(result["value"].is_null());
        }
    }

    #[tokio::test]
    async fn storage_values_persist_across_module_reinit() {
        let dir = make_sandbox();

        // First instance: write a value.
        {
            let dispatcher = make_dispatcher_with_storage(&dir);
            dispatcher
                .dispatch(crate::rpc::schema::RpcRequest {
                    id: 1,
                    method: "storage.set".into(),
                    params: json!({ "key": "lang", "value": "en" }),
                })
                .await;
        }

        // Second instance: reload from the same directory — simulates restart.
        {
            let dispatcher = make_dispatcher_with_storage(&dir);
            let resp = dispatcher
                .dispatch(crate::rpc::schema::RpcRequest {
                    id: 2,
                    method: "storage.get".into(),
                    params: json!({ "key": "lang" }),
                })
                .await;
            assert!(resp.is_ok());
            if let crate::rpc::schema::RpcResponse::Success { result, .. } = resp {
                assert_eq!(result["value"], "en", "value must survive module reinit");
            }
        }
    }

    #[tokio::test]
    async fn storage_denied_when_permission_not_granted() {
        let dir = make_sandbox();
        let module = StorageModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions::default(), // storage NOT granted
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "storage.get".into(),
                params: json!({ "key": "x" }),
            })
            .await;
        assert!(resp.is_err());
        if let crate::rpc::schema::RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, crate::rpc::schema::error_codes::PERMISSION_DENIED);
        }
    }

    #[tokio::test]
    async fn concurrent_writes_do_not_corrupt_data() {
        use tokio::task::JoinSet;

        let dir = make_sandbox();
        let module = StorageModule::new(dir.path().to_path_buf());

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                storage: true,
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        let dispatcher = Arc::new(dispatcher.with_engine(Arc::new(engine)));

        // Spawn 10 concurrent writes.
        let mut set = JoinSet::new();
        for i in 0..10u32 {
            let d = dispatcher.clone();
            set.spawn(async move {
                d.dispatch(crate::rpc::schema::RpcRequest {
                    id: i as u64,
                    method: "storage.set".into(),
                    params: json!({ "key": format!("key_{i}"), "value": format!("val_{i}") }),
                })
                .await
            });
        }

        // All writes must succeed.
        while let Some(result) = set.join_next().await {
            let resp = result.unwrap();
            assert!(resp.is_ok(), "concurrent storage.set must succeed");
        }

        // Verify all keys are present.
        for i in 0..10u32 {
            let resp = dispatcher
                .dispatch(crate::rpc::schema::RpcRequest {
                    id: 100 + i as u64,
                    method: "storage.get".into(),
                    params: json!({ "key": format!("key_{i}") }),
                })
                .await;
            assert!(resp.is_ok());
            if let crate::rpc::schema::RpcResponse::Success { result, .. } = resp {
                assert_eq!(
                    result["value"],
                    format!("val_{i}"),
                    "key_{i} must survive concurrent writes"
                );
            }
        }
    }
}
