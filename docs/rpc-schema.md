# Axion RPC Schema

This document defines the canonical JSON message format for all communication
between the React application (via the TypeScript SDK) and the Rust runtime.
This schema is the stable contract between both layers — changes here are
breaking changes and require a version bump.

---

## Transport

Messages are exchanged over the WebView2 IPC bridge:

- **JS → Rust**: `window.chrome.webview.postMessage(JSON.stringify(request))`
- **Rust → JS**: `window.chrome.webview.addEventListener('message', handler)`

All messages are UTF-8 encoded JSON strings. Binary encoding is not supported
in v1.

---

## Message Size Limit

The runtime enforces a **1 MiB (1,048,576 byte)** hard cap on all messages in
both directions. Messages exceeding this limit are:

- **Inbound (JS → Rust)**: silently dropped; no error is sent back.
- **Outbound (Rust → JS)**: rejected at the call site with an `IpcError`.

Callers should keep payloads small. Large data transfers (e.g. file contents)
should use chunked reads rather than single large messages.

---

## Request (JS → Rust)

```json
{
  "id": 12,
  "method": "fs.write",
  "params": {
    "path": "notes.json",
    "content": "hello"
  }
}
```

| Field | Type | Description |
|---|---|---|
| `id` | `u64` | Caller-assigned request ID. Echoed back in the response. Must be unique per in-flight request. |
| `method` | `string` | Dot-namespaced capability: `"<module>.<action>"`, e.g. `"fs.write"`, `"storage.get"`. |
| `params` | `object` | Method-specific parameters. Shape is defined per method. |

### Method naming convention

Methods follow the pattern `<module>.<action>`:

| Module | Example methods |
|---|---|
| `fs` | `fs.read`, `fs.write`, `fs.delete`, `fs.pickDirectory` |
| `storage` | `storage.get`, `storage.set`, `storage.remove` |
| `notifications` | `notifications.show` |
| `system` | `system.info`, `system.platform`, `system.version` |
| `window` | `window.minimize`, `window.maximize`, `window.close`, `window.setTitle` |

---

## Success Response (Rust → JS)

```json
{
  "id": 12,
  "result": true
}
```

| Field | Type | Description |
|---|---|---|
| `id` | `u64` | The `id` from the originating request. |
| `result` | `any` | The method's return value. Shape is method-specific. |

The `error` key is **absent** on success responses.

---

## Error Response (Rust → JS)

```json
{
  "id": 12,
  "error": {
    "code": -32601,
    "message": "Method not found: fs.write"
  }
}
```

| Field | Type | Description |
|---|---|---|
| `id` | `u64` | The `id` from the originating request. |
| `error.code` | `i32` | Numeric error code (see table below). |
| `error.message` | `string` | Human-readable description. Never contains internal details (stack traces, absolute paths) in production builds. |

The `result` key is **absent** on error responses.

---

## Error Codes

Axion follows the [JSON-RPC 2.0](https://www.jsonrpc.org/specification) error
code convention for standard codes. Implementation-defined codes occupy the
reserved server error range.

| Code | Constant | Meaning |
|---|---|---|
| `-32700` | `PARSE_ERROR` | The runtime could not parse the JSON payload. |
| `-32600` | `INVALID_REQUEST` | The message is valid JSON but not a valid RPC request. |
| `-32601` | `METHOD_NOT_FOUND` | The method is not registered with the runtime. |
| `-32602` | `INVALID_PARAMS` | The supplied `params` are invalid for the requested method. |
| `-32603` | `INTERNAL_ERROR` | An unexpected internal error occurred. |
| `-32000` | `PERMISSION_DENIED` | The caller lacks the required permission declared in `permissions.json`. |

---

## Typed Params Examples

### `fs.write`

```json
{
  "id": 1,
  "method": "fs.write",
  "params": { "path": "notes.json", "content": "hello world" }
}
```

### `storage.get`

```json
{
  "id": 2,
  "method": "storage.get",
  "params": { "key": "theme" }
}
```

### `notifications.show`

```json
{
  "id": 3,
  "method": "notifications.show",
  "params": { "title": "Saved", "body": "Your file has been saved." }
}
```

### `system.info`

```json
{
  "id": 4,
  "method": "system.info",
  "params": {}
}
```

---

## Security Considerations

- **Method validation**: The dispatcher rejects any method not explicitly
  registered. Unknown methods always return `METHOD_NOT_FOUND`.
- **Permission enforcement**: Every method handler checks permissions before
  executing. Unauthorized calls return `PERMISSION_DENIED` and are logged.
- **Input validation**: Handlers validate `params` shape before operating on
  the filesystem or other native resources.
- **No internal detail leakage**: Error `message` fields in production builds
  must not expose stack traces, absolute paths, or other internal state.
- **Request ID**: IDs are caller-supplied and not validated for uniqueness by
  the runtime. The SDK is responsible for generating unique IDs per session.

---

## Versioning

This schema is considered **v1**. Future breaking changes will be introduced
through a versioned field or a separate schema document. Additive changes
(new methods, new optional fields) are non-breaking.
