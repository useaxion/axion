// Public API — used by platform layer and future RPC dispatcher.
#![allow(dead_code)]

/// Errors produced by the IPC bridge.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// The message payload exceeds the configured size limit.
    /// Prevents memory exhaustion from malformed or malicious messages.
    #[error("message size {actual} bytes exceeds the {limit} byte limit")]
    MessageTooLarge { actual: usize, limit: usize },

    /// The underlying WebView2 send call failed.
    #[error("failed to send message to JavaScript: {0}")]
    SendFailed(String),
}
