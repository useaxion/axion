#!/usr/bin/env node
/**
 * Axion CLI entry point.
 *
 * Commands:
 *   axion dev     — start Vite dev server + Axion runtime
 *   axion build   — produce a distributable Windows .exe
 */

import { Command } from "commander";
import { build } from "./commands/build.js";
import { dev } from "./commands/dev.js";

const program = new Command();

program
  .name("axion")
  .description("Axion — React-first desktop runtime for Windows")
  .version("0.1.0");

program
  .command("dev")
  .description("Start Vite dev server and the Axion runtime together")
  .option(
    "--cwd <path>",
    "Project root directory (defaults to current working directory)"
  )
  .option("--port <n>", "Vite dev server port (defaults to 5173)", parseInt)
  .action(async (opts: { cwd?: string; port?: number }) => {
    await dev({ cwd: opts.cwd, port: opts.port });
  });

program
  .command("build")
  .description(
    "Build a distributable Windows executable from the current Axion project"
  )
  .option(
    "--cwd <path>",
    "Project root directory (defaults to current working directory)"
  )
  .action(async (opts: { cwd?: string }) => {
    await build({ cwd: opts.cwd });
  });

program.parse(process.argv);
