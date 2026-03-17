//! Integration tests for the Permission Engine — issue #11.
//!
//! Covers all permission scenarios end-to-end, including:
//! - Granted permission → RPC call succeeds
//! - Missing permission → `PERMISSION_DENIED` error code
//! - Missing `permissions.json` → startup failure with clear message
//! - Malformed `permissions.json` → startup failure with parse error details
//! - Granular `fs` permissions: `appData` granted but `absolutePath` not
//! - End-to-end: denied permission flows through the dispatcher to the RPC caller

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use axion_core::permissions::engine::{PermissionEngine, PermissionKey};
use axion_core::permissions::{FsPermissions, PermissionError, Permissions};
use axion_core::rpc::dispatcher::{make_handler, Dispatcher};
use axion_core::rpc::schema::{error_codes, RpcRequest, RpcResponse};
use serde_json::json;
use tempfile::NamedTempFile;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_temp(json: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(json.as_bytes()).unwrap();
    f
}

// ── Scenario 1: Granted permission → RPC call succeeds ───────────────────────

#[tokio::test]
async fn granted_permission_rpc_call_succeeds() {
    let mut engine = PermissionEngine::new(Permissions {
        storage: true,
        ..Default::default()
    });
    engine.require("storage.get", PermissionKey::Storage);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register(
        "storage.get",
        make_handler(|_| async { Ok(json!({ "value": "dark" })) }),
    );

    let req = RpcRequest {
        id: 1,
        method: "storage.get".into(),
        params: json!({ "key": "theme" }),
    };
    let resp = dispatcher.dispatch(req).await;

    assert!(resp.is_ok(), "granted permission must allow the RPC call");
    assert_eq!(resp.id(), 1);
    if let RpcResponse::Success { result, .. } = resp {
        assert_eq!(result["value"], "dark");
    }
}

// ── Scenario 2: Missing permission → PERMISSION_DENIED ───────────────────────

#[tokio::test]
async fn missing_permission_returns_permission_denied_error() {
    // Only storage is granted — fs is not.
    let mut engine = PermissionEngine::new(Permissions {
        storage: true,
        ..Default::default()
    });
    engine.require("fs.write", PermissionKey::FsAppData);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register(
        "fs.write",
        make_handler(|_| async { Ok(json!("written")) }),
    );

    let req = RpcRequest {
        id: 2,
        method: "fs.write".into(),
        params: json!({ "path": "notes.txt", "content": "hello" }),
    };
    let resp = dispatcher.dispatch(req).await;

    assert!(resp.is_err(), "missing permission must return an error");
    assert_eq!(resp.id(), 2);
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(
            error.code,
            error_codes::PERMISSION_DENIED,
            "error code must be PERMISSION_DENIED"
        );
        assert!(
            error.message.contains("fs.appData"),
            "error message must name the missing permission key"
        );
    }
}

// ── Scenario 3: Missing permissions.json → startup failure ───────────────────

#[test]
fn missing_permissions_json_returns_not_found_error() {
    let result = PermissionEngine::load(Path::new("/nonexistent/permissions.json"));

    assert!(
        result.is_err(),
        "missing permissions.json must return an error"
    );
    let err = result.err().unwrap();
    match err {
        PermissionError::NotFound { path, .. } => {
            assert!(
                path.contains("permissions.json"),
                "NotFound error must include the file path"
            );
        }
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[test]
fn missing_permissions_json_error_message_is_clear() {
    let result = PermissionEngine::load(Path::new("/nonexistent/permissions.json"));
    let err = result.err().unwrap();
    let message = err.to_string();

    // The error message must be human-readable and describe the problem.
    assert!(
        message.contains("permissions.json"),
        "error message must reference the file name: {message}"
    );
    assert!(
        !message.is_empty(),
        "error message must not be empty"
    );
}

// ── Scenario 4: Malformed permissions.json → startup failure with details ─────

#[test]
fn malformed_permissions_json_returns_invalid_error() {
    let f = write_temp("{ this is not valid json }");
    let result = PermissionEngine::load(f.path());

    assert!(
        result.is_err(),
        "malformed permissions.json must return an error"
    );
    assert!(
        matches!(result.err().unwrap(), PermissionError::Invalid(_)),
        "error must be PermissionError::Invalid for malformed JSON"
    );
}

#[test]
fn permissions_json_with_unknown_key_returns_invalid_error() {
    // Unknown keys are rejected to catch typos before runtime.
    let f = write_temp(r#"{ "unknownModule": true }"#);
    let result = PermissionEngine::load(f.path());

    assert!(
        result.is_err(),
        "unknown keys in permissions.json must return an error"
    );
    assert!(
        matches!(result.err().unwrap(), PermissionError::Invalid(_)),
        "error must be PermissionError::Invalid for unknown keys"
    );
}

#[test]
fn malformed_permissions_json_error_includes_parse_details() {
    let f = write_temp("{ invalid }");
    let err = PermissionEngine::load(f.path()).err().unwrap();
    let message = err.to_string();

    // The error message must help the developer fix their config.
    assert!(
        !message.is_empty(),
        "parse error message must not be empty"
    );
    assert!(
        message.contains("invalid") || message.contains("permissions.json") || message.len() > 10,
        "parse error message must be informative: {message}"
    );
}

// ── Scenario 5: Granular fs permissions ──────────────────────────────────────

/// `appData` granted → `fs.read` and `fs.write` succeed.
/// `absolutePath` NOT granted → out-of-sandbox paths denied.
#[tokio::test]
async fn fs_app_data_granted_but_absolute_path_denied() {
    let mut engine = PermissionEngine::new(Permissions {
        fs: Some(FsPermissions {
            app_data: true,
            absolute_path: false, // NOT granted
            user_selected: false,
        }),
        ..Default::default()
    });
    engine.require("fs.read", PermissionKey::FsAppData);
    engine.require("fs.write", PermissionKey::FsAppData);
    engine.require("fs.readAbsolute", PermissionKey::FsAbsolutePath);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register("fs.read", make_handler(|_| async { Ok(json!("content")) }));
    dispatcher.register("fs.write", make_handler(|_| async { Ok(json!({})) }));
    dispatcher.register(
        "fs.readAbsolute",
        make_handler(|_| async { Ok(json!("absolute content")) }),
    );

    // fs.read succeeds — appData is granted.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 10,
            method: "fs.read".into(),
            params: json!({ "path": "notes.txt" }),
        })
        .await;
    assert!(resp.is_ok(), "fs.read must succeed when appData is granted");

    // fs.write succeeds — appData is granted.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 11,
            method: "fs.write".into(),
            params: json!({ "path": "notes.txt", "content": "hello" }),
        })
        .await;
    assert!(resp.is_ok(), "fs.write must succeed when appData is granted");

    // fs.readAbsolute denied — absolutePath is NOT granted.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 12,
            method: "fs.readAbsolute".into(),
            params: json!({ "path": "C:\\Users\\secret.txt" }),
        })
        .await;
    assert!(
        resp.is_err(),
        "fs.readAbsolute must be denied when absolutePath is not granted"
    );
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(error.code, error_codes::PERMISSION_DENIED);
        assert!(
            error.message.contains("fs.absolutePath"),
            "error must name the missing permission"
        );
    }
}

/// `appData` does NOT grant `userSelected` or `absolutePath`.
#[test]
fn app_data_flag_isolation_does_not_grant_other_fs_keys() {
    let mut engine = PermissionEngine::new(Permissions {
        fs: Some(FsPermissions {
            app_data: true,
            user_selected: false,
            absolute_path: false,
        }),
        ..Default::default()
    });
    engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
    engine.require("fs.readAbsolute", PermissionKey::FsAbsolutePath);

    assert!(
        engine.check("fs.pickDirectory").is_err(),
        "appData must not grant userSelected"
    );
    assert!(
        engine.check("fs.readAbsolute").is_err(),
        "appData must not grant absolutePath"
    );
}

/// `userSelected` granted → `fs.pickDirectory` succeeds.
/// Other fs methods still require `appData`.
#[tokio::test]
async fn user_selected_granted_allows_pick_directory_only() {
    let mut engine = PermissionEngine::new(Permissions {
        fs: Some(FsPermissions {
            user_selected: true,
            app_data: false, // NOT granted
            absolute_path: false,
        }),
        ..Default::default()
    });
    engine.require("fs.pickDirectory", PermissionKey::FsUserSelected);
    engine.require("fs.read", PermissionKey::FsAppData);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register(
        "fs.pickDirectory",
        make_handler(|_| async { Ok(json!({ "path": "C:\\Users\\docs" })) }),
    );
    dispatcher.register("fs.read", make_handler(|_| async { Ok(json!("content")) }));

    // pickDirectory succeeds.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 20,
            method: "fs.pickDirectory".into(),
            params: json!({}),
        })
        .await;
    assert!(
        resp.is_ok(),
        "fs.pickDirectory must succeed when userSelected is granted"
    );

    // fs.read fails — appData is NOT granted.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 21,
            method: "fs.read".into(),
            params: json!({ "path": "notes.txt" }),
        })
        .await;
    assert!(
        resp.is_err(),
        "fs.read must be denied when appData is not granted"
    );
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(error.code, error_codes::PERMISSION_DENIED);
    }
}

// ── Scenario 6: End-to-end — denied permission flows back to RPC caller ───────

/// Verifies the complete denial chain:
/// RPC request → dispatcher permission check → PERMISSION_DENIED response.
/// The handler is never called.
#[tokio::test]
async fn end_to_end_denied_permission_never_calls_handler() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_clone = handler_called.clone();

    // Engine with nothing granted.
    let mut engine = PermissionEngine::new(Permissions::default());
    engine.require("notifications.show", PermissionKey::Notifications);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register(
        "notifications.show",
        make_handler(move |_| {
            let flag = handler_called_clone.clone();
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(json!({}))
            }
        }),
    );

    let req = RpcRequest {
        id: 30,
        method: "notifications.show".into(),
        params: json!({ "title": "Alert", "body": "Hello!" }),
    };
    let resp = dispatcher.dispatch(req).await;

    // Handler must never have been invoked.
    assert!(
        !handler_called.load(Ordering::SeqCst),
        "handler must never be called when permission is denied"
    );

    // Response must be a structured PERMISSION_DENIED error.
    assert!(resp.is_err());
    assert_eq!(resp.id(), 30);
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(
            error.code,
            error_codes::PERMISSION_DENIED,
            "end-to-end denial must produce PERMISSION_DENIED code"
        );
        assert!(
            error.message.contains("notifications"),
            "error message must identify the required permission"
        );
    }
}

/// Multiple modules — only the granted one succeeds end-to-end.
#[tokio::test]
async fn end_to_end_only_granted_modules_succeed() {
    let mut engine = PermissionEngine::new(Permissions {
        storage: true,
        notifications: false, // NOT granted
        system: true,
        ..Default::default()
    });
    engine.require("storage.set", PermissionKey::Storage);
    engine.require("notifications.show", PermissionKey::Notifications);
    engine.require("system.info", PermissionKey::System);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register("storage.set", make_handler(|_| async { Ok(json!({})) }));
    dispatcher.register("notifications.show", make_handler(|_| async { Ok(json!({})) }));
    dispatcher.register(
        "system.info",
        make_handler(|_| async {
            Ok(json!({ "os": "Windows", "arch": "x86_64" }))
        }),
    );

    // storage.set — granted → succeeds.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 40,
            method: "storage.set".into(),
            params: json!({ "key": "x", "value": "1" }),
        })
        .await;
    assert!(resp.is_ok(), "storage.set must succeed (storage granted)");

    // notifications.show — NOT granted → denied.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 41,
            method: "notifications.show".into(),
            params: json!({ "title": "T", "body": "B" }),
        })
        .await;
    assert!(
        resp.is_err(),
        "notifications.show must fail (notifications not granted)"
    );
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(error.code, error_codes::PERMISSION_DENIED);
    }

    // system.info — granted → succeeds.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 42,
            method: "system.info".into(),
            params: json!({}),
        })
        .await;
    assert!(resp.is_ok(), "system.info must succeed (system granted)");
}

/// Loading a valid permissions.json and performing end-to-end checks.
#[tokio::test]
async fn end_to_end_loaded_permissions_json_enforced() {
    // Write a real permissions.json granting only window.
    let f = write_temp(r#"{ "window": true }"#);

    let engine = PermissionEngine::load(f.path()).expect("valid permissions.json must load");
    let mut engine = engine;
    engine.require("window.minimize", PermissionKey::Window);
    engine.require("window.close", PermissionKey::Window);
    engine.require("storage.get", PermissionKey::Storage);

    let dispatcher = Dispatcher::new().with_engine(Arc::new(engine));
    dispatcher.register("window.minimize", make_handler(|_| async { Ok(json!({})) }));
    dispatcher.register("window.close", make_handler(|_| async { Ok(json!({})) }));
    dispatcher.register("storage.get", make_handler(|_| async { Ok(json!({ "value": "x" })) }));

    // window.minimize — granted → succeeds.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 50,
            method: "window.minimize".into(),
            params: json!({}),
        })
        .await;
    assert!(resp.is_ok(), "window.minimize must succeed (window granted)");

    // storage.get — NOT in this permissions.json → denied.
    let resp = dispatcher
        .dispatch(RpcRequest {
            id: 51,
            method: "storage.get".into(),
            params: json!({ "key": "theme" }),
        })
        .await;
    assert!(
        resp.is_err(),
        "storage.get must be denied when storage is absent from permissions.json"
    );
    if let RpcResponse::Error { error, .. } = resp {
        assert_eq!(error.code, error_codes::PERMISSION_DENIED);
    }
}
