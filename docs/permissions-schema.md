# Permissions Schema

Every Axion app declares the native capabilities it needs in a `permissions.json` file at the project root. The runtime reads this file at startup and rejects any RPC call whose module has not been explicitly granted — in both development and production.

---

## File location

```
my-app/
├── src/
├── permissions.json   ← required
├── axion.config.json
└── package.json
```

---

## Schema

```json
{
  "fs":            { "appData": boolean, "userSelected": boolean, "absolutePath": boolean },
  "storage":       boolean,
  "notifications": boolean,
  "system":        boolean,
  "window":        boolean
}
```

All keys are optional and default to `false` / denied when omitted. Unknown keys are rejected at startup to catch typos before the app runs.

A JSON Schema file for editor IntelliSense is provided at `schemas/permissions.schema.json`. Add this to your `settings.json` to enable autocompletion in VS Code:

```json
{
  "json.schemas": [
    {
      "fileMatch": ["permissions.json"],
      "url": "./schemas/permissions.schema.json"
    }
  ]
}
```

---

## Module reference

### `fs` — Filesystem

Controls access to the filesystem. `fs` is the only module with granular flags; all others are a single boolean.

| Flag           | Type    | Default | RPC methods                          | Description |
|----------------|---------|---------|--------------------------------------|-------------|
| `appData`      | boolean | `false` | `fs.read`, `fs.write`, `fs.delete`   | Access to the app's AppData sandbox: `AppData/Local/Axion/<AppName>/`. This is the default safe storage location for all Axion apps. |
| `userSelected` | boolean | `false` | `fs.pickDirectory`                   | Access to directories the user explicitly picks via a system directory picker. Grants access only to the chosen directory. |
| `absolutePath` | boolean | `false` | `fs.read`, `fs.write`, `fs.delete` (with absolute paths) | Access to arbitrary absolute paths on the filesystem. Only grant if strictly required. |

Omitting the `fs` key entirely denies all `fs.*` calls. Including it with all flags `false` has the same effect.

```json
// Minimal — sandbox only
"fs": { "appData": true }

// With user-selected directories
"fs": { "appData": true, "userSelected": true }

// Full access (use sparingly)
"fs": { "appData": true, "userSelected": true, "absolutePath": true }
```

---

### `storage` — Key-value storage

```json
"storage": true
```

Enables the persistent key-value storage module. Data is scoped to the app's AppData sandbox and does not require any filesystem permission.

| RPC method        | Description                         |
|-------------------|-------------------------------------|
| `storage.get`     | Read a value by key                 |
| `storage.set`     | Write a value by key                |
| `storage.remove`  | Delete a key                        |

---

### `notifications` — Desktop notifications

```json
"notifications": true
```

Allows the app to display system desktop notifications.

| RPC method            | Description              |
|-----------------------|--------------------------|
| `notifications.show`  | Show a desktop notification with a title and body |

---

### `system` — System information

```json
"system": true
```

Grants read-only access to system metadata. No writes are possible through this module.

| RPC method         | Description                         |
|--------------------|-------------------------------------|
| `system.info`      | Full system info object             |
| `system.platform`  | Platform string, e.g. `"windows"`  |
| `system.version`   | OS version string                   |

---

### `window` — Window management

```json
"window": true
```

Enables window management operations.

| RPC method          | Description                       |
|---------------------|-----------------------------------|
| `window.minimize`   | Minimize the app window           |
| `window.maximize`   | Maximize / restore the app window |
| `window.close`      | Close the app window              |
| `window.setTitle`   | Set the window title bar text     |

---

## Examples

### Notes app (filesystem + storage)

```json
{
  "fs": { "appData": true },
  "storage": true,
  "window": true
}
```

### File manager (user-selected directories)

```json
{
  "fs": { "appData": true, "userSelected": true },
  "window": true
}
```

### System dashboard (read-only, no filesystem)

```json
{
  "system": true,
  "notifications": true,
  "window": true
}
```

### Full permissions (all capabilities)

```json
{
  "fs": { "appData": true, "userSelected": true, "absolutePath": false },
  "storage": true,
  "notifications": true,
  "system": true,
  "window": true
}
```

---

## Runtime behaviour

- The runtime reads `permissions.json` at **startup**. If the file is missing or contains invalid JSON or unknown keys, the runtime exits immediately with a descriptive error message.
- Every RPC call is checked against the loaded permissions **before** execution. A denied call returns an RPC error with code `-32000` (`PERMISSION_DENIED`).
- Permissions are enforced in both **development** and **production** — `axion dev` applies the same rules as a packaged `.exe`.
- Granular `fs` flag enforcement (which paths and operations are permitted beyond the module-level check) is handled by the Permission Engine (see issue #9 and `docs/permission-engine.md`).

---

## Startup error messages

| Condition | Message |
|---|---|
| File missing | `permissions.json not found at '<path>': <io error>` |
| Invalid JSON | `permissions.json is invalid: <serde error>` |
| Unknown key | `permissions.json is invalid: unknown field '<key>', expected one of: fs, storage, notifications, system, window` |
