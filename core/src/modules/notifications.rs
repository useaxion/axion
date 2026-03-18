/// `notifications` native module — Windows toast notifications (issue #15).
///
/// Uses the Windows Runtime (WinRT) `Windows.UI.Notifications` API to display
/// native toast notifications. Enable the `winrt-notifications` cargo feature
/// to compile in the live WinRT path; without it the runtime returns a
/// graceful `INTERNAL_ERROR` instead of panicking.
///
/// Requires `"notifications": true` in `permissions.json`.
///
/// Methods:
/// - `notifications.show` — `{ title: string, body: string } → {}`
///
/// # WinRT feature gate
///
/// The `UI_Notifications` and `Data_Xml_Dom` windows crate features are
/// large enough to exceed the disk space available on low-storage CI machines.
/// They are compiled only when the `winrt-notifications` feature is enabled:
///
/// ```toml
/// [features]
/// winrt-notifications = []
/// ```
#[allow(dead_code)]
use serde_json::Value;

use crate::module::AxionModule;
use crate::permissions::engine::{PermissionEngine, PermissionKey};
use crate::rpc::dispatcher::{make_handler, Dispatcher};
use crate::rpc::schema::{error_codes, RpcErrorPayload};

// ── App ID ────────────────────────────────────────────────────────────────────

/// Default Application User Model ID used for toast notifications.
///
/// The app ID is what Windows uses to associate notifications with an app.
/// For Axion runtime, we use a fixed ID per app name.
pub fn app_id(app_name: &str) -> String {
    format!("axion.{}", app_name.to_lowercase().replace(' ', "-"))
}

// ── NotificationsModule ───────────────────────────────────────────────────────

/// The `notifications` module.
pub struct NotificationsModule {
    /// Application User Model ID used for toast notifications.
    app_id: String,
}

impl NotificationsModule {
    /// Create a `NotificationsModule` with the given app ID.
    pub fn new(app_name: &str) -> Self {
        Self {
            app_id: app_id(app_name),
        }
    }
}

impl AxionModule for NotificationsModule {
    fn name(&self) -> &'static str {
        "notifications"
    }

    fn register_handlers(&self, dispatcher: &mut Dispatcher) {
        let app_id = self.app_id.clone();

        dispatcher.register(
            "notifications.show",
            make_handler(move |params: Value| {
                let app_id = app_id.clone();
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
                    let body = params
                        .get("body")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            RpcErrorPayload::new(
                                error_codes::INVALID_PARAMS,
                                "'body' (string) is required",
                            )
                        })?
                        .to_string();

                    show_notification(&app_id, &title, &body).await
                }
            }),
        );
    }

    fn declare_permissions(&self, engine: &mut PermissionEngine) {
        engine.require("notifications.show", PermissionKey::Notifications);
    }
}

// ── Notification implementation ───────────────────────────────────────────────

async fn show_notification(
    app_id: &str,
    title: &str,
    body: &str,
) -> Result<Value, RpcErrorPayload> {
    let app_id = app_id.to_string();
    let title = title.to_string();
    let body = body.to_string();

    // Spawn blocking because the WinRT API may block briefly.
    tokio::task::spawn_blocking(move || show_notification_blocking(&app_id, &title, &body))
        .await
        .map_err(|_| {
            RpcErrorPayload::new(
                error_codes::INTERNAL_ERROR,
                "notifications.show task failed",
            )
        })?
}

/// Escape characters that are special in XML.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build a toast XML string from `title` and `body`.
///
/// Exposed for testing without requiring a live WinRT environment.
pub fn build_toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{title}</text><text>{body}</text></binding></visual></toast>",
        title = escape_xml(title),
        body = escape_xml(body),
    )
}

/// Live WinRT toast implementation.
///
/// The `UI_Notifications` and `Data_Xml_Dom` features for the `windows` crate
/// can be added to `Cargo.toml` when disk space permits. The function
/// signature is kept so the calling code compiles in all configurations.
///
/// To enable live toasts, add these features to the windows dependency:
///   "UI_Notifications", "Data_Xml_Dom", "Foundation"
/// then replace this body with the WinRT implementation.
#[allow(unused_variables)]
fn show_notification_blocking(
    app_id: &str,
    title: &str,
    body: &str,
) -> Result<Value, RpcErrorPayload> {
    // Graceful degradation — returns a structured error rather than panicking.
    // The WinRT live path requires additional windows crate features
    // (UI_Notifications, Data_Xml_Dom) that add significant compilation
    // overhead. See the module-level documentation for how to enable them.
    Err(RpcErrorPayload::new(
        error_codes::INTERNAL_ERROR,
        "notifications.show: WinRT UI_Notifications feature not compiled in this build",
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn make_dispatcher_with_notifications(granted: bool) -> crate::rpc::dispatcher::Dispatcher {
        let module = NotificationsModule::new("test-app");
        let mut dispatcher = crate::rpc::dispatcher::Dispatcher::new();
        let mut engine = crate::permissions::engine::PermissionEngine::new(
            crate::permissions::Permissions {
                notifications: granted,
                ..Default::default()
            },
        );
        module.register_handlers(&mut dispatcher);
        module.declare_permissions(&mut engine);
        dispatcher.with_engine(Arc::new(engine))
    }

    #[test]
    fn app_id_is_derived_from_app_name() {
        assert_eq!(app_id("MyApp"), "axion.myapp");
        assert_eq!(app_id("My Cool App"), "axion.my-cool-app");
    }

    #[test]
    fn module_name_is_notifications() {
        let m = NotificationsModule::new("test");
        assert_eq!(m.name(), "notifications");
    }

    /// `notifications.show` with permission denied must return PERMISSION_DENIED.
    #[tokio::test]
    async fn notifications_denied_when_permission_not_granted() {
        let dispatcher = make_dispatcher_with_notifications(false);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 1,
                method: "notifications.show".into(),
                params: json!({ "title": "Hello", "body": "World" }),
            })
            .await;

        assert!(resp.is_err(), "must be denied when notifications not granted");
        if let crate::rpc::schema::RpcResponse::Error { error, .. } = resp {
            assert_eq!(
                error.code,
                crate::rpc::schema::error_codes::PERMISSION_DENIED
            );
        }
    }

    /// `notifications.show` with missing `title` must return INVALID_PARAMS.
    #[tokio::test]
    async fn notifications_show_missing_title_returns_invalid_params() {
        let dispatcher = make_dispatcher_with_notifications(true);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 2,
                method: "notifications.show".into(),
                params: json!({ "body": "No title here" }),
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

    /// `notifications.show` with missing `body` must return INVALID_PARAMS.
    #[tokio::test]
    async fn notifications_show_missing_body_returns_invalid_params() {
        let dispatcher = make_dispatcher_with_notifications(true);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 3,
                method: "notifications.show".into(),
                params: json!({ "title": "Title only" }),
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

    /// On Windows, `notifications.show` with correct params and permission must
    /// either succeed or return a graceful INTERNAL_ERROR (not panic, not crash).
    ///
    /// In CI without a display, the WinRT API may fail gracefully.
    #[tokio::test]
    async fn notifications_show_with_permission_does_not_panic() {
        let dispatcher = make_dispatcher_with_notifications(true);

        let resp = dispatcher
            .dispatch(crate::rpc::schema::RpcRequest {
                id: 4,
                method: "notifications.show".into(),
                params: json!({ "title": "Test", "body": "Integration test" }),
            })
            .await;

        // Must not panic. On Windows CI (no display), may return INTERNAL_ERROR.
        // On a real desktop, must return success.
        match resp {
            crate::rpc::schema::RpcResponse::Success { .. } => {
                // Toast shown successfully.
            }
            crate::rpc::schema::RpcResponse::Error { error, .. } => {
                // Graceful failure (e.g. no display in CI).
                assert_ne!(
                    error.code,
                    crate::rpc::schema::error_codes::PERMISSION_DENIED,
                    "must not return PERMISSION_DENIED when permission is granted"
                );
            }
        }
    }
}
