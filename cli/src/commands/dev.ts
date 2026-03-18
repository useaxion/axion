/**
 * axion dev — issue #28.
 *
 * Manages the full Vite + Axion runtime subprocess lifecycle:
 *   1. Validate axion.config.json
 *   2. Check port availability — clear error if port is already in use
 *   3. Spawn Vite dev server
 *   4. Poll until Vite responds (up to 30 s) — never shows a blank window
 *   5. Launch the Axion runtime in dev mode (AXION_DEV=1)
 *   6. Register SIGINT/SIGTERM handler — kills both processes cleanly
 */

import net from "node:net";
import path from "node:path";
import fs from "node:fs";
import { spawn, ChildProcess } from "node:child_process";
import { loadProjectConfig } from "../utils/config.js";

export interface DevOptions {
  /** Project root directory (defaults to cwd). */
  cwd?: string;
  /** Vite port (defaults to 5173). */
  port?: number;
}

const DEFAULT_PORT = 5173;
/** Maximum time to wait for Vite to respond before giving up. */
const READY_TIMEOUT_MS = 30_000;
/** How often to poll the Vite dev server during startup. */
const POLL_INTERVAL_MS = 250;

export async function dev(opts: DevOptions = {}): Promise<void> {
  const projectRoot = path.resolve(opts.cwd ?? process.cwd());
  const port = opts.port ?? DEFAULT_PORT;
  const devUrl = `http://localhost:${port}`;

  // ── Step 1: validate config ────────────────────────────────────────────────
  console.log("axion dev — validating project config...");
  let config;
  try {
    config = loadProjectConfig(projectRoot);
  } catch (err) {
    console.error(`\n  Error: ${String(err)}\n`);
    process.exit(1);
  }
  console.log(`  App: ${config.axion.name} v${config.axion.version}`);

  // ── Step 2: detect port conflict before starting anything ─────────────────
  if (await isPortInUse(port)) {
    console.error(
      `\n  Error: Port ${port} is already in use.\n` +
        `  Stop the process occupying port ${port}, or pass --port <n> to ` +
        `use a different port.\n`
    );
    process.exit(1);
  }

  // ── Step 3: spawn Vite dev server ─────────────────────────────────────────
  console.log(`\naxion dev — starting Vite on port ${port}...`);

  const vite = spawn("npx", ["vite", "--port", String(port)], {
    cwd: projectRoot,
    env: process.env,
    stdio: "inherit",
    shell: process.platform === "win32",
  });

  vite.on("error", (err) => {
    console.error(`\n  Error: Failed to start Vite: ${err.message}\n`);
    process.exit(1);
  });

  // If Vite exits on its own (e.g. unrecoverable error), propagate the code.
  vite.on("close", (code) => {
    if (code !== 0 && code !== null) {
      console.error(`\n  Vite exited with code ${code}\n`);
      process.exit(code);
    }
  });

  // ── Step 4: wait until Vite is ready ──────────────────────────────────────
  process.stdout.write(`\n  Waiting for dev server at ${devUrl}`);
  const ready = await pollUntilReady(devUrl, READY_TIMEOUT_MS, POLL_INTERVAL_MS);
  process.stdout.write("\n");

  if (!ready) {
    console.error(
      `\n  Error: Dev server did not become ready within ` +
        `${READY_TIMEOUT_MS / 1000} s at ${devUrl}.\n` +
        `  Check Vite output above for errors.\n`
    );
    vite.kill();
    process.exit(1);
  }

  console.log(`\n  ✔  Dev server ready → ${devUrl}`);

  // ── Step 5: launch Axion runtime in dev mode ───────────────────────────────
  console.log("\naxion dev — launching Axion runtime...");

  const cargoRoot = findCargoRoot(projectRoot);
  let runtime: ChildProcess | null = null;

  if (cargoRoot) {
    runtime = spawn("cargo", ["run", "--package", "axion-core"], {
      cwd: cargoRoot,
      env: {
        ...process.env,
        AXION_DEV: "1",
        AXION_DEV_URL: devUrl,
      },
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    runtime.on("error", (err) => {
      console.warn(
        `\n  Warning: Could not start Axion runtime: ${err.message}\n`
      );
    });
  } else {
    console.warn(
      "  Warning: Cargo workspace not found — runtime will not be launched.\n" +
        "  Run axion dev from inside your Axion project directory."
    );
  }

  // ── Step 6: clean shutdown on Ctrl+C / SIGTERM ────────────────────────────
  const cleanup = (signal: string) => {
    console.log(`\naxion dev — received ${signal}, shutting down...`);
    if (runtime && !runtime.killed) runtime.kill();
    if (!vite.killed) vite.kill();
    process.exit(0);
  };

  process.on("SIGINT", () => cleanup("SIGINT"));
  process.on("SIGTERM", () => cleanup("SIGTERM"));

  // Keep the CLI alive until both child processes exit.
  const pending: Promise<void>[] = [waitForClose(vite)];
  if (runtime) pending.push(waitForClose(runtime));
  await Promise.all(pending);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Returns `true` if a TCP listener is already bound on `port`.
 * Used to detect port conflicts before launching Vite.
 * Exported for testing.
 */
export function isPortInUse(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", (err: NodeJS.ErrnoException) => {
      resolve(err.code === "EADDRINUSE");
    });
    server.once("listening", () => {
      server.close(() => resolve(false));
    });
    server.listen(port, "127.0.0.1");
  });
}

/**
 * Poll `url` with GET requests every `intervalMs` until a response is received
 * or `timeoutMs` elapses. Returns `true` if the server became ready.
 * Exported for testing.
 */
export async function pollUntilReady(
  url: string,
  timeoutMs: number,
  intervalMs: number
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(intervalMs) });
      if (res.ok || res.status < 500) return true;
    } catch {
      // Not ready yet — keep polling.
    }
    process.stdout.write(".");
    await sleep(intervalMs);
  }
  return false;
}

/** Resolve when a child process closes (any exit code). */
function waitForClose(child: ChildProcess): Promise<void> {
  return new Promise((resolve) => child.on("close", () => resolve()));
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Walk up from `startDir` looking for a Cargo.toml with a [workspace] section.
 * Falls back to the first directory that contains any Cargo.toml.
 */
function findCargoRoot(startDir: string): string | null {
  let dir = startDir;
  let fallback: string | null = null;

  // eslint-disable-next-line no-constant-condition
  while (true) {
    const cargoToml = path.join(dir, "Cargo.toml");
    if (fs.existsSync(cargoToml)) {
      const content = fs.readFileSync(cargoToml, "utf-8");
      if (content.includes("[workspace]")) return dir;
      if (!fallback) fallback = dir;
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  return fallback;
}
