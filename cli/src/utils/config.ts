/**
 * Config loader — reads and validates axion.config.json and permissions.json.
 */

import fs from "node:fs";
import path from "node:path";

export interface AxionConfig {
  name: string;
  version: string;
  description?: string;
  icon?: string;
}

export interface FsPermissions {
  appData?: boolean;
  userSelected?: boolean;
  absolutePath?: boolean;
}

export interface Permissions {
  fs?: FsPermissions;
  storage?: boolean;
  notifications?: boolean;
  system?: boolean;
  window?: boolean;
}

export interface ProjectConfig {
  axion: AxionConfig;
  permissions: Permissions;
}

/**
 * Load and validate both config files from the project root.
 * Throws with a clear, actionable message on any problem.
 */
export function loadProjectConfig(projectRoot: string): ProjectConfig {
  const axionConfigPath = path.join(projectRoot, "axion.config.json");
  const permissionsPath = path.join(projectRoot, "permissions.json");

  // ── axion.config.json ──────────────────────────────────────────────────────
  if (!fs.existsSync(axionConfigPath)) {
    throw new Error(
      `Missing axion.config.json in ${projectRoot}\n` +
        `  Create it with at minimum: { "name": "MyApp", "version": "1.0.0" }`
    );
  }

  let axion: AxionConfig;
  try {
    const raw = fs.readFileSync(axionConfigPath, "utf-8");
    axion = JSON.parse(raw) as AxionConfig;
  } catch (e) {
    throw new Error(`Failed to parse axion.config.json: ${String(e)}`);
  }

  if (!axion.name || typeof axion.name !== "string") {
    throw new Error(
      `axion.config.json is missing a required "name" field.\n` +
        `  Add: "name": "MyApp"`
    );
  }
  if (!axion.version || typeof axion.version !== "string") {
    throw new Error(
      `axion.config.json is missing a required "version" field.\n` +
        `  Add: "version": "1.0.0"`
    );
  }

  // ── permissions.json ───────────────────────────────────────────────────────
  if (!fs.existsSync(permissionsPath)) {
    throw new Error(
      `Missing permissions.json in ${projectRoot}\n` +
        `  Create it with at minimum: {} (empty object = no native capabilities)`
    );
  }

  let permissions: Permissions;
  try {
    const raw = fs.readFileSync(permissionsPath, "utf-8");
    permissions = JSON.parse(raw) as Permissions;
  } catch (e) {
    throw new Error(`Failed to parse permissions.json: ${String(e)}`);
  }

  return { axion, permissions };
}
