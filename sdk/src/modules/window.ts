/**
 * Axion `window` capability module — issue #20.
 *
 * Typed wrappers around the Rust `window.*` RPC handlers.
 * No permissions required — window control is implicit for any Axion app.
 *
 * Errors propagate as `RpcError` exceptions.
 */
import { rpc } from "../rpc-client";
import type { RpcError } from "../rpc-client";

export type { RpcError };

// ── Module ────────────────────────────────────────────────────────────────────

/**
 * Minimize the window to the taskbar.
 *
 * @throws {RpcError} If the window operation fails.
 */
export async function minimize(): Promise<void> {
  await rpc.invoke<Record<string, never>>("window.minimize");
}

/**
 * Maximize the window, or restore it if already maximized.
 *
 * @throws {RpcError} If the window operation fails.
 */
export async function maximize(): Promise<void> {
  await rpc.invoke<Record<string, never>>("window.maximize");
}

/**
 * Close the window and exit the application.
 *
 * This triggers a clean Tokio shutdown on the Rust side. The promise
 * resolves before the process exits.
 *
 * @throws {RpcError} If the window operation fails.
 */
export async function close(): Promise<void> {
  await rpc.invoke<Record<string, never>>("window.close");
}

/**
 * Set the window title bar text.
 *
 * @param title - The new window title.
 * @throws {RpcError} If the window operation fails.
 */
export async function setTitle(title: string): Promise<void> {
  await rpc.invoke<Record<string, never>>("window.setTitle", { title });
}

/**
 * Named window module for barrel imports: `import { window } from "axion"`.
 *
 * Note: use `import { window as axionWindow } from "axion"` to avoid
 * shadowing the browser's global `window` object.
 */
export const window_ = { minimize, maximize, close, setTitle };
