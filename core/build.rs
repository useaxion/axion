//! Axion build script — issue #26.
//!
//! Embeds Windows executable metadata (app name, version, description, icon)
//! from environment variables set by `axion build`:
//!
//! | Env var                 | axion.config.json field | Default        |
//! |-------------------------|-------------------------|----------------|
//! | `AXION_APP_NAME`        | `name`                  | "Axion App"    |
//! | `AXION_APP_VERSION`     | `version`               | "0.0.0"        |
//! | `AXION_APP_DESCRIPTION` | `description`           | (same as name) |
//! | `AXION_APP_ICON`        | `icon` (path to .ico)   | built-in icon  |
//!
//! No-op on non-Windows targets so cross-compilation stays possible.

fn main() {
    // Rerun when any metadata env var changes.
    println!("cargo:rerun-if-env-changed=AXION_APP_NAME");
    println!("cargo:rerun-if-env-changed=AXION_APP_VERSION");
    println!("cargo:rerun-if-env-changed=AXION_APP_DESCRIPTION");
    println!("cargo:rerun-if-env-changed=AXION_APP_ICON");

    #[cfg(windows)]
    embed_windows_resources();
}

#[cfg(windows)]
fn embed_windows_resources() {
    let name =
        std::env::var("AXION_APP_NAME").unwrap_or_else(|_| "Axion App".to_string());
    let version =
        std::env::var("AXION_APP_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let description =
        std::env::var("AXION_APP_DESCRIPTION").unwrap_or_else(|_| name.clone());
    let icon_env = std::env::var("AXION_APP_ICON").ok();

    let mut res = winres::WindowsResource::new();

    // String resources — visible in Windows Explorer › Properties › Details.
    res.set("FileDescription", &description);
    res.set("ProductName", &name);
    res.set("ProductVersion", &version);
    res.set("FileVersion", &version);

    // Numeric FILEVERSION / PRODUCTVERSION (major.minor.patch.0 packed u64).
    let ver = parse_version(&version);
    res.set_version_info(winres::VersionInfo::FILEVERSION, ver);
    res.set_version_info(winres::VersionInfo::PRODUCTVERSION, ver);

    // Resolve icon: caller path → built-in fallback → none.
    let icon_path = resolve_icon(icon_env.as_deref());
    if let Some(p) = &icon_path {
        res.set_icon(p);
    } else {
        println!(
            "cargo:warning=axion: no icon available — \
             executable will use the default Windows icon"
        );
    }

    if let Err(e) = res.compile() {
        // Non-fatal: exe is still produced without embedded resources when
        // rc.exe / windres is absent (e.g. bare GNU toolchain).
        println!("cargo:warning=axion: winres failed to embed resources: {e}");
    }
}

// ── Version parsing ───────────────────────────────────────────────────────────

/// Parse "major.minor.patch[.build]" → packed u64 for `winres::VersionInfo`.
/// Format: `(major << 48) | (minor << 32) | (patch << 16) | build`.
#[cfg(windows)]
fn parse_version(v: &str) -> u64 {
    let mut parts = v.split('.').filter_map(|p| p.parse::<u64>().ok());
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    let build = parts.next().unwrap_or(0);
    (major << 48) | (minor << 32) | (patch << 16) | build
}

// ── Icon resolution ───────────────────────────────────────────────────────────

/// Returns the path to an `.ico` file to embed:
/// 1. The caller-supplied path if it exists.
/// 2. A built-in 16×16 ICO written to `OUT_DIR`.
/// 3. `None` if neither is available.
#[cfg(windows)]
fn resolve_icon(caller: Option<&str>) -> Option<String> {
    if let Some(p) = caller {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
        println!(
            "cargo:warning=axion: icon \"{p}\" not found — \
             falling back to built-in icon"
        );
    }

    // Write the built-in icon bytes to OUT_DIR so winres can read them.
    let out_dir = std::env::var("OUT_DIR").ok()?;
    let ico_path = std::path::Path::new(&out_dir).join("axion-default.ico");
    match std::fs::write(&ico_path, make_default_ico()) {
        Ok(_) => Some(ico_path.to_string_lossy().into_owned()),
        Err(e) => {
            println!("cargo:warning=axion: could not write built-in icon: {e}");
            None
        }
    }
}

// ── Built-in icon ─────────────────────────────────────────────────────────────

/// Build a minimal valid 16×16 32-bpp ICO (solid Axion blue #0066FF).
///
/// Structure:
/// ```text
/// ICONDIR          6 B
/// ICONDIRENTRY    16 B
/// BITMAPINFOHEADER 40 B  ─┐
/// XOR mask       1024 B   │ resource = 1128 B
/// AND mask         64 B  ─┘
/// Total          1150 B
/// ```
/// AND-mask rows: 16 pixels / 8 bits = 2 bytes, DWORD-padded to 4 bytes,
/// × 16 rows = 64 bytes. All zeros → every pixel is fully visible.
#[cfg(windows)]
fn make_default_ico() -> Vec<u8> {
    // Pixel colour: BGRA for #0066FF fully opaque.
    const PIXEL: [u8; 4] = [0xFF, 0x66, 0x00, 0xFF];

    const RES_SIZE: u32 = 40 + 1024 + 64; // 1128
    const IMAGE_OFFSET: u32 = 22; // 6 ICONDIR + 16 ICONDIRENTRY

    let mut buf: Vec<u8> = Vec::with_capacity(1150);

    // ── ICONDIR (6 B) ─────────────────────────────────────────────────────
    buf.extend_from_slice(&[0x00, 0x00]); // reserved
    buf.extend_from_slice(&[0x01, 0x00]); // type = ICO
    buf.extend_from_slice(&[0x01, 0x00]); // count = 1

    // ── ICONDIRENTRY (16 B) ───────────────────────────────────────────────
    buf.push(16); // width
    buf.push(16); // height
    buf.push(0);  // colorCount (0 = true-color)
    buf.push(0);  // reserved
    buf.extend_from_slice(&[0x01, 0x00]); // planes = 1
    buf.extend_from_slice(&[0x20, 0x00]); // bitCount = 32
    buf.extend_from_slice(&RES_SIZE.to_le_bytes());    // bytesInRes
    buf.extend_from_slice(&IMAGE_OFFSET.to_le_bytes()); // imageOffset

    // ── BITMAPINFOHEADER (40 B) ───────────────────────────────────────────
    buf.extend_from_slice(&40u32.to_le_bytes());  // biSize
    buf.extend_from_slice(&16i32.to_le_bytes());  // biWidth
    buf.extend_from_slice(&32i32.to_le_bytes());  // biHeight (2 × 16; ICO convention)
    buf.extend_from_slice(&1u16.to_le_bytes());   // biPlanes
    buf.extend_from_slice(&32u16.to_le_bytes());  // biBitCount
    buf.extend_from_slice(&0u32.to_le_bytes());   // biCompression (BI_RGB)
    buf.extend_from_slice(&0u32.to_le_bytes());   // biSizeImage
    buf.extend_from_slice(&0i32.to_le_bytes());   // biXPelsPerMeter
    buf.extend_from_slice(&0i32.to_le_bytes());   // biYPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes());   // biClrUsed
    buf.extend_from_slice(&0u32.to_le_bytes());   // biClrImportant

    // ── XOR mask: 16×16 BGRA pixels (bottom-to-top row order) ────────────
    for _ in 0..256 {
        buf.extend_from_slice(&PIXEL);
    }

    // ── AND mask: 16 rows × 4 bytes (all 0 = pixel fully visible) ─────────
    for _ in 0..16 {
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    }

    buf
}
