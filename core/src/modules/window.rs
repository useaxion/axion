/// `window` native module — native window control (issue #17).
///
/// No permission required — window control is implicit for any Axion app.
///
/// Methods:
/// - `window.minimize`  — minimize to taskbar
/// - `window.maximize`  — maximize / restore (toggle)
/// - `window.close`     — close and clean up Tokio runtime
/// - `window.setTitle`  — set window title bar text
///
/// The module shares the native window handle (HWND) via an `Arc<WindowHandle>`.
/// In tests or environments without a real window, the handle is null and
/// operations return a graceful error instead of panicking.
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::module::AxionModule;
use crate::permissions::engine::PermissionEngine;
use crate::rpc::dispatcher::{make_handler, Dispatcher};
use crate::rpc::schema::{error_codes, RpcErrorPayload};

// ── Window handle wrapper ─────────────────────────────────────────────────────

/// A thread-safe wrapper around a native window handle.
///
/// Wraps the platform-specific handle so that:
/// - `None` means no window is present (graceful fallback for tests).
/// - `Some(hwnd)` holds the Win32 `HWND` as a `isize` (platform-neutral type).
///
/// Sharing across threads is safe because Win32 window messages are
/// dispatched from the main thread and the HWND value itself is a pointer-sized
/// integer (not a mutable reference).
pub struct WindowHandle {
    /// The HWND as a pointer-sized integer. `0` means no window.
    hwnd: isize,
    /// Tokio `Notify` used by `window.close` to signal the runtime to shut down.
    shutdown: Arc<Notify>,
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

impl WindowHandle {
    /// Create a `WindowHandle` from a Win32 HWND.
    ///
    /// # Safety
    ///
    /// `hwnd` must be a valid Win32 window handle for the lifetime of the
    /// `WindowHandle`. The caller is responsible for ensuring the HWND is not
    /// destroyed while this wrapper is alive.
    pub fn new(hwnd: isize, shutdown: Arc<Notify>) -> Self {
        Self { hwnd, shutdown }
    }

    /// Create a null `WindowHandle` for use in tests or pre-window contexts.
    pub fn null() -> Self {
        Self {
            hwnd: 0,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Returns `true` if the handle refers to a real window.
    pub fn is_valid(&self) -> bool {
        self.hwnd != 0
    }
}

// ── WindowModule ──────────────────────────────────────────────────────────────

/// The `window` module.
///
/// Construct with a shared `Arc<WindowHandle>` that references the app's
/// native window. The handle must outlive the `Dispatcher`.
pub struct WindowModule {
    handle: Arc<WindowHandle>,
}

impl WindowModule {
    /// Create a `WindowModule` backed by `handle`.
    pub fn new(handle: Arc<WindowHandle>) -> Self {
        Self { handle }
    }

    /// Create a `WindowModule` with a null handle (for tests / pre-window use).
    pub fn null() -> Self {
        Self::new(Arc::new(WindowHandle::null()))
    }
}

impl AxionModule for WindowModule {
    fn name(&self) -> &'static str {
        "window"
    }

    fn register_handlers(&self, dispatcher: &mut Dispatcher) {
        let handle = self.handle.clone();

        // ── window.minimize ───────────────────────────────────────────────────
        {
            let handle = handle.clone();
            dispatcher.register(
                "window.minimize",
                make_handler(move |_params: Value| {
                    let handle = handle.clone();
                    async move { window_op(&handle, WindowOp::Minimize) }
                }),
            );
        }

        // ── window.maximize ───────────────────────────────────────────────────
        {
            let handle = handle.clone();
            dispatcher.register(
                "window.maximize",
                make_handler(move |_params: Value| {
                    let handle = handle.clone();
                    async move { window_op(&handle, WindowOp::Maximize) }
                }),
            );
        }

        // ── window.close ─────────────────────────────────────────────────────
        {
            let handle = handle.clone();
            dispatcher.register(
                "window.close",
                make_handler(move |_params: Value| {
                    let handle = handle.clone();
                    async move {
                        let result = window_op(&handle, WindowOp::Close);
                        // Signal clean Tokio shutdown regardless of whether the
                        // Win32 call succeeded (e.g. in tests without a real window).
                        handle.shutdown.notify_one();
                        result
                    }
                }),
            );
        }

        // ── window.setTitle ───────────────────────────────────────────────────
        {
            let handle = handle.clone();
            dispatcher.register(
                "window.setTitle",
                make_handler(move |params: Value| {
                    let handle = handle.clone();
                    async move {
                        let title = params
                            .get("title")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                RpcErrorPayload::new(
                                    error_codes::INVALID_PARAMS,
                                    "'title' (string) is required",
                                )
                            })?
                            .to_string();
                        window_op(&handle, WindowOp::SetTitle(title))
                    }
                }),
            );
        }
    }

    /// `window` methods require no permissions.
    fn declare_permissions(&self, _engine: &mut PermissionEngine) {}
}

// ── Window operations ─────────────────────────────────────────────────────────

enum WindowOp {
    Minimize,
    Maximize,
    Close,
    SetTitle(String),
}

fn window_op(handle: &WindowHandle, op: WindowOp) -> Result<Value, RpcErrorPayload> {
    if !handle.is_valid() {
        // Graceful degradation: no window available (tests / headless mode).
        return Err(RpcErrorPayload::new(
            error_codes::INTERNAL_ERROR,
            "window operation unavailable: no native window handle",
        ));
    }

    #[cfg(windows)]
    {
        do_window_op_windows(handle.hwnd, op)
    }
    #[cfg(not(windows))]
    {
        let _ = op;
        Err(RpcErrorPayload::new(
            error_codes::INTERNAL_ERROR,
            "window operations are only supported on Windows",
        ))
    }
}

#[cfg(windows)]
fn do_window_op_windows(hwnd: isize, op: WindowOp) -> Result<Value, RpcErrorPayload> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, PostMessageW, SetWindowTextW, ShowWindow,
        SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, WM_CLOSE,
    };

    let hwnd = HWND(hwnd);

    match op {
        WindowOp::Minimize => unsafe {
            ShowWindow(hwnd, SW_MINIMIZE);
            Ok(json!({}))
        },
        WindowOp::Maximize => unsafe {
            // Toggle between maximize and restore.
            let cmd = if IsZoomed(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_MAXIMIZE
            };
            ShowWindow(hwnd, cmd);
            Ok(json!({}))
        },
        WindowOp::Close => unsafe {
            // Post WM_CLOSE so the message loop can clean up before exit.
            let _ = PostMessageW(hwnd, WM_CLOSE, None, None);
            Ok(json!({}))
        },
        WindowOp::SetTitle(title) => unsafe {
            use windows::core::HSTRING;
            SetWindowTextW(hwnd, &HSTRING::from(title))
                .map_err(|e| {
                    RpcErrorPayload::new(
                        error_codes::INTERNAL_ERROR,
                        format!("window.setTitle failed: {e}"),
                    )
                })?;
            Ok(json!({}))
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_dispatcher(with_handle: bool) -> crate::rpc::dispatcher::Dispatcher {
        let module = if with_handle {
            WindowModule::new(Arc::new(WindowHandle::new(0, Arc::new(Notify::new()))))
        } else {
            WindowModule::null()
        };
        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions::default(),
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        dispatcher
    }

    #[test]
    fn module_name_is_window() {
        assert_eq!(WindowModule::null().name(), "window");
    }

    #[test]
    fn window_module_declares_no_permissions() {
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions::default(),
        );
        WindowModule::null().declare_permissions(&mut engine);
        assert_eq!(
            engine.registered_count(),
            0,
            "window module must not declare any permissions"
        );
    }

    #[test]
    fn null_handle_is_not_valid() {
        assert!(!WindowHandle::null().is_valid());
    }

    #[test]
    fn non_zero_handle_is_valid() {
        let h = WindowHandle::new(1, Arc::new(Notify::new()));
        assert!(h.is_valid());
    }

    /// All four methods must be callable without panic.
    /// With no real window, they return a graceful INTERNAL_ERROR.
    #[tokio::test]
    async fn all_methods_callable_without_panic_no_window() {
        let dispatcher = make_dispatcher(false); // null handle

        for method in ["window.minimize", "window.maximize", "window.close"] {
            let resp = dispatcher
                .dispatch(crate::rpc::schema::RpcRequest {
                    id: 1,
                    method: method.into(),
                    params: json!({}),
                })
                .await;

            // Must not panic. With null handle, returns graceful error.
            match resp {
                crate::rpc::schema::RpcResponse::Error { error, .. } => {
                    assert_ne!(
                        error.code,
                        crate::rpc::schema::error_codes::PERMISSION_DENIED,
                        "{method} must not return PERMISSION_DENIED"
                    );
                    assert_eq!(
                        error.code,
                        crate::rpc::schema::error_codes::INTERNAL_ERROR,
                        "{method} must return INTERNAL_ERROR when no window"
                    );
                }
                crate::rpc::schema::RpcResponse::Success { .. } => {
                    // Succeeded — valid on a real desktop.
                }
            }
        }
    }

    #[tokio::test]
    async fn set_title_missing_param_returns_invalid_params() {
        let dispatcher = make_dispatcher(false);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 2,
                method: "window.setTitle".into(),
                params: json!({}), // missing 'title'
            })
            .await;

        assert!(resp.is_err());
        if let crate::rpc::schema::RpcResponse::Error { error, .. } = resp {
            assert_eq!(
                error.code,
                crate::rpc::schema::error_codes::INVALID_PARAMS
            );
        }
    }

    #[tokio::test]
    async fn window_close_triggers_shutdown_notify() {
        let shutdown = Arc::new(Notify::new());
        let handle = Arc::new(WindowHandle {
            hwnd: 0, // no real window
            shutdown: shutdown.clone(),
        });
        let module = WindowModule::new(handle);

        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions::default(),
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);

        // Spawn a task that waits for the shutdown notification.
        let shutdown_clone = shutdown.clone();
        let waiter = tokio::spawn(async move {
            shutdown_clone.notified().await;
            true // received
        });

        // Dispatch window.close — should notify the shutdown watcher.
        dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 3,
                method: "window.close".into(),
                params: json!({}),
            })
            .await;

        // The shutdown notifier must have been triggered.
        let received = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            waiter,
        )
        .await;

        assert!(
            received.is_ok(),
            "window.close must trigger the shutdown Notify within 500ms"
        );
    }
}
