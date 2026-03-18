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

  const { name, version, description, icon } = config.axion;
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

  // ── Step 2b: copy Vite output into core/frontend-dist/ for rust-embed ───────
  console.log("\naxion build — staging frontend assets for embedding...");
  const cargoRoot = findCargoRoot(projectRoot);
  if (!cargoRoot) {
    console.error(
      "\n  Error: Could not find Cargo.toml. Run axion build from an Axion project directory.\n"
    );
    process.exit(1);
  }

  const viteDist = path.join(projectRoot, "dist");
  const embedTarget = path.join(cargoRoot, "core", "frontend-dist");

  // Clear stale assets then copy fresh ones.
  if (fs.existsSync(embedTarget)) {
    fs.rmSync(embedTarget, { recursive: true });
  }
  fs.mkdirSync(embedTarget, { recursive: true });
  copyDir(viteDist, embedTarget);
  console.log(`  Staged ${countFiles(embedTarget)} asset(s) → ${embedTarget}`);

  // ── Step 3: cargo build --release ─────────────────────────────────────────
  console.log("\naxion build — compiling Rust runtime (release)...");

  const cargoEnv: NodeJS.ProcessEnv = {
    ...process.env,
    AXION_APP_NAME: name,
    AXION_APP_VERSION: version,
    AXION_FRONTEND_DIST: path.join(projectRoot, "dist"),
    ...(description ? { AXION_APP_DESCRIPTION: description } : {}),
    ...(icon ? { AXION_APP_ICON: path.resolve(projectRoot, icon) } : {}),
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

/** Recursively copy all files from `src` into `dest` (must already exist). */
function copyDir(src: string, dest: string): void {
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      fs.mkdirSync(destPath, { recursive: true });
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

/** Count all files in a directory tree. */
function countFiles(dir: string): number {
  let count = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      count += countFiles(path.join(dir, entry.name));
    } else {
      count++;
    }
  }
  return count;
}

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
