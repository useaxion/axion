// Public API consumed by platform integration and the future RPC dispatcher.
// Items are intentionally unused until those callers land.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use crate::ipc::error::IpcError;

/// Maximum IPC message payload — 1 MiB.
///
/// Guards against memory exhaustion from malformed or malicious messages sent
/// through the WebView2 JS context. Any message exceeding this limit is
/// silently dropped on receive and rejected with an error on send.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

type Handler = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// Low-level IPC bridge between the WebView2 JavaScript context and the
/// Rust host. Every SDK call from React flows through this bridge.
///
/// `IpcBridge` is a platform-agnostic routing layer. The actual WebView2
/// send mechanism is injected at construction time via `js_sender`, keeping
/// all routing logic fully testable without a live WebView2 environment.
///
/// Share the bridge across threads using `Arc<IpcBridge>`.
pub struct IpcBridge {
    /// Handler invoked when a message arrives from JavaScript.
    handler: Arc<Mutex<Option<Handler>>>,
    /// Sends a string message into the WebView2 JS context.
    js_sender: Arc<dyn Fn(String) -> Result<(), IpcError> + Send + Sync + 'static>,
}

impl IpcBridge {
    /// Create a bridge.
    ///
    /// `js_sender` is called by [`send_to_js`] to write a message into the
    /// WebView2 JS context. In production this calls WebView2's
    /// `PostWebMessageAsString`; in tests it can be any mock closure.
    pub fn new(
        js_sender: impl Fn(String) -> Result<(), IpcError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            handler: Arc::new(Mutex::new(None)),
            js_sender: Arc::new(js_sender),
        }
    }

    /// Register the handler for messages arriving from JavaScript.
    ///
    /// A second call replaces the previously registered handler.
    pub fn on_message(&self, handler: impl Fn(String) + Send + Sync + 'static) {
        *self.handler.lock().expect("ipc handler lock poisoned") = Some(Arc::new(handler));
    }

    /// Route a raw message from the WebView2 callback to the registered handler.
    ///
    /// Called by the platform layer when `window.chrome.webview.postMessage()`
    /// fires in JavaScript. Messages exceeding [`MAX_MESSAGE_BYTES`] are
    /// silently dropped to prevent memory exhaustion.
    ///
    /// The handler is invoked *outside* the internal lock to prevent deadlocks
    /// if the handler itself re-enters the bridge (e.g. to call `send_to_js`).
    pub fn dispatch(&self, message: String) {
        if message.len() > MAX_MESSAGE_BYTES {
            return;
        }

        // Clone the Arc while holding the lock, then invoke outside the lock.
        let handler = self
            .handler
            .lock()
            .expect("ipc handler lock poisoned")
            .clone();

        if let Some(h) = handler {
            h(message);
        }
    }

    /// Send a message from Rust to JavaScript.
    ///
    /// Returns [`IpcError::MessageTooLarge`] if the payload exceeds
    /// [`MAX_MESSAGE_BYTES`], or [`IpcError::SendFailed`] if the underlying
    /// WebView2 call fails.
    pub fn send_to_js(&self, message: String) -> Result<(), IpcError> {
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(IpcError::MessageTooLarge {
                actual: message.len(),
                limit: MAX_MESSAGE_BYTES,
            });
        }
        (self.js_sender)(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Build a bridge whose sender captures outbound messages into `sent`.
    fn mock_bridge() -> (IpcBridge, Arc<Mutex<Vec<String>>>) {
        let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent.clone();
        let bridge = IpcBridge::new(move |msg| {
            sent_clone.lock().unwrap().push(msg);
            Ok(())
        });
        (bridge, sent)
    }

    #[test]
    fn js_to_rust_dispatch_reaches_handler() {
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        let (bridge, _sent) = mock_bridge();
        bridge.on_message(move |msg| received_clone.lock().unwrap().push(msg));

        bridge.dispatch("hello from js".to_string());

        assert_eq!(*received.lock().unwrap(), vec!["hello from js"]);
    }

    #[test]
    fn rust_to_js_send_reaches_sender() {
        let (bridge, sent) = mock_bridge();
        bridge.send_to_js("hello from rust".to_string()).unwrap();
        assert_eq!(*sent.lock().unwrap(), vec!["hello from rust"]);
    }

    #[test]
    fn round_trip_echo() {
        // Handler echoes every inbound message back to JS.
        let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent.clone();

        let bridge = Arc::new(IpcBridge::new(move |msg| {
            sent_clone.lock().unwrap().push(msg);
            Ok(())
        }));

        let bridge_for_handler = bridge.clone();
        bridge.on_message(move |msg| {
            bridge_for_handler
                .send_to_js(format!("echo:{msg}"))
                .unwrap();
        });

        bridge.dispatch("ping".to_string());
        assert_eq!(*sent.lock().unwrap(), vec!["echo:ping"]);
    }

    #[test]
    fn oversized_inbound_message_is_silently_dropped() {
        let handler_called = Arc::new(Mutex::new(false));
        let handler_called_clone = handler_called.clone();

        let (bridge, _) = mock_bridge();
        bridge.on_message(move |_| *handler_called_clone.lock().unwrap() = true);

        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);
        bridge.dispatch(oversized);

        assert!(
            !*handler_called.lock().unwrap(),
            "handler must not be called for an oversized inbound message"
        );
    }

    #[test]
    fn oversized_outbound_message_returns_error() {
        let (bridge, _) = mock_bridge();
        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let result = bridge.send_to_js(oversized);
        assert!(
            matches!(result, Err(IpcError::MessageTooLarge { .. })),
            "send_to_js must reject oversized messages"
        );
    }

    #[test]
    fn dispatch_without_handler_does_not_panic() {
        let (bridge, _) = mock_bridge();
        // No handler registered — must be a no-op, not a panic.
        bridge.dispatch("no handler yet".to_string());
    }

    #[test]
    fn handler_is_replaceable() {
        let first_called = Arc::new(Mutex::new(false));
        let second_called = Arc::new(Mutex::new(false));

        let first_clone = first_called.clone();
        let second_clone = second_called.clone();

        let (bridge, _) = mock_bridge();
        bridge.on_message(move |_| *first_clone.lock().unwrap() = true);
        bridge.on_message(move |_| *second_clone.lock().unwrap() = true);

        bridge.dispatch("msg".to_string());

        assert!(!*first_called.lock().unwrap(), "first handler must be replaced");
        assert!(*second_called.lock().unwrap(), "second handler must be called");
    }
}
