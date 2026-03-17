/// Built-in native modules — issue #12 (trait) through #18 (integration tests).
///
/// Each module lives in its own file and implements [`crate::module::AxionModule`].
/// The `build_registry` function creates a fully configured [`crate::module::ModuleRegistry`]
/// containing all five built-in modules for use in the main startup sequence.
pub mod fs;
pub mod notifications;
pub mod storage;
pub mod system;
