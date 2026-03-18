//! Frontend asset embedding — issue #25.
//!
//! In **production** builds, the Vite bundle is embedded directly into the
//! Rust binary via `rust-embed`. The WebView2 runtime serves assets from
//! memory using the custom `axion://` URI scheme — no separate asset folder
//! is required on the end-user's machine.
//!
//! In **dev** mode (`AXION_DEV` env var set, or `debug_assertions` enabled),
//! assets are not served from the binary. The runtime navigates WebView2 to
//! the Vite dev server URL (`AXION_DEV_URL`, default `http://localhost:5173`)
//! so Hot Module Replacement works normally.
//!
//! ## Build flow
//!
//! ```text
//! axion build
//!   1. vite build  →  dist/  (project root)
//!   2. copy dist/  →  core/frontend-dist/  (consumed by rust-embed)
//!   3. cargo build --release  →  binary with assets baked in
//! ```

use rust_embed::RustEmbed;

/// Embedded frontend assets (Vite production bundle).
///
/// `rust-embed` embeds the contents of `core/frontend-dist/` into the binary
/// at compile time in release builds. In debug builds (without the
/// `debug-embed` feature) the files are read from disk at runtime so the
/// folder can be empty during development.
#[derive(RustEmbed)]
#[folder = "frontend-dist/"]
pub struct FrontendAssets;

/// A resolved asset ready to send to the browser.
pub struct Asset {
    /// Raw file bytes.
    pub data: Vec<u8>,
    /// MIME type string, e.g. `"text/html; charset=utf-8"`.
    pub mime: String,
}

/// Runtime mode: embedded assets vs. live Vite dev server.
#[derive(Debug, Clone)]
pub enum RuntimeMode {
    /// Serve assets from the embedded `FrontendAssets`.
    Production,
    /// Navigate WebView2 to a live Vite dev server.
    Dev {
        /// Full URL of the Vite dev server, e.g. `http://localhost:5173`.
        url: String,
    },
}

impl RuntimeMode {
    /// Detect the runtime mode from environment variables.
    ///
    /// - If `AXION_DEV` is set, return `Dev` with the URL from `AXION_DEV_URL`
    ///   (defaults to `http://localhost:5173`).
    /// - In debug builds, default to dev mode even without `AXION_DEV`.
    /// - Otherwise return `Production`.
    pub fn detect() -> Self {
        let explicit_dev = std::env::var("AXION_DEV").is_ok();

        #[cfg(debug_assertions)]
        let is_dev = true;
        #[cfg(not(debug_assertions))]
        let is_dev = explicit_dev;

        if is_dev {
            let url = std::env::var("AXION_DEV_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string());
            RuntimeMode::Dev { url }
        } else {
            RuntimeMode::Production
        }
    }

    /// Returns the dev server URL if this is `Dev` mode, `None` otherwise.
    pub fn dev_url(&self) -> Option<&str> {
        match self {
            RuntimeMode::Dev { url } => Some(url.as_str()),
            RuntimeMode::Production => None,
        }
    }

    /// `true` if this is production mode (assets served from binary).
    pub fn is_production(&self) -> bool {
        matches!(self, RuntimeMode::Production)
    }
}

/// Look up an embedded asset by its URL path.
///
/// `path` should be the portion of the URL after the scheme+host, e.g.
/// `/` or `/assets/index-abc123.js`. Normalises to `index.html` for the root.
///
/// Returns `None` if the asset is not embedded (i.e. dev mode, or the path
/// does not exist in the bundle).
pub fn get(path: &str) -> Option<Asset> {
    let normalised = normalise_path(path);
    let file = FrontendAssets::get(normalised)?;
    let mime = mime_for(normalised);
    Some(Asset {
        data: file.data.into_owned(),
        mime,
    })
}

/// Return the list of all embedded asset paths (relative, no leading `/`).
pub fn all_paths() -> impl Iterator<Item = std::borrow::Cow<'static, str>> {
    FrontendAssets::iter()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Normalise a URL path to a key understood by `FrontendAssets::get`.
///
/// - `/` or `` → `index.html`
/// - `/some/path` → `some/path`
fn normalise_path(path: &str) -> &str {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    }
}

/// Infer a MIME type from the file path extension.
fn mime_for(path: &str) -> String {
    // rust-embed's mime-guess feature exposes mime_guess via the embedded file,
    // but we can also do a fast lookup ourselves for the common web asset types.
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_root_to_index_html() {
        assert_eq!(normalise_path("/"), "index.html");
        assert_eq!(normalise_path(""), "index.html");
    }

    #[test]
    fn normalise_strips_leading_slash() {
        assert_eq!(normalise_path("/assets/app.js"), "assets/app.js");
        assert_eq!(normalise_path("/styles/main.css"), "styles/main.css");
    }

    #[test]
    fn mime_for_common_extensions() {
        assert_eq!(mime_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("app.js"), "application/javascript");
        assert_eq!(mime_for("styles.css"), "text/css");
        assert_eq!(mime_for("logo.svg"), "image/svg+xml");
        assert_eq!(mime_for("logo.png"), "image/png");
        assert_eq!(mime_for("font.woff2"), "font/woff2");
        assert_eq!(mime_for("data.json"), "application/json");
        assert_eq!(mime_for("binary.bin"), "application/octet-stream");
    }

    #[test]
    fn runtime_mode_detect_dev_in_debug_build() {
        // In debug builds (tests always run in debug), detect() should return Dev.
        let mode = RuntimeMode::detect();
        assert!(
            matches!(mode, RuntimeMode::Dev { .. }),
            "expected Dev mode in debug build"
        );
    }

    #[test]
    fn runtime_mode_dev_url_default() {
        // dev_url() returns whatever URL was stored at construction — no env
        // var manipulation needed and no risk of races with parallel tests.
        let mode = RuntimeMode::Dev {
            url: "http://localhost:5173".to_string(),
        };
        assert_eq!(mode.dev_url(), Some("http://localhost:5173"));
    }

    #[test]
    fn runtime_mode_custom_dev_url() {
        let mode = RuntimeMode::Dev {
            url: "http://localhost:3000".to_string(),
        };
        assert_eq!(mode.dev_url(), Some("http://localhost:3000"));
    }

    #[test]
    fn runtime_mode_production_dev_url_is_none() {
        let mode = RuntimeMode::Production;
        assert!(mode.dev_url().is_none());
        assert!(mode.is_production());
    }

    #[test]
    fn get_missing_asset_returns_none() {
        // No real assets in frontend-dist/ during tests — just confirm it
        // doesn't panic and returns None.
        assert!(get("/nonexistent.js").is_none());
    }
}
