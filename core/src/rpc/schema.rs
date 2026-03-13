// Public API — consumed by the RPC dispatcher and TypeScript SDK type generation.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Named error codes for the Axion RPC protocol.
///
/// Standard codes follow the JSON-RPC 2.0 specification.
/// Implementation-defined codes occupy the reserved range -32000 to -32099.
pub mod error_codes {
    /// Invalid JSON — the server could not parse the request.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON is valid but not a valid RPC request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The requested method does not exist or is not registered.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// The supplied parameters are invalid for the requested method.
    pub const INVALID_PARAMS: i32 = -32602;
    /// An unexpected internal error occurred in the runtime.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// The caller lacks the required permission for this operation.
    pub const PERMISSION_DENIED: i32 = -32000;
}

/// A JSON-RPC request from JavaScript to the Rust runtime.
///
/// Wire format (JS → Rust):
/// ```json
/// { "id": 12, "method": "fs.write", "params": { "path": "notes.json", "content": "hello" } }
/// ```
///
/// - `id` is caller-assigned and echoed back in the response.
/// - `method` is dot-namespaced: `"<module>.<capability>"`, e.g. `"fs.write"`.
/// - `params` is a JSON object specific to the method.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct RpcRequest<P = serde_json::Value> {
    /// Caller-assigned request ID. Echoed back unchanged in the response.
    // `u64` is correct in Rust. TypeScript represents it as `number` because
    // JSON.parse() always produces `number` and request IDs never exceed
    // Number.MAX_SAFE_INTEGER in practice.
    #[ts(type = "number")]
    pub id: u64,
    /// Dot-namespaced method name, e.g. `"fs.write"` or `"storage.get"`.
    pub method: String,
    /// Method-specific parameters as a JSON value or a typed struct.
    pub params: P,
}

/// Error payload carried by an [`RpcResponse::Error`].
///
/// Wire format:
/// ```json
/// { "code": -32601, "message": "Method not found: fs.write" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct RpcErrorPayload {
    /// Numeric error code (see [`error_codes`]).
    pub code: i32,
    /// Human-readable error description. Never expose internal details
    /// (stack traces, file paths) in production builds.
    pub message: String,
}

impl RpcErrorPayload {
    /// Construct an error payload from a code and message.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A JSON-RPC response from the Rust runtime to JavaScript.
///
/// Exactly one of `result` or `error` is present — never both.
///
/// Success wire format (Rust → JS):
/// ```json
/// { "id": 12, "result": { ... } }
/// ```
///
/// Error wire format (Rust → JS):
/// ```json
/// { "id": 12, "error": { "code": -32601, "message": "Method not found: fs.write" } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(untagged)]
pub enum RpcResponse<R = serde_json::Value> {
    /// Successful response — contains the method's return value.
    Success {
        #[ts(type = "number")]
        id: u64,
        result: R,
    },
    /// Error response — contains a structured error payload.
    Error {
        #[ts(type = "number")]
        id: u64,
        error: RpcErrorPayload,
    },
}

impl<R> RpcResponse<R> {
    /// Construct a success response.
    pub fn success(id: u64, result: R) -> Self {
        Self::Success { id, result }
    }

    /// Construct an error response from a code and message.
    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self::Error {
            id,
            error: RpcErrorPayload::new(code, message),
        }
    }

    /// Return the request ID echoed by this response.
    pub fn id(&self) -> u64 {
        match self {
            Self::Success { id, .. } | Self::Error { id, .. } => *id,
        }
    }

    /// Returns `true` if this is a success response.
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns `true` if this is an error response.
    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }
}

/// Export all RPC types (including transitive dependencies like `JsonValue`)
/// to `sdk/src/types/` so every generated file lands in the same directory.
///
/// Run with `cargo test --lib export_rpc_types` or `cargo test --lib` to
/// regenerate. The output is committed to the repo — see docs/type-generation.md.
#[cfg(test)]
mod type_export {
    use super::*;

    #[test]
    fn export_rpc_types() {
        // `export_all_to` writes the type AND all its transitive dependencies
        // (e.g. `JsonValue` for `serde_json::Value`) into the same target
        // directory, keeping all generated files under `sdk/src/types/`.
        let out = concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdk/src/types");
        RpcRequest::<serde_json::Value>::export_all_to(out).expect("export RpcRequest");
        RpcErrorPayload::export_all_to(out).expect("export RpcErrorPayload");
        RpcResponse::<serde_json::Value>::export_all_to(out).expect("export RpcResponse");
    }
}

#[cfg(test)]
mod tests {
    use super::{error_codes::*, *};
    use serde_json::{json, Value};

    // ── Error codes ───────────────────────────────────────────────────────────

    #[test]
    fn error_codes_match_jsonrpc_spec() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
        // Implementation-defined range
        assert!(PERMISSION_DENIED >= -32099 && PERMISSION_DENIED <= -32000);
    }

    // ── RpcRequest ────────────────────────────────────────────────────────────

    #[test]
    fn request_serializes_correctly() {
        let req: RpcRequest = RpcRequest {
            id: 12,
            method: "fs.write".to_string(),
            params: json!({ "path": "notes.json", "content": "hello" }),
        };

        let json = serde_json::to_string(&req).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["id"], 12);
        assert_eq!(v["method"], "fs.write");
        assert_eq!(v["params"]["path"], "notes.json");
        assert_eq!(v["params"]["content"], "hello");
    }

    #[test]
    fn request_deserializes_correctly() {
        let raw = r#"{"id":12,"method":"fs.write","params":{"path":"notes.json","content":"hello"}}"#;
        let req: RpcRequest = serde_json::from_str(raw).unwrap();

        assert_eq!(req.id, 12);
        assert_eq!(req.method, "fs.write");
        assert_eq!(req.params["path"], "notes.json");
        assert_eq!(req.params["content"], "hello");
    }

    #[test]
    fn request_round_trips() {
        let original: RpcRequest = RpcRequest {
            id: 99,
            method: "storage.get".to_string(),
            params: json!({ "key": "theme" }),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: RpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    // ── RpcResponse — success ─────────────────────────────────────────────────

    #[test]
    fn success_response_serializes_correctly() {
        let resp: RpcResponse = RpcResponse::success(12, json!(true));
        let v: Value = serde_json::to_value(&resp).unwrap();

        assert_eq!(v["id"], 12);
        assert_eq!(v["result"], true);
        assert!(v.get("error").is_none());
    }

    #[test]
    fn success_response_deserializes_correctly() {
        let raw = r#"{"id":12,"result":true}"#;
        let resp: RpcResponse = serde_json::from_str(raw).unwrap();

        assert!(resp.is_ok());
        assert_eq!(resp.id(), 12);
    }

    #[test]
    fn success_response_round_trips() {
        let original: RpcResponse = RpcResponse::success(7, json!({ "os": "Windows", "arch": "x86_64" }));
        let json = serde_json::to_string(&original).unwrap();
        let restored: RpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    // ── RpcResponse — error ───────────────────────────────────────────────────

    #[test]
    fn error_response_serializes_correctly() {
        let resp: RpcResponse = RpcResponse::error(12, METHOD_NOT_FOUND, "Method not found: fs.write");
        let v: Value = serde_json::to_value(&resp).unwrap();

        assert_eq!(v["id"], 12);
        assert_eq!(v["error"]["code"], -32601);
        assert_eq!(v["error"]["message"], "Method not found: fs.write");
        assert!(v.get("result").is_none());
    }

    #[test]
    fn error_response_deserializes_correctly() {
        let raw = r#"{"id":12,"error":{"code":-32601,"message":"Method not found: fs.write"}}"#;
        let resp: RpcResponse = serde_json::from_str(raw).unwrap();

        assert!(resp.is_err());
        assert_eq!(resp.id(), 12);
    }

    #[test]
    fn error_response_round_trips() {
        let original: RpcResponse =
            RpcResponse::error(5, PERMISSION_DENIED, "Permission denied: fs.write");
        let json = serde_json::to_string(&original).unwrap();
        let restored: RpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    // ── Helper methods ────────────────────────────────────────────────────────

    #[test]
    fn response_id_returns_correct_id_for_both_variants() {
        let ok: RpcResponse = RpcResponse::success(42, json!(null));
        let err: RpcResponse = RpcResponse::error(43, INTERNAL_ERROR, "oops");
        assert_eq!(ok.id(), 42);
        assert_eq!(err.id(), 43);
    }

    #[test]
    fn is_ok_and_is_err_are_consistent() {
        let ok: RpcResponse = RpcResponse::success(1, json!(null));
        let err: RpcResponse = RpcResponse::error(2, INTERNAL_ERROR, "fail");
        assert!(ok.is_ok() && !ok.is_err());
        assert!(err.is_err() && !err.is_ok());
    }

    // ── Typed params ──────────────────────────────────────────────────────────

    #[test]
    fn request_works_with_typed_params() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct FsWriteParams {
            path: String,
            content: String,
        }

        let req = RpcRequest {
            id: 1,
            method: "fs.write".to_string(),
            params: FsWriteParams {
                path: "notes.json".to_string(),
                content: "hello".to_string(),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        let restored: RpcRequest<FsWriteParams> = serde_json::from_str(&json).unwrap();
        assert_eq!(req, restored);
    }
}
