//! Custom `axion://` URI scheme handler for WebView2 — issue #25.
//!
//! In production mode, WebView2 navigates to `axion://app/` and all asset
//! requests for `axion://app/*` are intercepted here. The handler looks up
//! the path in [`crate::assets::FrontendAssets`] and returns the embedded
//! bytes as a synthetic HTTP response via the WebView2 Web Resource API.
//!
//! In dev mode the WebView2 navigates to the Vite dev server URL directly;
//! no scheme handler is registered.
//!
//! ## URL mapping
//!
//! ```text
//! axion://app/              → FrontendAssets::get("index.html")
//! axion://app/assets/app.js → FrontendAssets::get("assets/app.js")
//! ```

#[cfg(windows)]
pub mod webview2 {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2Environment,
        ICoreWebView2WebResourceRequestedEventArgs, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    };
    use webview2_com::WebResourceRequestedEventHandler;
    use windows::{
        core::HSTRING,
        Win32::System::WinRT::EventRegistrationToken,
    };

    use crate::assets;
    use crate::ipc::error::IpcError;

    /// The custom scheme host used for all production asset requests.
    pub const SCHEME_URL: &str = "axion://app/";
    /// Filter pattern registered with WebView2.
    const SCHEME_FILTER: &str = "axion://app/*";

    /// Register the `axion://` Web Resource handler on `webview`.
    ///
    /// After this call, any navigation to `axion://app/*` will be served
    /// from the embedded [`crate::assets::FrontendAssets`].
    ///
    /// Returns the event registration token so the caller can deregister
    /// the handler during shutdown.
    ///
    /// # Safety
    /// Must be called from the UI thread. `env` must be the same
    /// `ICoreWebView2Environment` that created `webview`.
    pub fn register(
        webview: &ICoreWebView2,
        env: &ICoreWebView2Environment,
    ) -> Result<EventRegistrationToken, IpcError> {
        let env_clone = env.clone();

        let handler = WebResourceRequestedEventHandler::create(Box::new(
            move |_sender, args| {
                handle_request(&env_clone, args)?;
                Ok(())
            },
        ));

        // Register the filter so WebView2 calls our handler for axion:// URLs.
        unsafe {
            webview
                .AddWebResourceRequestedFilter(
                    &HSTRING::from(SCHEME_FILTER),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )
                .map_err(|e| IpcError::SendFailed(e.to_string()))?;
        }

        let mut token = EventRegistrationToken::default();
        unsafe {
            webview
                .add_WebResourceRequested(&handler, &mut token)
                .map_err(|e| IpcError::SendFailed(e.to_string()))?;
        }

        Ok(token)
    }

    /// Handle a single `axion://` web resource request.
    fn handle_request(
        env: &ICoreWebView2Environment,
        args: Option<ICoreWebView2WebResourceRequestedEventArgs>,
    ) -> windows::core::Result<()> {
        let args = args.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_POINTER))?;

        // Extract the URL from the request.
        let request = unsafe { args.Request() }?;
        let mut url_pwstr = windows::core::PWSTR::null();
        unsafe { request.Uri(&mut url_pwstr) }?;
        let url = unsafe { url_pwstr.to_string() }.unwrap_or_default();

        // Strip scheme + host to get the asset path.
        let path = url
            .strip_prefix("axion://app")
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/");

        // Look up the asset.
        let (body_bytes, mime) = match assets::get(path) {
            Some(asset) => (asset.data, asset.mime),
            None => {
                // 404 — asset not found in bundle.
                let body_404 = b"404 Not Found".to_vec();
                set_response(env, &args, 404, body_404, "text/plain")?;
                return Ok(());
            }
        };

        set_response(env, &args, 200, body_bytes, &mime)?;
        Ok(())
    }

    /// Build a synthetic WebView2 response and set it on the event args.
    fn set_response(
        env: &ICoreWebView2Environment,
        args: &ICoreWebView2WebResourceRequestedEventArgs,
        status: u32,
        body: Vec<u8>,
        mime: &str,
    ) -> windows::core::Result<()> {
        // Create an IStream backed by the body bytes.
        let stream = bytes_to_stream(&body)?;

        let headers = format!("Content-Type: {mime}\r\nAccess-Control-Allow-Origin: *");
        let status_str = status.to_string();

        let response = unsafe {
            env.CreateWebResourceResponse(
                &stream,
                status as i32,
                &HSTRING::from(status_str.as_str()),
                &HSTRING::from(headers.as_str()),
            )
        }?;

        unsafe { args.SetResponse(&response) }?;
        Ok(())
    }

    /// Wrap a `Vec<u8>` in a COM `IStream` for passing to WebView2.
    fn bytes_to_stream(data: &[u8]) -> windows::core::Result<windows::Win32::System::Com::IStream> {
        use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

        unsafe {
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, data.len()).map_err(|_| {
                windows::core::Error::from(windows::Win32::Foundation::E_OUTOFMEMORY)
            })?;
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return Err(windows::core::Error::from(windows::Win32::Foundation::E_POINTER));
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
            GlobalUnlock(hglobal).ok();
            CreateStreamOnHGlobal(hglobal, true)
        }
    }

    /// Returns the URL the WebView2 should navigate to in production mode.
    pub fn production_start_url() -> &'static str {
        SCHEME_URL
    }
}
