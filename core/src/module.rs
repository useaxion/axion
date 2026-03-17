/// `AxionModule` trait and `ModuleRegistry` — issue #12.
///
/// Every built-in native module implements `AxionModule`. The `ModuleRegistry`
/// collects all modules and initialises them at startup by calling
/// `register_handlers` and `declare_permissions` on each one in order.
///
/// # Isolation guarantee
///
/// Modules receive mutable references to the shared `Dispatcher` and
/// `PermissionEngine` only during the `init` call. After startup the
/// dispatcher and engine are shared as `Arc<_>` and modules never hold a
/// reference to each other — all inter-module communication must go through
/// RPC or the event bus.
use crate::permissions::engine::PermissionEngine;
use crate::rpc::dispatcher::Dispatcher;

// ── Trait ─────────────────────────────────────────────────────────────────────

/// The interface every Axion native module must implement.
///
/// # Contract
///
/// - [`name`] returns a stable, unique identifier for logging and debugging.
/// - [`register_handlers`] registers this module's RPC handlers with the
///   dispatcher. Called exactly once at startup.
/// - [`declare_permissions`] registers the `method → PermissionKey` mappings
///   this module needs. Called exactly once at startup.
///
/// Both methods receive mutable references and are called *before* the
/// dispatcher or engine are shared across threads, so `&mut` is safe.
///
/// # Example
///
/// ```rust,ignore
/// pub struct EchoModule;
///
/// impl AxionModule for EchoModule {
///     fn name(&self) -> &'static str { "echo" }
///
///     fn register_handlers(&self, dispatcher: &mut Dispatcher) {
///         dispatcher.register("echo.ping", make_handler(|_| async { Ok(json!("pong")) }));
///     }
///
///     fn declare_permissions(&self, _engine: &mut PermissionEngine) {
///         // echo requires no permissions
///     }
/// }
/// ```
pub trait AxionModule: Send + Sync {
    /// Stable, unique module identifier, e.g. `"fs"` or `"storage"`.
    ///
    /// Used in logs and diagnostics. Must not contain spaces or dots.
    fn name(&self) -> &'static str;

    /// Register this module's async RPC handlers with `dispatcher`.
    ///
    /// Called once during [`ModuleRegistry::init`]. The dispatcher is not yet
    /// shared at this point so `&mut` access is exclusive.
    fn register_handlers(&self, dispatcher: &mut Dispatcher);

    /// Declare the `method → PermissionKey` requirements for this module.
    ///
    /// Called once during [`ModuleRegistry::init`], after `register_handlers`.
    /// The engine is not yet shared at this point so `&mut` access is exclusive.
    fn declare_permissions(&self, engine: &mut PermissionEngine);
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Holds all built-in [`AxionModule`] instances and initialises them at
/// startup.
///
/// # Usage
///
/// ```rust,ignore
/// let registry = ModuleRegistry::new();
/// registry.init(&mut dispatcher, &mut engine);
/// ```
///
/// After `init`, the dispatcher and engine can be wrapped in `Arc` and shared
/// across threads for the lifetime of the runtime.
pub struct ModuleRegistry {
    modules: Vec<Box<dyn AxionModule>>,
}

impl ModuleRegistry {
    /// Create an empty registry.
    ///
    /// Call [`register`](Self::register) to add modules before [`init`](Self::init).
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Add a module to the registry.
    ///
    /// Modules are initialised in insertion order.
    pub fn register(&mut self, module: impl AxionModule + 'static) {
        self.modules.push(Box::new(module));
    }

    /// Initialise all registered modules.
    ///
    /// For each module, in insertion order:
    /// 1. Calls [`AxionModule::register_handlers`] to wire up RPC handlers.
    /// 2. Calls [`AxionModule::declare_permissions`] to register permission
    ///    requirements.
    ///
    /// After this call the `dispatcher` and `engine` are fully configured and
    /// ready to be wrapped in `Arc` and shared.
    pub fn init(&self, dispatcher: &mut Dispatcher, engine: &mut PermissionEngine) {
        for module in &self.modules {
            module.register_handlers(dispatcher);
            module.declare_permissions(engine);
        }
    }

    /// Return the number of registered modules.
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    /// Return module names in registration order.
    pub fn module_names(&self) -> Vec<&'static str> {
        self.modules.iter().map(|m| m.name()).collect()
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::permissions::engine::PermissionKey;
    use crate::permissions::Permissions;
    use crate::rpc::dispatcher::make_handler;
    use crate::rpc::schema::RpcRequest;

    // ── Stub module ───────────────────────────────────────────────────────────

    /// A minimal module used to verify the trait and registry.
    struct EchoModule;

    impl AxionModule for EchoModule {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn register_handlers(&self, dispatcher: &mut Dispatcher) {
            dispatcher.register("echo.ping", make_handler(|_| async { Ok(json!("pong")) }));
        }

        fn declare_permissions(&self, engine: &mut PermissionEngine) {
            // echo requires no permissions — nothing to declare.
            let _ = engine; // suppress unused warning
        }
    }

    /// A module that declares a storage permission.
    struct StorageStubModule;

    impl AxionModule for StorageStubModule {
        fn name(&self) -> &'static str {
            "storage_stub"
        }

        fn register_handlers(&self, dispatcher: &mut Dispatcher) {
            dispatcher.register(
                "storage_stub.get",
                make_handler(|_| async { Ok(json!({ "value": "hello" })) }),
            );
        }

        fn declare_permissions(&self, engine: &mut PermissionEngine) {
            engine.require("storage_stub.get", PermissionKey::Storage);
        }
    }

    // ── Registry ─────────────────────────────────────────────────────────────

    #[test]
    fn empty_registry_has_zero_modules() {
        let registry = ModuleRegistry::new();
        assert_eq!(registry.module_count(), 0);
    }

    #[test]
    fn register_increments_module_count() {
        let mut registry = ModuleRegistry::new();
        registry.register(EchoModule);
        assert_eq!(registry.module_count(), 1);
        registry.register(StorageStubModule);
        assert_eq!(registry.module_count(), 2);
    }

    #[test]
    fn module_names_are_returned_in_insertion_order() {
        let mut registry = ModuleRegistry::new();
        registry.register(EchoModule);
        registry.register(StorageStubModule);
        assert_eq!(registry.module_names(), vec!["echo", "storage_stub"]);
    }

    // ── init() ────────────────────────────────────────────────────────────────

    #[test]
    fn init_registers_handlers_in_dispatcher() {
        let mut registry = ModuleRegistry::new();
        registry.register(EchoModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions::default());

        registry.init(&mut dispatcher, &mut engine);

        // Trying to register the same method again must fail (already exists).
        let registered = dispatcher.register(
            "echo.ping",
            make_handler(|_| async { Ok(json!("duplicate")) }),
        );
        assert!(
            !registered,
            "echo.ping must already be registered after init()"
        );
    }

    #[test]
    fn init_declares_permissions_in_engine() {
        let mut registry = ModuleRegistry::new();
        registry.register(StorageStubModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions {
            storage: true,
            ..Default::default()
        });

        registry.init(&mut dispatcher, &mut engine);

        // The engine must have the method registered (registered_count > 0).
        assert_eq!(engine.registered_count(), 1);
        // The method is allowed because storage is granted.
        assert!(engine.check("storage_stub.get").is_ok());
    }

    #[test]
    fn init_processes_all_modules() {
        let mut registry = ModuleRegistry::new();
        registry.register(EchoModule);
        registry.register(StorageStubModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions {
            storage: true,
            ..Default::default()
        });

        registry.init(&mut dispatcher, &mut engine);

        // Both modules' handlers must be registered.
        assert!(!dispatcher.register("echo.ping", make_handler(|_| async { Ok(json!(null)) })),
            "echo.ping must be registered");
        assert!(!dispatcher.register("storage_stub.get", make_handler(|_| async { Ok(json!(null)) })),
            "storage_stub.get must be registered");

        // storage_stub permission must be declared.
        assert_eq!(engine.registered_count(), 1);
    }

    // ── End-to-end: module registered and callable ────────────────────────────

    #[tokio::test]
    async fn module_registered_via_registry_is_callable() {
        let mut registry = ModuleRegistry::new();
        registry.register(EchoModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions::default());
        registry.init(&mut dispatcher, &mut engine);

        // echo has no permission requirement — dispatcher without engine works.
        let req = RpcRequest {
            id: 1,
            method: "echo.ping".into(),
            params: json!({}),
        };
        let resp = dispatcher.dispatch(req).await;

        assert!(resp.is_ok(), "echo.ping must return a success response");
        use crate::rpc::schema::RpcResponse;
        if let RpcResponse::Success { result, .. } = resp {
            assert_eq!(result, json!("pong"));
        }
    }

    #[tokio::test]
    async fn module_with_granted_permission_is_callable_end_to_end() {
        use std::sync::Arc;
        use crate::rpc::schema::RpcResponse;

        let mut registry = ModuleRegistry::new();
        registry.register(StorageStubModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions {
            storage: true,
            ..Default::default()
        });
        registry.init(&mut dispatcher, &mut engine);

        // Wrap engine in Arc and attach to dispatcher.
        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let req = RpcRequest {
            id: 2,
            method: "storage_stub.get".into(),
            params: json!({ "key": "theme" }),
        };
        let resp = dispatcher.dispatch(req).await;

        assert!(resp.is_ok(), "storage_stub.get must succeed with permission granted");
        if let RpcResponse::Success { result, .. } = resp {
            assert_eq!(result["value"], "hello");
        }
    }

    #[tokio::test]
    async fn module_with_missing_permission_is_denied_end_to_end() {
        use std::sync::Arc;
        use crate::rpc::schema::{error_codes, RpcResponse};

        let mut registry = ModuleRegistry::new();
        registry.register(StorageStubModule);

        let mut dispatcher = Dispatcher::new();
        let mut engine = PermissionEngine::new(Permissions::default()); // storage NOT granted
        registry.init(&mut dispatcher, &mut engine);

        let dispatcher = dispatcher.with_engine(Arc::new(engine));

        let req = RpcRequest {
            id: 3,
            method: "storage_stub.get".into(),
            params: json!({ "key": "theme" }),
        };
        let resp = dispatcher.dispatch(req).await;

        assert!(resp.is_err(), "must be denied when storage not granted");
        if let RpcResponse::Error { error, .. } = resp {
            assert_eq!(error.code, error_codes::PERMISSION_DENIED);
        }
    }
}
