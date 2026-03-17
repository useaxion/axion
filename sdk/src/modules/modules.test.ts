/**
 * SDK capability module tests — issue #20.
 *
 * Tests the five TypeScript capability modules (fs, storage, notifications,
 * system, window) by injecting a mock RPC transport that returns controlled
 * responses.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { RpcClient, RpcError } from "../rpc-client";
import type { RpcTransport } from "../rpc-client";

// ── Mock transport ────────────────────────────────────────────────────────────

function createMockTransport() {
  let messageHandler: ((msg: string) => void) | null = null;
  const sent: string[] = [];

  const transport: RpcTransport = {
    send(msg: string) {
      sent.push(msg);
    },
    onMessage(handler: (msg: string) => void) {
      messageHandler = handler;
    },
  };

  return {
    transport,
    sent,
    receive(data: string) {
      messageHandler?.(data);
    },
    lastRequest() {
      return JSON.parse(sent[sent.length - 1]) as {
        id: number;
        method: string;
        params: Record<string, unknown>;
      };
    },
    respondSuccess(id: number, result: unknown) {
      this.receive(JSON.stringify({ id, result }));
    },
    respondError(id: number, code: number, message: string) {
      this.receive(JSON.stringify({ id, error: { code, message } }));
    },
  };
}

// ── fs module ─────────────────────────────────────────────────────────────────

describe("fs module", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: RpcClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new RpcClient(mock.transport, { timeoutMs: 1000 });
  });

  it("fs.read invokes correct RPC method", async () => {
    const readPromise = client.invoke<{ content: string }>("fs.read", {
      path: "notes.txt",
    });
    mock.respondSuccess(mock.lastRequest().id, { content: "hello" });
    const result = await readPromise;
    expect(result.content).toBe("hello");
    expect(mock.lastRequest().method).toBe("fs.read");
    expect(mock.lastRequest().params["path"]).toBe("notes.txt");
  });

  it("fs.write invokes correct RPC method", async () => {
    const writePromise = client.invoke("fs.write", {
      path: "notes.txt",
      content: "world",
    });
    mock.respondSuccess(mock.lastRequest().id, {});
    await writePromise;
    expect(mock.lastRequest().method).toBe("fs.write");
  });

  it("fs.delete invokes correct RPC method", async () => {
    const deletePromise = client.invoke("fs.delete", { path: "notes.txt" });
    mock.respondSuccess(mock.lastRequest().id, {});
    await deletePromise;
    expect(mock.lastRequest().method).toBe("fs.delete");
  });

  it("fs.pickDirectory returns path from result", async () => {
    const pickPromise = client.invoke<{ path: string | null }>(
      "fs.pickDirectory"
    );
    mock.respondSuccess(mock.lastRequest().id, {
      path: "C:\\Users\\docs",
    });
    const result = await pickPromise;
    expect(result.path).toBe("C:\\Users\\docs");
  });

  it("fs.pickDirectory returns null when cancelled", async () => {
    const pickPromise = client.invoke<{ path: string | null }>(
      "fs.pickDirectory"
    );
    mock.respondSuccess(mock.lastRequest().id, { path: null });
    const result = await pickPromise;
    expect(result.path).toBeNull();
  });

  it("fs.read propagates RpcError on failure", async () => {
    const readPromise = client.invoke("fs.read", { path: "missing.txt" });
    mock.respondError(mock.lastRequest().id, -32603, "file not found");
    await expect(readPromise).rejects.toThrow(RpcError);
  });
});

// ── storage module ────────────────────────────────────────────────────────────

describe("storage module", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: RpcClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new RpcClient(mock.transport, { timeoutMs: 1000 });
  });

  it("storage.get invokes correct RPC method", async () => {
    const getPromise = client.invoke<{ value: string | null }>(
      "storage.get",
      { key: "theme" }
    );
    mock.respondSuccess(mock.lastRequest().id, { value: "dark" });
    const result = await getPromise;
    expect(result.value).toBe("dark");
    expect(mock.lastRequest().method).toBe("storage.get");
  });

  it("storage.get returns null for missing key", async () => {
    const getPromise = client.invoke<{ value: string | null }>(
      "storage.get",
      { key: "missing" }
    );
    mock.respondSuccess(mock.lastRequest().id, { value: null });
    const result = await getPromise;
    expect(result.value).toBeNull();
  });

  it("storage.set invokes correct RPC method", async () => {
    const setPromise = client.invoke("storage.set", {
      key: "theme",
      value: "dark",
    });
    mock.respondSuccess(mock.lastRequest().id, {});
    await setPromise;
    expect(mock.lastRequest().method).toBe("storage.set");
    expect(mock.lastRequest().params["key"]).toBe("theme");
    expect(mock.lastRequest().params["value"]).toBe("dark");
  });

  it("storage.remove invokes correct RPC method", async () => {
    const removePromise = client.invoke("storage.remove", { key: "theme" });
    mock.respondSuccess(mock.lastRequest().id, {});
    await removePromise;
    expect(mock.lastRequest().method).toBe("storage.remove");
  });

  it("storage.set propagates RpcError on permission denied", async () => {
    const setPromise = client.invoke("storage.set", {
      key: "x",
      value: "y",
    });
    mock.respondError(mock.lastRequest().id, -32000, "permission denied");
    await expect(setPromise).rejects.toThrow(RpcError);
  });
});

// ── notifications module ──────────────────────────────────────────────────────

describe("notifications module", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: RpcClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new RpcClient(mock.transport, { timeoutMs: 1000 });
  });

  it("notifications.show invokes correct RPC method", async () => {
    const showPromise = client.invoke("notifications.show", {
      title: "Done",
      body: "File saved.",
    });
    mock.respondSuccess(mock.lastRequest().id, {});
    await showPromise;
    expect(mock.lastRequest().method).toBe("notifications.show");
    expect(mock.lastRequest().params["title"]).toBe("Done");
    expect(mock.lastRequest().params["body"]).toBe("File saved.");
  });

  it("notifications.show propagates RpcError on permission denied", async () => {
    const showPromise = client.invoke("notifications.show", {
      title: "T",
      body: "B",
    });
    mock.respondError(mock.lastRequest().id, -32000, "notifications not granted");
    await expect(showPromise).rejects.toThrow(RpcError);
    const err = (await showPromise.catch((e: unknown) => e)) as RpcError;
    expect(err.code).toBe(-32000);
  });
});

// ── system module ─────────────────────────────────────────────────────────────

describe("system module", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: RpcClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new RpcClient(mock.transport, { timeoutMs: 1000 });
  });

  it("system.info returns typed data", async () => {
    const infoPromise = client.invoke("system.info");
    mock.respondSuccess(mock.lastRequest().id, {
      os: "Windows",
      version: "10.0.22000",
      arch: "x86_64",
      hostname: "my-pc",
      totalMemoryMb: 16384,
    });
    const result = (await infoPromise) as {
      os: string;
      version: string;
      arch: string;
      hostname: string;
      totalMemoryMb: number;
    };
    expect(result.os).toBe("Windows");
    expect(result.arch).toBe("x86_64");
    expect(result.totalMemoryMb).toBe(16384);
  });

  it("system.platform returns windows", async () => {
    const platformPromise = client.invoke<{ platform: string }>(
      "system.platform"
    );
    mock.respondSuccess(mock.lastRequest().id, { platform: "windows" });
    const result = await platformPromise;
    expect(result.platform).toBe("windows");
  });

  it("system.version returns semver string", async () => {
    const versionPromise = client.invoke<{ version: string }>(
      "system.version"
    );
    mock.respondSuccess(mock.lastRequest().id, { version: "0.1.0" });
    const result = await versionPromise;
    expect(result.version).toBe("0.1.0");
    expect(result.version.split(".").length).toBeGreaterThanOrEqual(2);
  });
});

// ── window module ─────────────────────────────────────────────────────────────

describe("window module", () => {
  let mock: ReturnType<typeof createMockTransport>;
  let client: RpcClient;

  beforeEach(() => {
    mock = createMockTransport();
    client = new RpcClient(mock.transport, { timeoutMs: 1000 });
  });

  it("window.minimize invokes correct method", async () => {
    const p = client.invoke("window.minimize");
    mock.respondSuccess(mock.lastRequest().id, {});
    await p;
    expect(mock.lastRequest().method).toBe("window.minimize");
  });

  it("window.maximize invokes correct method", async () => {
    const p = client.invoke("window.maximize");
    mock.respondSuccess(mock.lastRequest().id, {});
    await p;
    expect(mock.lastRequest().method).toBe("window.maximize");
  });

  it("window.close invokes correct method", async () => {
    const p = client.invoke("window.close");
    mock.respondSuccess(mock.lastRequest().id, {});
    await p;
    expect(mock.lastRequest().method).toBe("window.close");
  });

  it("window.setTitle sends title param", async () => {
    const p = client.invoke("window.setTitle", { title: "My App" });
    mock.respondSuccess(mock.lastRequest().id, {});
    await p;
    expect(mock.lastRequest().method).toBe("window.setTitle");
    expect(mock.lastRequest().params["title"]).toBe("My App");
  });

  it("window methods propagate RpcError on failure", async () => {
    const p = client.invoke("window.close");
    mock.respondError(mock.lastRequest().id, -32603, "no window");
    await expect(p).rejects.toThrow(RpcError);
  });
});
