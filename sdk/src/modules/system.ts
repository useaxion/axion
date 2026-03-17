/**
 * Axion `system` capability module — issue #20.
 *
 * Typed wrappers around the Rust `system.*` RPC handlers.
 * No permissions required — system info is non-sensitive and read-only.
 *
 * Errors propagate as `RpcError` exceptions.
 */
import { rpc } from "../rpc-client";
import type { RpcError } from "../rpc-client";

export type { RpcError };

// ── Return types ──────────────────────────────────────────────────────────────

/** Result of `system.info`. */
export interface SystemInfo {
  /** OS name, e.g. `"Windows"`. */
  os: string;
  /** OS version string, e.g. `"10.0.22000"`. */
  version: string;
  /** CPU architecture, e.g. `"x86_64"`. */
  arch: string;
  /** Machine hostname. */
  hostname: string;
  /** Total physical memory in megabytes. */
  totalMemoryMb: number;
}

/** Result of `system.platform`. */
export interface SystemPlatform {
  /** Always `"windows"` for Axion v1. */
  platform: "windows";
}

/** Result of `system.version`. */
export interface SystemVersion {
  /** Axion runtime version (semver string). */
  version: string;
}

// ── Module ────────────────────────────────────────────────────────────────────

/**
 * Get OS and hardware information.
 *
 * @throws {RpcError} If the system info query fails.
 */
export async function info(): Promise<SystemInfo> {
  return rpc.invoke<SystemInfo>("system.info");
}

/**
 * Get the current platform identifier.
 *
 * Always returns `{ platform: "windows" }` in Axion v1.
 *
 * @throws {RpcError} If the RPC call fails.
 */
export async function platform(): Promise<SystemPlatform> {
  return rpc.invoke<SystemPlatform>("system.platform");
}

/**
 * Get the Axion runtime version.
 *
 * @throws {RpcError} If the RPC call fails.
 */
export async function version(): Promise<SystemVersion> {
  return rpc.invoke<SystemVersion>("system.version");
}

/** Named system module for barrel imports: `import { system } from "axion"`. */
export const system = { info, platform, version };
