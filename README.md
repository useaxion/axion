# Axion

> Build native desktop apps with React and TypeScript. No native code required.

Axion is a secure, lightweight, React-first desktop runtime. It combines a Rust-powered native engine with modern web tooling so you can ship a real desktop application using the stack you already know.

---

## Why Axion?

Most desktop runtimes make you choose between developer experience and performance. Axion doesn't.

- **React-first** — build UIs with React, TanStack Router, Zustand, and Tailwind, exactly as you would on the web
- **Rust-powered runtime** — native performance, low memory footprint, and a binary under 20MB
- **Secure by default** — every native capability is permission-gated at the runtime level, not the UI level
- **Fully typed** — Rust APIs auto-generate TypeScript types, so your SDK is always in sync with the runtime
- **Zero native code** — scaffold, develop, and ship without touching Rust or C++

---

## Quick Start

```bash
# Install the CLI
npm install -g axion

# Create a new app
axion create my-app

# Start developing
cd my-app
axion dev
```

Your app opens in a native window with full hot reload. That's it.

---

## How It Works

Axion is made up of three layers that work together:

```
React App (TypeScript + Vite)
       ↓  SDK calls
Axion SDK (TypeScript)
       ↓  JSON RPC
Rust Runtime (WebView2 + Tokio)
       ↓  native modules
Filesystem · Storage · Notifications · System
```

The **Rust runtime** owns the window, security enforcement, and all native operations. The **TypeScript SDK** exposes a clean async API to your React app. The **Node.js CLI** handles everything in between — scaffolding, dev server, and builds.

---

## Native Capabilities

```ts
import { fs, storage, notifications, window, system } from "axion"

// Storage
await storage.set("theme", "dark")
const theme = await storage.get("theme")

// Filesystem
await fs.write("notes.json", JSON.stringify(data))
const content = await fs.read("notes.json")

// Notifications
await notifications.show({ title: "Saved", body: "Your file has been saved." })

// System
const info = await system.info()
```

React hooks are also available:

```ts
const [theme, setTheme] = useStorage("theme")
const info = useSystemInfo()
```

---

## Permissions

Declare what your app needs in `permissions.json`. The runtime enforces it — nothing gets through without explicit permission.

```json
{
  "fs": {
    "appData": true,
    "userSelected": true
  },
  "notifications": true
}
```

---

## CLI

| Command | Description |
|---|---|
| `axion create` | Scaffold a new project |
| `axion dev` | Run Vite dev server + Axion runtime |
| `axion build` | Produce a distributable executable |

---

## Project Structure

A generated Axion app looks like this:

```
my-app/
├── src/
│   ├── pages/
│   ├── components/
│   ├── App.tsx
│   └── main.tsx
├── permissions.json
├── axion.config.json
├── package.json
└── vite.config.ts
```

---

## Monorepo Structure

```
axion/
├── core/         # Rust runtime
├── sdk/          # TypeScript SDK
├── cli/          # Node.js CLI
├── templates/    # Project templates
├── examples/     # Example applications
├── docs/         # Documentation
└── scripts/      # Build and release scripts
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Runtime | Rust + Tokio |
| Renderer | WebView2 |
| CLI | Node.js |
| SDK | TypeScript |
| Frontend | React |
| Dev Server | Vite |
| Routing | TanStack Router |
| State | Zustand |
| Server State | TanStack Query |
| Styling | Tailwind CSS |

---

## Roadmap

- [x] Runtime skeleton — WebView2 window
- [x] Dev mode — Vite + hot reload inside Axion
- [ ] RPC bridge — typed React ↔ Rust communication
- [ ] Permission engine — runtime security enforcement
- [ ] Native modules — fs, storage, notifications, system
- [ ] TypeScript SDK — capability APIs + React hooks
- [ ] Build pipeline — distributable `.exe`

---

## v1 Scope

Axion v1 targets **Windows only**. The following are planned for future releases:

- macOS and Linux support
- Multi-window
- Background services
- Auto-updater
- Plugin marketplace

---

## Contributing

Contributions are welcome. Please open an issue before submitting a pull request for significant changes.

---

## License

MIT
