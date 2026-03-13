/**
 * fix-generated-types.mjs
 *
 * ts-rs emits `serde_json::Value → JsonValue` into a hardcoded path
 * (`core/bindings/serde_json/JsonValue.ts`) that lies outside the SDK's
 * TypeScript `rootDir`. This script rewrites that cross-package import to
 * `./JsonValue` so all generated types stay within `sdk/src/types/`.
 *
 * Run after `cargo test --lib`:
 *   node scripts/fix-generated-types.mjs
 */

import { readFileSync, writeFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const typesDir = resolve(__dirname, "../sdk/src/types");

const CROSS_PACKAGE_IMPORT =
  /"\.\.\/\.\.\/\.\.\/core\/bindings\/serde_json\/JsonValue"/g;
const LOCAL_IMPORT = '"./JsonValue"';

const files = ["RpcRequest.ts", "RpcResponse.ts"];

for (const file of files) {
  const path = resolve(typesDir, file);
  const original = readFileSync(path, "utf8");
  const patched = original.replace(CROSS_PACKAGE_IMPORT, LOCAL_IMPORT);

  if (patched !== original) {
    writeFileSync(path, patched, "utf8");
    console.log(`Patched import in ${file}`);
  }
}

console.log("Type import paths fixed.");
