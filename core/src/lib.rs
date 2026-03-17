/// Axion core library — public API surface.
///
/// Exposing modules here allows:
/// - `cargo test --lib` to trigger ts-rs type export to `sdk/src/types/`.
/// - Integration tests to import types without linking the binary.
/// - The `rpc::dispatcher::wire_to_bridge` function to resolve `crate::ipc`.
pub mod assets;
pub mod ipc;
pub mod module;
pub mod modules;
pub mod permissions;
pub mod rpc;
