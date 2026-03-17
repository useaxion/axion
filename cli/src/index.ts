#!/usr/bin/env node
/**
 * Axion CLI entry point — issue #24.
 *
 * Commands:
 *   axion build   — produce a distributable Windows .exe
 */

import { Command } from "commander";
import { build } from "./commands/build.js";

const program = new Command();

program
  .name("axion")
  .description("Axion — React-first desktop runtime for Windows")
  .version("0.1.0");

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
