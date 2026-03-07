/**
 * Axion RPC client — TypeScript side of the React ↔ Rust communication layer.
 *
 * All SDK capability calls go through `invoke()`. The client correlates
 * asynchronous responses by request ID and resolves or rejects the caller's
 * Promise accordingly.
 */

// ── Schema types (mirror core/src/rpc/schema.rs) ──────────────────────────────

interface RpcRequest {
  id: number;
  method: string;
  params: unknown;
}

interface RpcSuccessResponse<T> {
  id: number;
  result: T;
}

interface RpcErrorPayload {
  code: number;
  message: string;
}

interface RpcErrorResponse {
  id: number;
  error: RpcErrorPayload;
}

// ── Public error type ─────────────────────────────────────────────────────────

/** Typed error thrown when the Rust runtime returns an RPC error response. */
export class RpcError extends Error {
  /** Numeric error code matching the Rust `error_codes` constants. */
  readonly code: number;

  constructor(payload: RpcErrorPayload) {
    super(payload.message);
    this.name = "RpcError";
    this.code = payload.code;
  }
}

// ── Transport abstraction (enables testing without a real WebView2) ───────────

/**
 * Minimal transport interface used by `RpcClient`.
 * The production implementation uses `window.chrome.webview`;
 * tests inject a mock.
 */
export interface RpcTransport {
  send(message: string): void;
  onMessage(handler: (message: string) => void): void;
}

// ── RpcClient ─────────────────────────────────────────────────────────────────

interface Pending<T> {
  resolve: (value: T) => void;
  reject: (reason: RpcError) => void;
  /** setTimeout handle, or null when timeouts are disabled. */
  timer: ReturnType<typeof setTimeout> | null;
}

export interface RpcClientConfig {
  /**
   * Request timeout in milliseconds.
   * Set to 0 to disable. Defaults to 30 000 ms.
   *
   * Timed-out requests are removed from the pending map to prevent memory
   * leaks when a response never arrives (e.g. runtime crash).
   */
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 30_000;

export class RpcClient {
  private nextId = 1;
  private readonly MAX_ID = Number.MAX_SAFE_INTEGER;
  private readonly pending = new Map<number, Pending<unknown>>();
  private readonly timeoutMs: number;

  constructor(
    private readonly transport: RpcTransport,
    config: RpcClientConfig = {}
  ) {
    this.timeoutMs = config.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    transport.onMessage((msg) => this.handleMessage(msg));
  }

  private allocateId(): number {
    const id = this.nextId;
    this.nextId = this.nextId >= this.MAX_ID ? 1 : this.nextId + 1;
    return id;
  }

  private handleMessage(raw: string): void {
    let response: Record<string, unknown>;
    try {
      response = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      // Malformed JSON from the runtime — drop silently.
      return;
    }

    const id = response["id"];
    if (typeof id !== "number") return;

    const pending = this.pending.get(id);
    if (!pending) return; // Response for an unknown or already-resolved request.

    this.pending.delete(id);
    if (pending.timer !== null) clearTimeout(pending.timer);

    if ("result" in response) {
      pending.resolve(response["result"]);
    } else if ("error" in response && isRpcErrorPayload(response["error"])) {
      pending.reject(new RpcError(response["error"] as RpcErrorPayload));
    } else {
      // Malformed response shape — treat as internal error.
      pending.reject(
        new RpcError({ code: -32603, message: "Internal error" })
      );
    }
  }

  /**
   * Call a Rust RPC method and return a typed Promise.
   *
   * @param method  Dot-namespaced method name, e.g. `"fs.write"`.
   * @param params  Method-specific parameters. Defaults to `{}`.
   */
  invoke<T>(method: string, params: unknown = {}): Promise<T> {
    if (!method || typeof method !== "string") {
      return Promise.reject(
        new RpcError({ code: -32600, message: "Invalid method name" })
      );
    }

    const id = this.allocateId();

    return new Promise<T>((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | null = null;

      if (this.timeoutMs > 0) {
        timer = setTimeout(() => {
          this.pending.delete(id);
          reject(
            new RpcError({
              code: -32603,
              message: `Request timed out: ${method}`,
            })
          );
        }, this.timeoutMs);
      }

      this.pending.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
        timer,
      });

      const request: RpcRequest = { id, method, params };
      this.transport.send(JSON.stringify(request));
    });
  }
}

// ── Type guard ────────────────────────────────────────────────────────────────

function isRpcErrorPayload(value: unknown): value is RpcErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as Record<string, unknown>)["code"] === "number" &&
    typeof (value as Record<string, unknown>)["message"] === "string"
  );
}

// ── Production WebView2 transport ─────────────────────────────────────────────

/**
 * WebView2 transport used when running inside the Axion runtime.
 *
 * Guards every access to `window.chrome.webview` with optional chaining so
 * that importing the SDK outside the Axion runtime (tests, plain browser)
 * does not throw. Calls issued outside the runtime are silently no-ops.
 */
export const webViewTransport: RpcTransport = {
  send(message: string): void {
    window.chrome?.webview?.postMessage(message);
  },
  onMessage(handler: (message: string) => void): void {
    window.chrome?.webview?.addEventListener(
      "message",
      (e: MessageEvent<string>) => {
        handler(e.data);
      }
    );
  },
};

// ── Singleton ─────────────────────────────────────────────────────────────────

/**
 * Singleton RPC client instance. All SDK capability modules use this to
 * communicate with the Rust runtime.
 */
export const rpc = new RpcClient(webViewTransport);
