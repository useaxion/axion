/**
 * Axion `storage` capability module — issue #20.
 *
 * Typed wrappers around the Rust `storage.*` RPC handlers.
 * Backed by a JSON file in the app's sandbox — persists across restarts.
 *
 * Requires `"storage": true` in `permissions.json`.
 * Errors propagate as `RpcError` exceptions.
 */
import { rpc } from "../rpc-client";
import type { RpcError } from "../rpc-client";

export type { RpcError };

// ── Return types ──────────────────────────────────────────────────────────────

/** Result of `storage.get`. */
export interface StorageGetResult {
  /** The stored value, or `null` if the key does not exist. */
  value: string | null;
}

// ── Module ────────────────────────────────────────────────────────────────────

/**
 * Get the value stored at `key`.
 *
 * Returns `null` if the key does not exist.
 *
 * @throws {RpcError} If the storage read fails.
 */
export async function get(key: string): Promise<string | null> {
  const result = await rpc.invoke<StorageGetResult>("storage.get", { key });
  return result.value;
}

/**
 * Set `key` to `value`. Overwrites any existing value.
 *
 * @throws {RpcError} If the storage write fails.
 */
export async function set(key: string, value: string): Promise<void> {
  await rpc.invoke<Record<string, never>>("storage.set", { key, value });
}

/**
 * Remove `key` from storage. No-op if the key does not exist.
 *
 * @throws {RpcError} If the storage remove fails.
 */
export async function remove(key: string): Promise<void> {
  await rpc.invoke<Record<string, never>>("storage.remove", { key });
}

/** Named storage module for barrel imports: `import { storage } from "axion"`. */
export const storage = { get, set, remove };
