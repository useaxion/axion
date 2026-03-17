/**
 * Axion `fs` capability module — issue #20.
 *
 * Typed wrappers around the Rust `fs.*` RPC handlers.
 * All paths are relative to the app's sandbox root
 * (`%APPDATA%\axion\<AppName>\`) unless `fs.absolutePath` is granted.
 *
 * Errors propagate as `RpcError` exceptions.
 */
import { rpc } from "../rpc-client";
import type { RpcError } from "../rpc-client";

// Re-export so callers can import the error type from the same module.
export type { RpcError };

// ── Parameter / return types ──────────────────────────────────────────────────

/** Params for `fs.read`. */
export interface FsReadParams {
  /** Sandbox-relative path to the file. */
  path: string;
}

/** Result of `fs.read`. */
export interface FsReadResult {
  /** UTF-8 contents of the file. */
  content: string;
}

/** Params for `fs.write`. */
export interface FsWriteParams {
  /** Sandbox-relative path to write. */
  path: string;
  /** UTF-8 content to write. Overwrites the existing file. */
  content: string;
}

/** Params for `fs.delete`. */
export interface FsDeleteParams {
  /** Sandbox-relative path to delete. */
  path: string;
}

/** Result of `fs.pickDirectory`. */
export interface FsPickDirectoryResult {
  /**
   * Absolute path selected by the user, or `null` if the dialog was
   * cancelled.
   */
  path: string | null;
}

// ── Module ────────────────────────────────────────────────────────────────────

/**
 * Read the contents of a file at `path` (sandbox-relative).
 *
 * @throws {RpcError} If the file does not exist or is not readable.
 */
export async function read(path: string): Promise<string> {
  const result = await rpc.invoke<FsReadResult>("fs.read", { path });
  return result.content;
}

/**
 * Write `content` to `path` (sandbox-relative).
 * Creates intermediate directories as needed.
 *
 * @throws {RpcError} If the write fails.
 */
export async function write(path: string, content: string): Promise<void> {
  await rpc.invoke<Record<string, never>>("fs.write", { path, content });
}

/**
 * Delete the file at `path` (sandbox-relative).
 *
 * @throws {RpcError} If the file does not exist or deletion fails.
 */
export async function deleteFile(path: string): Promise<void> {
  await rpc.invoke<Record<string, never>>("fs.delete", { path });
}

/**
 * Open the native Windows folder-picker dialog.
 *
 * Returns the absolute path of the selected directory, or `null` if the
 * user cancelled.
 *
 * Requires `"fs": { "userSelected": true }` in `permissions.json`.
 *
 * @throws {RpcError} If the dialog API is unavailable.
 */
export async function pickDirectory(): Promise<string | null> {
  const result = await rpc.invoke<FsPickDirectoryResult>("fs.pickDirectory");
  return result.path;
}

/** Named fs module for barrel imports: `import { fs } from "axion"`. */
export const fs = { read, write, delete: deleteFile, pickDirectory };
