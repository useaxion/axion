import { describe, it, expect, vi, beforeEach } from "vitest";
import { RpcClient, RpcError } from "./rpc-client";
import type { RpcTransport } from "./rpc-client";

// ── Mock transport factory ────────────────────────────────────────────────────

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
    /** Messages captured by send(). */
    sent,
    /** Simulate an incoming message from the Rust runtime. */
    receive(data: string) {
      messageHandler?.(data);
    },
    /** Parse the most-recently sent message as JSON. */
    lastRequest(): { id: number; method: string; params: unknown } {
      return JSON.parse(sent[sent.length - 1]);
    },
  };
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Create a client with timeouts disabled (cleaner unit tests). */
function makeClient(transport: RpcTransport) {
  return new RpcClient(transport, { timeoutMs: 0 });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("RpcClient", () => {
  // ── Request shape ───────────────────────────────────────────────────────────

  it("sends a request with id, method, and params", async () => {
    const { transport, sent, receive } = createMockTransport();
    const client = makeClient(transport);

    const promise = client.invoke("test.method", { key: "value" });
    const req = JSON.parse(sent[0]);

    expect(typeof req.id).toBe("number");
    expect(req.method).toBe("test.method");
    expect(req.params).toEqual({ key: "value" });

    receive(JSON.stringify({ id: req.id, result: null }));
    await promise;
  });

  it("defaults params to {} when not provided", async () => {
    const { transport, sent, receive } = createMockTransport();
    const client = makeClient(transport);

    const promise = client.invoke("system.info");
    const req = JSON.parse(sent[0]);

    expect(req.params).toEqual({});
    receive(JSON.stringify({ id: req.id, result: null }));
    await promise;
  });

  // ── Success path ────────────────────────────────────────────────────────────

  it("resolves with the result on a success response", async () => {
    const { transport, receive, lastRequest } = createMockTransport();
    const client = makeClient(transport);

    const promise = client.invoke<{ theme: string }>("storage.get", {
      key: "theme",
    });
    receive(
      JSON.stringify({ id: lastRequest().id, result: { theme: "dark" } })
    );

    await expect(promise).resolves.toEqual({ theme: "dark" });
  });

  // ── Error path ──────────────────────────────────────────────────────────────

  it("rejects with RpcError on an error response", async () => {
    const { transport, receive, lastRequest } = createMockTransport();
    const client = makeClient(transport);

    const promise = client.invoke("fs.write", {});
    receive(
      JSON.stringify({
        id: lastRequest().id,
        error: { code: -32000, message: "Permission denied: fs.write" },
      })
    );

    const err = await promise.catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RpcError);
    expect((err as RpcError).code).toBe(-32000);
    expect((err as RpcError).message).toBe("Permission denied: fs.write");
  });

  it("rejects with INTERNAL_ERROR for a malformed response shape", async () => {
    const { transport, receive, lastRequest } = createMockTransport();
    const client = makeClient(transport);

    const promise = client.invoke("any.method");
    // Response has neither `result` nor a valid `error` object.
    receive(JSON.stringify({ id: lastRequest().id, weirdField: true }));

    const err = await promise.catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RpcError);
    expect((err as RpcError).code).toBe(-32603);
  });

  // ── Correlation ─────────────────────────────────────────────────────────────

  it("correlates responses to the correct in-flight request by id", async () => {
    const { transport, sent, receive } = createMockTransport();
    const client = makeClient(transport);

    const p1 = client.invoke<string>("a.one");
    const p2 = client.invoke<string>("a.two");

    const req1 = JSON.parse(sent[0]);
    const req2 = JSON.parse(sent[1]);

    // Respond in reverse order.
    receive(JSON.stringify({ id: req2.id, result: "second" }));
    receive(JSON.stringify({ id: req1.id, result: "first" }));

    await expect(p1).resolves.toBe("first");
    await expect(p2).resolves.toBe("second");
  });

  it("ignores responses with an unknown id", () => {
    const { transport, receive } = createMockTransport();
    makeClient(transport);

    // Must not throw.
    expect(() =>
      receive(JSON.stringify({ id: 99999, result: "orphan" }))
    ).not.toThrow();
  });

  it("ignores malformed JSON responses", () => {
    const { transport, receive } = createMockTransport();
    makeClient(transport);

    expect(() => receive("not valid json {")).not.toThrow();
  });

  // ── Request ID counter ──────────────────────────────────────────────────────

  it("increments the request id on each call", async () => {
    const { transport, sent, receive } = createMockTransport();
    const client = makeClient(transport);

    const p1 = client.invoke("a.b");
    const p2 = client.invoke("c.d");

    const id1 = JSON.parse(sent[0]).id as number;
    const id2 = JSON.parse(sent[1]).id as number;

    expect(id2).toBe(id1 + 1);

    receive(JSON.stringify({ id: id1, result: null }));
    receive(JSON.stringify({ id: id2, result: null }));
    await Promise.all([p1, p2]);
  });

  // ── Timeout ─────────────────────────────────────────────────────────────────

  it("rejects with RpcError when the request times out", async () => {
    vi.useFakeTimers();
    const { transport } = createMockTransport();
    const client = new RpcClient(transport, { timeoutMs: 100 });

    const promise = client.invoke("slow.op");
    vi.advanceTimersByTime(101);

    const err = await promise.catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RpcError);
    expect((err as RpcError).message).toContain("slow.op");

    vi.useRealTimers();
  });

  it("removes a timed-out request from pending so a late response is ignored", async () => {
    vi.useFakeTimers();
    const { transport, receive, lastRequest } = createMockTransport();
    const client = new RpcClient(transport, { timeoutMs: 100 });

    const promise = client.invoke("slow.op").catch(() => "timed out");
    const id = lastRequest().id;

    vi.advanceTimersByTime(101);
    await promise;

    // Late response should not cause an unhandled rejection.
    expect(() =>
      receive(JSON.stringify({ id, result: "too late" }))
    ).not.toThrow();

    vi.useRealTimers();
  });

  // ── Input validation ────────────────────────────────────────────────────────

  it("rejects immediately for an empty method name", async () => {
    const { transport } = createMockTransport();
    const client = makeClient(transport);

    await expect(client.invoke("")).rejects.toBeInstanceOf(RpcError);
  });

  it("rejects immediately for a non-string method", async () => {
    const { transport } = createMockTransport();
    const client = makeClient(transport);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await expect(client.invoke(null as any)).rejects.toBeInstanceOf(RpcError);
  });
});
