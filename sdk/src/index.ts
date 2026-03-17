/**
 * Axion SDK entry point.
 *
 * ```ts
 * import { fs, storage, notifications, system, window as axionWindow } from "axion";
 * ```
 */

// ── RPC infrastructure ────────────────────────────────────────────────────────
export { rpc, RpcClient, RpcError, webViewTransport } from "./rpc-client";
export type { RpcTransport, RpcClientConfig } from "./rpc-client";

// ── Generated RPC types ────────────────────────────────────────────────────────
export type { RpcRequest, RpcResponse, RpcErrorPayload } from "./types/index";

// ── Capability modules ────────────────────────────────────────────────────────
export { fs, read, write, pickDirectory } from "./modules/fs";
export { deleteFile } from "./modules/fs";
export type {
  FsReadParams,
  FsReadResult,
  FsWriteParams,
  FsDeleteParams,
  FsPickDirectoryResult,
} from "./modules/fs";

export { storage, get, set, remove } from "./modules/storage";
export type { StorageGetResult } from "./modules/storage";

export { notifications, show } from "./modules/notifications";
export type { NotificationOptions } from "./modules/notifications";

export { system, info, platform, version } from "./modules/system";
export type { SystemInfo, SystemPlatform, SystemVersion } from "./modules/system";

export {
  window_,
  minimize,
  maximize,
  close,
  setTitle,
} from "./modules/window";

// ── React hooks ───────────────────────────────────────────────────────────────
export { useStorage } from "./hooks/useStorage";
export { useSystemInfo } from "./hooks/useSystemInfo";
export { useWindow } from "./hooks/useWindow";
export type { WindowControls } from "./hooks/useWindow";
