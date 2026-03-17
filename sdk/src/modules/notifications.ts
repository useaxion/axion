/**
 * Axion `notifications` capability module — issue #20.
 *
 * Typed wrapper around the Rust `notifications.show` RPC handler.
 *
 * Requires `"notifications": true` in `permissions.json`.
 * Errors propagate as `RpcError` exceptions.
 */
import { rpc } from "../rpc-client";
import type { RpcError } from "../rpc-client";

export type { RpcError };

// ── Parameter types ───────────────────────────────────────────────────────────

/** Params for `notifications.show`. */
export interface NotificationOptions {
  /** Title line of the toast notification. */
  title: string;
  /** Body text of the toast notification. */
  body: string;
}

// ── Module ────────────────────────────────────────────────────────────────────

/**
 * Show a Windows toast notification.
 *
 * @param options - `{ title, body }` for the toast.
 * @throws {RpcError} If the notification API is unavailable or the permission
 *   is not granted.
 */
export async function show(options: NotificationOptions): Promise<void> {
  await rpc.invoke<Record<string, never>>("notifications.show", options);
}

/** Named notifications module for barrel imports. */
export const notifications = { show };
