/**
 * axion build — issue #24.
 *
 * Produces a distributable Windows executable from an Axion project:
 *
 *   1. Validate axion.config.json + permissions.json
 *   2. Run `vite build` → frontend bundle
 *   3. Run `cargo build --release` → Rust binary
 *   4. Copy binary to dist/<AppName>.exe
 *   5. Print build summary (output path + binary size)
 */

import fs from "node:fs";
import path from "node:path";
import { loadProjectConfig } from "../utils/config.js";
import { run } from "../utils/spawn.js";

export interface BuildOptions {
  /** Project root (defaults to cwd). */
  cwd?: string;
}

export async function build(opts: BuildOptions = {}): Promise<void> {
  const projectRoot = path.resolve(opts.cwd ?? process.cwd());

  // ── Step 1: validate config ────────────────────────────────────────────────
  console.log("axion build — validating project config...");
  let config;
  try {
    config = loadProjectConfig(projectRoot);
  } catch (err) {
    console.error(`\n  Error: ${String(err)}\n`);
    process.exit(1);
  }

  const { name, version } = config.axion;
  console.log(`  App: ${name} v${version}`);

  // ── Step 2: vite build ─────────────────────────────────────────────────────
  console.log("\naxion build — running vite build...");
  const viteExitCode = await run("npx", ["vite", "build"], {
    cwd: projectRoot,
    label: "[vite]",
  });

  if (viteExitCode !== 0) {
    console.error(`\n  Error: vite build failed (exit code ${viteExitCode})\n`);
    process.exit(viteExitCode);
  }

  // ── Step 3: cargo build --release ─────────────────────────────────────────
  console.log("\naxion build — compiling Rust runtime (release)...");

  // Find the Cargo workspace root (search upward from project root).
  const cargoRoot = findCargoRoot(projectRoot);
  if (!cargoRoot) {
    console.error(
      "\n  Error: Could not find Cargo.toml. Run axion build from an Axion project directory.\n"
    );
    process.exit(1);
  }

  const cargoEnv: NodeJS.ProcessEnv = {
    ...process.env,
    AXION_APP_NAME: name,
    AXION_APP_VERSION: version,
    AXION_FRONTEND_DIST: path.join(projectRoot, "dist"),
  };

  const cargoExitCode = await run(
    "cargo",
    ["build", "--release", "--package", "axion-core"],
    {
      cwd: cargoRoot,
      env: cargoEnv,
      label: "[cargo]",
    }
  );

  if (cargoExitCode !== 0) {
    console.error(
      `\n  Error: cargo build failed (exit code ${cargoExitCode})\n`
    );
    process.exit(cargoExitCode);
  }

  // ── Step 4: copy binary to dist/<AppName>.exe ──────────────────────────────
  console.log("\naxion build — copying output binary...");

  const releaseBinary = path.join(
    cargoRoot,
    "target",
    "release",
    "axion-core.exe"
  );

  if (!fs.existsSync(releaseBinary)) {
    console.error(
      `\n  Error: Expected release binary not found at:\n  ${releaseBinary}\n`
    );
    process.exit(1);
  }

  const outputDir = path.join(projectRoot, "dist");
  fs.mkdirSync(outputDir, { recursive: true });

  // Sanitize app name for use as a filename.
  const safeName = name.replace(/[^a-zA-Z0-9._-]/g, "_");
  const outputExe = path.join(outputDir, `${safeName}.exe`);

  fs.copyFileSync(releaseBinary, outputExe);

  // ── Step 5: build summary ──────────────────────────────────────────────────
  const stats = fs.statSync(outputExe);
  const sizeMb = (stats.size / 1024 / 1024).toFixed(2);

  console.log("\n  ✔ Build complete!\n");
  console.log(`  Output : ${outputExe}`);
  console.log(`  Size   : ${sizeMb} MB`);

  const SIZE_WARN_MB = 20;
  if (stats.size > SIZE_WARN_MB * 1024 * 1024) {
    console.warn(
      `\n  Warning: Binary size (${sizeMb} MB) exceeds the recommended ${SIZE_WARN_MB} MB limit.`
    );
  }

  console.log();
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Walk up from `startDir` looking for a directory that contains a Cargo.toml
 * with a [workspace] section (i.e. the monorepo root).
 * Falls back to the first directory containing any Cargo.toml.
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
    if (parent === dir) break; // filesystem root
    dir = parent;
  }

  return fallback;
}
