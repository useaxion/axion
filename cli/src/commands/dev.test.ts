/**
 * Tests for axion dev lifecycle helpers — issue #28.
 *
 * Uses real network I/O (TCP / HTTP) so there are no mocks to drift.
 */

import net from "node:net";
import http from "node:http";
import { describe, it, expect, afterEach } from "vitest";
import { isPortInUse, pollUntilReady } from "./dev.js";

// ── isPortInUse ───────────────────────────────────────────────────────────────

describe("isPortInUse", () => {
  // Pick a high ephemeral port unlikely to be in use in CI.
  const TEST_PORT = 54_321;
  let server: net.Server | null = null;

  afterEach(
    () =>
      new Promise<void>((resolve) => {
        if (server) {
          server.close(() => {
            server = null;
            resolve();
          });
        } else {
          resolve();
        }
      })
  );

  it("returns false when nothing is listening on the port", async () => {
    expect(await isPortInUse(TEST_PORT)).toBe(false);
  });

  it("returns true when a TCP server is already listening", async () => {
    server = net.createServer();
    await new Promise<void>((resolve) =>
      server!.listen(TEST_PORT, "127.0.0.1", () => resolve())
    );
    expect(await isPortInUse(TEST_PORT)).toBe(true);
  });

  it("returns false after the server is closed", async () => {
    // Start then stop — port should be free again.
    const tmp = net.createServer();
    await new Promise<void>((resolve) =>
      tmp.listen(TEST_PORT, "127.0.0.1", () => resolve())
    );
    await new Promise<void>((resolve) => tmp.close(() => resolve()));
    expect(await isPortInUse(TEST_PORT)).toBe(false);
  });
});

// ── pollUntilReady ────────────────────────────────────────────────────────────

describe("pollUntilReady", () => {
  const TEST_PORT = 54_322;
  let httpServer: http.Server | null = null;

  afterEach(
    () =>
      new Promise<void>((resolve) => {
        if (httpServer) {
          httpServer.close(() => {
            httpServer = null;
            resolve();
          });
        } else {
          resolve();
        }
      })
  );

  it("returns true immediately when the server is already up", async () => {
    httpServer = http.createServer((_req, res) => {
      res.writeHead(200);
      res.end("ok");
    });
    await new Promise<void>((resolve) =>
      httpServer!.listen(TEST_PORT, "127.0.0.1", () => resolve())
    );

    const ready = await pollUntilReady(
      `http://127.0.0.1:${TEST_PORT}`,
      5_000,
      100
    );
    expect(ready).toBe(true);
  });

  it("returns false when the server never starts within the timeout", async () => {
    // Nothing listening — must time out quickly.
    const ready = await pollUntilReady(
      `http://127.0.0.1:${TEST_PORT}`,
      500, // very short timeout
      100
    );
    expect(ready).toBe(false);
  });

  it("returns true once the server becomes available mid-poll", async () => {
    // Start the server 300 ms after polling begins.
    const startDelay = 300;
    const timeout = 3_000;

    setTimeout(() => {
      httpServer = http.createServer((_req, res) => {
        res.writeHead(200);
        res.end("ok");
      });
      httpServer.listen(TEST_PORT, "127.0.0.1");
    }, startDelay);

    const ready = await pollUntilReady(
      `http://127.0.0.1:${TEST_PORT}`,
      timeout,
      100
    );
    expect(ready).toBe(true);
  });
});
