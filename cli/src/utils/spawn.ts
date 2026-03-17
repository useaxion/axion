/**
 * Subprocess helpers — thin wrappers over child_process.spawn that stream
 * stdout/stderr to the parent terminal and return an exit-code Promise.
 */

import { spawn } from "node:child_process";

export interface SpawnOptions {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  /** Label prepended to stderr output lines, e.g. "[cargo]". */
  label?: string;
}

/**
 * Spawn a command, inherit stdio, and resolve with the exit code.
 * Rejects only on spawn errors (e.g. command not found).
 */
export function run(
  cmd: string,
  args: string[],
  opts: SpawnOptions = {}
): Promise<number> {
  return new Promise((resolve, reject) => {
    const label = opts.label ? `${opts.label} ` : "";
    const child = spawn(cmd, args, {
      cwd: opts.cwd ?? process.cwd(),
      env: opts.env ?? process.env,
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    child.on("error", (err) => {
      reject(
        new Error(
          `${label}Failed to start '${cmd}': ${err.message}\n` +
            `  Make sure '${cmd}' is installed and in your PATH.`
        )
      );
    });

    child.on("close", (code) => {
      resolve(code ?? 1);
    });
  });
}
