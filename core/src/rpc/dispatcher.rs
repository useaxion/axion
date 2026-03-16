// Public API — consumed by module registration and IPC bridge wiring.
#![allow(dead_code)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::permissions::engine::PermissionEngine;
use crate::rpc::schema::{error_codes, RpcErrorPayload, RpcRequest, RpcResponse};

// ── Handler types ─────────────────────────────────────────────────────────────

/// The return type of every RPC handler.
///
/// - `Ok(Value)` — serialized into a success `RpcResponse`.
/// - `Err(RpcErrorPayload)` — serialized into an error `RpcResponse`.
pub type HandlerResult = Result<Value, RpcErrorPayload>;

/// A boxed, async RPC handler function.
///
/// Use [`make_handler`] to construct one ergonomically.
pub type Handler =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = HandlerResult> + Send>> + Send + Sync + 'static>;

/// Wrap an async closure into a [`Handler`].
///
/// ```rust,ignore
/// dispatcher.register("system.info", make_handler(|_params| async move {
///     Ok(serde_json::json!({ "os": "Windows" }))
/// }));
/// ```
pub fn make_handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HandlerResult> + Send + 'static,
{
    Arc::new(move |params| Box::pin(f(params)))
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Routes incoming [`RpcRequest`] messages to registered async handlers.
///
/// Handlers are registered once at startup (by native modules) and then the
/// dispatcher is shared across threads via `Arc<Dispatcher>` for the lifetime
/// of the runtime.
///
/// # Permission enforcement
///
/// When a [`PermissionEngine`] is attached via [`with_engine`](Self::with_engine),
/// **every** dispatched request is checked against the engine before the handler
/// is invoked. A denied call returns a [`error_codes::PERMISSION_DENIED`] error
/// response — the handler is never reached. There is no bypass.
///
/// ```rust,ignore
/// let engine = Arc::new(PermissionEngine::load(Path::new("permissions.json"))?);
/// let dispatcher = Arc::new(Dispatcher::new().with_engine(engine));
/// ```
pub struct Dispatcher {
    handlers: Arc<RwLock<HashMap<String, Handler>>>,
    /// Optional permission engine. When `Some`, every `dispatch()` call is
    /// checked before the handler is invoked.
    engine: Option<Arc<PermissionEngine>>,
}

impl Dispatcher {
    /// Create an empty dispatcher with no registered handlers and no permission
    /// engine.
    ///
    /// Use [`with_engine`](Self::with_engine) to attach a [`PermissionEngine`]
    /// before the dispatcher is shared with the IPC bridge.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            engine: None,
        }
    }

    /// Attach a [`PermissionEngine`] to this dispatcher.
    ///
    /// Once set, every call to [`dispatch`](Self::dispatch) will invoke
    /// `engine.check(method)` before the handler. A denied call returns
    /// a [`error_codes::PERMISSION_DENIED`] error response immediately —
    /// the handler is never invoked.
    ///
    /// This is a consuming builder method — call it once during startup:
    ///
    /// ```rust,ignore
    /// let dispatcher = Dispatcher::new().with_engine(engine);
    /// ```
    pub fn with_engine(mut self, engine: Arc<PermissionEngine>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Register an async handler for `method`.
    ///
    /// Returns `true` on success. Returns `false` — without replacing the
    /// existing handler — if a handler is already registered for this method.
    /// This prevents modules from silently overwriting each other's handlers.
    pub fn register(&self, method: impl Into<String>, handler: Handler) -> bool {
        let method = method.into();
        let mut handlers = self.handlers.write().expect("dispatcher lock poisoned");
        if handlers.contains_key(&method) {
            return false;
        }
        handlers.insert(method, handler);
        true
    }

    /// Route an [`RpcRequest`] to the registered handler and return the response.
    ///
    /// - **Unregistered method** → `METHOD_NOT_FOUND` error response.
    /// - **Handler panic** → `INTERNAL_ERROR` error response (panic is caught
    ///   by the Tokio task boundary; the dispatcher itself never panics).
    /// - **Handler `Err`** → structured error response with the handler's payload.
    ///
    /// Requires an active Tokio runtime.
    pub async fn dispatch(&self, request: RpcRequest) -> RpcResponse {
        let id = request.id;
        let method = request.method.clone();

        // ── Permission check ──────────────────────────────────────────────────
        // Run before touching the handler registry. A denied call never reaches
        // the handler. There is no bypass for any environment.
        if let Some(engine) = &self.engine {
            if let Err(denied) = engine.check(&method) {
                return RpcResponse::error(
                    id,
                    error_codes::PERMISSION_DENIED,
                    denied.message,
                );
            }
        }

        let handler = {
            let handlers = self.handlers.read().expect("dispatcher lock poisoned");
            handlers.get(&method).cloned()
        };

        match handler {
            None => RpcResponse::error(
                id,
                error_codes::METHOD_NOT_FOUND,
                format!("Method not found: {method}"),
            ),

            Some(handler) => {
                let params = request.params;

                // Spawn a dedicated task so that a panic inside the handler is
                // caught at the JoinHandle boundary rather than unwinding the
                // dispatcher's thread.
                match tokio::task::spawn(async move { handler(params).await }).await {
                    Ok(Ok(result)) => RpcResponse::success(id, result),
                    Ok(Err(error)) => RpcResponse::Error { id, error },
                    // JoinError: task panicked or was cancelled.
                    Err(_join_err) => RpcResponse::error(
                        id,
                        error_codes::INTERNAL_ERROR,
                        "Internal error",
                    ),
                }
            }
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ── IPC bridge wiring ─────────────────────────────────────────────────────────

/// Wire a [`Dispatcher`] to an [`IpcBridge`].
///
/// After this call every message arriving from JavaScript is:
/// 1. Parsed as an [`RpcRequest`].
/// 2. Dispatched to the matching handler (async, on a Tokio task).
/// 3. Serialized and sent back as an [`RpcResponse`].
///
/// Parse failures return a `PARSE_ERROR` response with `id = 0`
/// (ID is unknown when the message cannot be decoded).
///
/// Requires an active Tokio runtime when messages arrive.
pub fn wire_to_bridge(
    bridge: Arc<crate::ipc::bridge::IpcBridge>,
    dispatcher: Arc<Dispatcher>,
) {
    let bridge_send = bridge.clone();
    bridge.on_message(move |raw| {
        let bridge_send = bridge_send.clone();
        let dispatcher = dispatcher.clone();

        tokio::spawn(async move {
            let response = match serde_json::from_str::<RpcRequest>(&raw) {
                Ok(req) => dispatcher.dispatch(req).await,
                Err(_) => {
                    // The request ID is unknown when parsing fails.
                    // Use 0 as a sentinel; documented in docs/rpc-schema.md.
                    RpcResponse::error(0, error_codes::PARSE_ERROR, "Parse error")
                }
            };

            // Serialization of RpcResponse should never fail for our types,
            // but we handle it gracefully rather than unwrapping.
            match serde_json::to_string(&response) {
                Ok(json) => {
                    let _ = bridge_send.send_to_js(json);
                }
                Err(_) => {
                    // Last-resort fallback: send a plain internal-error response.
                    let fallback = r#"{"id":0,"error":{"code":-32603,"message":"Internal error"}}"#;
                    let _ = bridge_send.send_to_js(fallback.to_string());
                }
            }
        });
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_dispatcher() -> Dispatcher {
        Dispatcher::new()
    }

    fn echo_handler() -> Handler {
        make_handler(|params| async move { Ok(params) })
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn register_returns_true_for_new_method() {
        let d = make_dispatcher();
        assert!(d.register("test.method", echo_handler()));
    }

    #[test]
    fn register_returns_false_for_duplicate_method() {
        let d = make_dispatcher();
        d.register("test.method", echo_handler());
        assert!(!d.register("test.method", echo_handler()),
            "duplicate registration must be rejected");
    }

    #[test]
    fn existing_handler_is_not_overwritten_on_duplicate() {
        let d = Arc::new(make_dispatcher());

        // First handler echoes params.
        d.register("test.op", make_handler(|_| async { Ok(json!("first")) }));
        // Second registration should be silently rejected.
        d.register("test.op", make_handler(|_| async { Ok(json!("second")) }));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = rt.block_on(async {
            let req = RpcRequest { id: 1, method: "test.op".into(), params: json!(null) };
            d.dispatch(req).await
        });

        assert_eq!(result, RpcResponse::success(1, json!("first")));
    }

    // ── Routing ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn registered_handler_is_called() {
        let d = make_dispatcher();
        d.register("echo", make_handler(|params| async move { Ok(params) }));

        let req = RpcRequest { id: 7, method: "echo".into(), params: json!({ "msg": "hi" }) };
        let resp = d.dispatch(req).await;

        assert_eq!(resp, RpcResponse::success(7, json!({ "msg": "hi" })));
    }

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let d = make_dispatcher();
        let req = RpcRequest { id: 1, method: "no.such.method".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_err());
        assert_eq!(resp.id(), 1);

        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::METHOD_NOT_FOUND);
            assert!(error.message.contains("no.such.method"));
        }
    }

    #[tokio::test]
    async fn handler_returning_err_produces_error_response() {
        let d = make_dispatcher();
        d.register(
            "failing.op",
            make_handler(|_| async {
                Err(RpcErrorPayload::new(
                    error_codes::INVALID_PARAMS,
                    "path is required",
                ))
            }),
        );

        let req = RpcRequest { id: 3, method: "failing.op".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_err());
        assert_eq!(resp.id(), 3);
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::INVALID_PARAMS);
        }
    }

    // ── Panic handling ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn panicking_handler_returns_internal_error() {
        let d = make_dispatcher();
        d.register(
            "panicky.op",
            make_handler(|_| async { panic!("simulated handler panic") }),
        );

        let req = RpcRequest { id: 99, method: "panicky.op".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_err(), "panic must produce an error response");
        assert_eq!(resp.id(), 99);
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::INTERNAL_ERROR);
            // Must not leak internal details like the panic message.
            assert!(!error.message.contains("simulated"));
        }
    }

    // ── Permission enforcement ────────────────────────────────────────────

    fn engine_allowing(method: &str, key: crate::permissions::engine::PermissionKey, permissions: crate::permissions::Permissions) -> Arc<PermissionEngine> {
        let mut engine = PermissionEngine::new(permissions);
        engine.require(method, key);
        Arc::new(engine)
    }

    #[tokio::test]
    async fn denied_method_returns_permission_denied_error() {
        use crate::permissions::engine::PermissionKey;
        use crate::permissions::Permissions;

        // Engine with storage granted, but fs is not.
        let mut engine = PermissionEngine::new(Permissions { storage: true, ..Default::default() });
        engine.require("fs.write", PermissionKey::FsAppData);
        engine.require("storage.get", PermissionKey::Storage);

        let d = Dispatcher::new().with_engine(Arc::new(engine));
        d.register("fs.write", make_handler(|_| async { Ok(json!("written")) }));

        let req = RpcRequest { id: 1, method: "fs.write".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_err());
        assert_eq!(resp.id(), 1);
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::PERMISSION_DENIED);
            assert!(error.message.contains("fs.appData"),
                "error message must name the missing permission");
        }
    }

    #[tokio::test]
    async fn granted_method_reaches_handler() {
        use crate::permissions::engine::PermissionKey;
        use crate::permissions::Permissions;

        let mut engine = PermissionEngine::new(Permissions { storage: true, ..Default::default() });
        engine.require("storage.get", PermissionKey::Storage);

        let d = Dispatcher::new().with_engine(Arc::new(engine));
        d.register("storage.get", make_handler(|_| async { Ok(json!("value")) }));

        let req = RpcRequest { id: 2, method: "storage.get".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_ok());
        assert_eq!(resp, RpcResponse::success(2, json!("value")));
    }

    #[tokio::test]
    async fn unregistered_method_returns_permission_denied_before_method_not_found() {
        use crate::permissions::Permissions;

        // Engine has no method registrations — everything is denied.
        let engine = PermissionEngine::new(Permissions { window: true, ..Default::default() });
        let d = Dispatcher::new().with_engine(Arc::new(engine));
        // No handler registered either.

        let req = RpcRequest { id: 3, method: "window.close".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        // Permission check fires first — method not in engine registry.
        assert!(resp.is_err());
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::PERMISSION_DENIED);
        }
    }

    #[tokio::test]
    async fn handler_is_never_called_when_permission_denied() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use crate::permissions::engine::PermissionKey;
        use crate::permissions::Permissions;

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let mut engine = PermissionEngine::new(Permissions::default()); // nothing granted
        engine.require("fs.read", PermissionKey::FsAppData);

        let d = Dispatcher::new().with_engine(Arc::new(engine));
        d.register("fs.read", make_handler(move |_| {
            let called = called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok(json!(null))
            }
        }));

        let req = RpcRequest { id: 4, method: "fs.read".into(), params: json!(null) };
        d.dispatch(req).await;

        assert!(!called.load(Ordering::SeqCst), "handler must never be called when permission is denied");
    }

    #[tokio::test]
    async fn dispatcher_without_engine_allows_all_registered_handlers() {
        // No engine attached — backward-compatible behaviour, used in tests
        // and before modules are registered at startup.
        let d = Dispatcher::new(); // no engine
        d.register("any.method", make_handler(|_| async { Ok(json!("ok")) }));

        let req = RpcRequest { id: 5, method: "any.method".into(), params: json!(null) };
        let resp = d.dispatch(req).await;

        assert!(resp.is_ok());
    }

    // ── IPC bridge wiring ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn wire_to_bridge_routes_request_and_sends_response() {
        use std::sync::Mutex;
        use crate::ipc::bridge::IpcBridge;

        let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent.clone();

        let bridge = Arc::new(IpcBridge::new(move |msg| {
            sent_clone.lock().unwrap().push(msg);
            Ok(())
        }));

        let dispatcher = Arc::new(Dispatcher::new());
        dispatcher.register("system.info", make_handler(|_| async {
            Ok(json!({ "os": "Windows" }))
        }));

        wire_to_bridge(bridge.clone(), dispatcher);

        let raw = r#"{"id":1,"method":"system.info","params":{}}"#;
        bridge.dispatch(raw.to_string());

        // Yield to let the spawned task complete.
        tokio::task::yield_now().await;

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);

        let resp: RpcResponse = serde_json::from_str(&messages[0]).unwrap();
        assert!(resp.is_ok());
        assert_eq!(resp.id(), 1);
    }

    #[tokio::test]
    async fn wire_to_bridge_returns_parse_error_for_invalid_json() {
        use std::sync::Mutex;
        use crate::ipc::bridge::IpcBridge;

        let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent.clone();

        let bridge = Arc::new(IpcBridge::new(move |msg| {
            sent_clone.lock().unwrap().push(msg);
            Ok(())
        }));

        wire_to_bridge(bridge.clone(), Arc::new(Dispatcher::new()));
        bridge.dispatch("this is not json".to_string());
        tokio::task::yield_now().await;

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);

        let resp: RpcResponse = serde_json::from_str(&messages[0]).unwrap();
        assert!(resp.is_err());
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::PARSE_ERROR);
        }
    }
}
