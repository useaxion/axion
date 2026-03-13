/// WebView2-specific IPC wiring (Windows only).
///
/// This module connects a platform-agnostic [`IpcBridge`] to a live
/// `ICoreWebView2` instance. All unsafe COM interactions are isolated here so
/// the rest of the runtime stays safe.
///
/// **Thread safety**: every function in this module must be called from the
/// thread that owns the WebView2 controller (the UI / main thread). Once
/// wired, the `Arc<IpcBridge>` is safe to share across threads.
#[cfg(windows)]
pub mod webview2 {
    use std::sync::Arc;

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2WebMessageReceivedEventArgs,
    };
    use webview2_com::WebMessageReceivedEventHandler;
    use windows::{
        core::{HSTRING, PWSTR},
        Win32::{Foundation::E_POINTER, System::WinRT::EventRegistrationToken},
    };

    use crate::ipc::{bridge::IpcBridge, error::IpcError};

    /// Newtype wrapper that makes [`ICoreWebView2`] sendable across threads.
    ///
    /// # Safety
    /// `PostWebMessageAsString` is documented by Microsoft as thread-safe —
    /// it may be called from any thread. The COM apartment must be initialised
    /// before this wrapper is constructed and must remain alive for as long as
    /// any instance of the wrapper exists.
    struct SendableWebView(ICoreWebView2);

    // SAFETY: see struct-level doc comment above.
    unsafe impl Send for SendableWebView {}
    unsafe impl Sync for SendableWebView {}

    impl SendableWebView {
        /// Post a string message into the WebView2 JavaScript context.
        ///
        /// Calling via this method ensures Rust 2021 closures capture the
        /// whole `SendableWebView` (which is `Send + Sync`) rather than the
        /// inner `ICoreWebView2` field (which is not).
        fn post_message(&self, message: &HSTRING) -> windows::core::Result<()> {
            // SAFETY: `PostWebMessageAsString` is thread-safe per WebView2 docs.
            unsafe { self.0.PostWebMessageAsString(message) }
        }
    }

    /// Wire an [`IpcBridge`] to a live WebView2 instance.
    ///
    /// After this call:
    /// - Messages posted from JS via `window.chrome.webview.postMessage(msg)`
    ///   are routed to the bridge's registered handler.
    /// - Messages sent with [`IpcBridge::send_to_js`] arrive in JS via the
    ///   `window.chrome.webview.addEventListener('message', ...)` event.
    ///
    /// Returns the `EventRegistrationToken` so the caller can deregister the
    /// handler during shutdown.
    ///
    /// # Safety
    /// Must be called on the UI thread. `webview` must be a valid, initialized
    /// `ICoreWebView2` instance.
    pub fn attach(
        webview: ICoreWebView2,
        bridge: Arc<IpcBridge>,
    ) -> Result<EventRegistrationToken, IpcError> {
        let dispatch_bridge = bridge.clone();

        let handler =
            WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
                let args: ICoreWebView2WebMessageReceivedEventArgs = args
                    .ok_or_else(|| windows::core::Error::from(E_POINTER))?;

                // SAFETY: `TryGetWebMessageAsString` is safe to call on the
                // UI thread from within the WebMessageReceived callback.
                // webview2-com 0.28 uses an out-parameter (*mut PWSTR) rather
                // than a direct return value.
                let mut raw = PWSTR::null();
                unsafe { args.TryGetWebMessageAsString(&mut raw) }?;
                // SAFETY: `raw` is a valid, null-terminated wide string owned
                // by the WebView2 event args for the duration of this callback.
                let msg = unsafe { raw.to_string() }.unwrap_or_default();
                dispatch_bridge.dispatch(msg);
                Ok(())
            }));

        let mut token = EventRegistrationToken::default();

        // SAFETY: `add_WebMessageReceived` is safe to call on the UI thread.
        unsafe {
            webview
                .add_WebMessageReceived(&handler, &mut token)
                .map_err(|e| IpcError::SendFailed(e.to_string()))?;
        }

        Ok(token)
    }

    /// Build an [`IpcBridge`] whose sender is backed by `PostWebMessageAsString`
    /// on the given WebView2 instance.
    ///
    /// # Safety
    /// `webview` must outlive the returned bridge, and `send_to_js` must be
    /// called from the UI thread (or marshalled to it).
    pub fn new_bridge(webview: ICoreWebView2) -> IpcBridge {
        // Wrap in SendableWebView so the closure satisfies Send + Sync.
        // Safety: see SendableWebView above.
        let sendable = SendableWebView(webview);
        IpcBridge::new(move |message| {
            let hstring = HSTRING::from(message.as_str());
            sendable
                .post_message(&hstring)
                .map_err(|e| IpcError::SendFailed(e.to_string()))
        })
    }
}
