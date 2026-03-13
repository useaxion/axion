# TypeScript Type Generation

Axion auto-generates TypeScript type definitions from Rust structs using the
[`ts-rs`](https://github.com/Aleph-Alpha/ts-rs) crate. TypeScript types must
**never be written by hand** — they are always derived from Rust definitions
so both layers stay in sync.

---

## How it works

1. Rust structs and enums in `core/src/rpc/schema.rs` are annotated with
   `#[derive(TS)]`. A single `#[test]` (`type_export::export_rpc_types`) calls
   `export_all_to("sdk/src/types")` for each type.
2. Running `cargo test --lib` triggers ts-rs to emit:
   - `sdk/src/types/RpcRequest.ts`
   - `sdk/src/types/RpcErrorPayload.ts`
   - `sdk/src/types/RpcResponse.ts`
   - `core/bindings/serde_json/JsonValue.ts` (ts-rs hardcodes this path for
     `serde_json::Value` — it cannot be redirected via the public API)
3. `scripts/fix-generated-types.mjs` rewrites the cross-package import in
   `RpcRequest.ts` and `RpcResponse.ts` from
   `../../../core/bindings/serde_json/JsonValue` → `./JsonValue`.
   `sdk/src/types/JsonValue.ts` is a stable committed file that mirrors the
   ts-rs-generated definition.
4. The files in `sdk/src/types/` are **committed to the repository**. CI
   enforces that the committed files always match fresh generation.

---

## Regenerating types

Run from the repo root whenever you change an RPC struct or add a new one:

```sh
cargo test --lib
node scripts/fix-generated-types.mjs
```

Then commit the updated files in `sdk/src/types/`.

---

## Adding a new exported type

1. Add `#[derive(TS)]` and `#[ts(export, export_to = "../../sdk/src/types/")]`
   to your Rust struct or enum in `core/src/`.
2. Run `cargo test --lib` to regenerate.
3. Commit the new `.ts` file alongside your Rust changes.

### `u64` / `i64` fields

JSON numbers in JavaScript are IEEE 754 doubles, so values above
`Number.MAX_SAFE_INTEGER` lose precision. For fields that will never exceed
that limit in practice (e.g. request IDs), override the TypeScript type:

```rust
#[ts(type = "number")]
pub id: u64,
```

---

## CI enforcement

The CI workflow runs `cargo test --lib` and then checks:

```sh
git diff --exit-code sdk/src/types/ core/bindings/
```

If the generated files differ from what is committed, the build fails.
To fix a failing CI check, run `cargo test --lib` locally and commit the
updated files.

---

## Design rationale

- **Single source of truth**: all type shapes are defined in Rust. TypeScript
  types are derived outputs, never authored directly.
- **ts-rs over manual generation**: ts-rs uses Rust's derive macro system,
  meaning it sees the actual struct layout and serde attributes — it cannot
  get out of sync with the real serialization format.
- **Committed generated files**: generated files are committed (not
  `.gitignore`d) so that reviewers can see type changes in PRs without
  running the Rust toolchain.
