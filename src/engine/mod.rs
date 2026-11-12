#![allow(clippy::too_many_arguments)] // CEF APIs fix callback signatures and browser option lists.

//! Modern windowless CEF boundary.
//! Uses CEF Off-Screen Rendering (OSR) directly into GPUI RenderImages.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::ffi::c_ulong;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use cef::args::Args;
use cef::*;
use gpui_kit::{KeyDownEvent, KeyUpEvent, RenderImage};

use crate::lifecycle::TabLifecycle;
use crate::model::TabState;

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadRecord {
    pub id: u32,
    pub url: String,
    pub path: PathBuf,
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub complete: bool,
    pub canceled: bool,
}

/// A window the page asked for (link click, `window.open`, OAuth popup).
pub struct PopupRequest {
    pub space_id: String,
    pub url: String,
    /// Set when CEF created the window itself. Those browsers keep a live
    /// `window.opener`, `postMessage` and `window.close()`; the app only has to
    /// put the existing browser on screen.
    pub browser_id: Option<String>,
}

#[derive(Default)]
struct EngineEvents {
    popups: Vec<PopupRequest>,
    /// Windowless browsers CEF is about to create for the page itself. Their
    /// shared state has to land in `pending_browsers` before `on_after_created`
    /// delivers the browser.
    pending_popup_states: Vec<(String, Arc<Mutex<BrowserSharedState>>)>,
    fullscreen: Vec<(String, bool)>,
    downloads: Vec<DownloadRecord>,
    created: Vec<(String, Browser)>,
    plugin_events: Vec<(String, String)>,
    close_requests: Vec<String>,
}

fn origin_favicon_url(page_url: &str) -> Option<String> {
    let (scheme, remainder) = page_url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = remainder.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}/favicon.ico"))
}

pub fn resolve_favicon_url(page_url: &str, raw_url: &str) -> Option<String> {
    let raw = raw_url.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:image/") {
        return Some(raw.to_owned());
    }
    if let Some(rest) = raw.strip_prefix("//") {
        let scheme = if page_url.starts_with("http://") {
            "http"
        } else {
            "https"
        };
        return Some(format!("{scheme}://{rest}"));
    }
    let (scheme, remainder) = page_url.split_once("://")?;
    let (authority, path) = match remainder.split_once('/') {
        Some((auth, p)) => (auth, format!("/{p}")),
        None => (remainder.split(['?', '#']).next()?, "/".to_string()),
    };
    if authority.is_empty() {
        return None;
    }
    let origin = format!("{scheme}://{authority}");
    if raw.starts_with('/') {
        Some(format!("{origin}{raw}"))
    } else {
        let clean_path = path.split('?').next().unwrap_or(&path);
        if let Some(idx) = clean_path.rfind('/') {
            let dir = &clean_path[..=idx];
            Some(format!("{origin}{dir}{raw}"))
        } else {
            Some(format!("{origin}/{raw}"))
        }
    }
}

pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in input.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

pub fn favicons_dir() -> std::path::PathBuf {
    crate::paths::data_dir().join("favicons")
}

pub fn save_favicon_to_disk(host: &str, image: &image::RgbaImage) {
    let dir = favicons_dir();
    let _ = std::fs::create_dir_all(&dir);
    let clean_host = host.trim().to_ascii_lowercase();
    if clean_host.is_empty() {
        return;
    }
    let path = dir.join(format!("{clean_host}.png"));
    let _ = image.save_with_format(path, image::ImageFormat::Png);

    // Also save alternate host (with/without www.) so lookups on either always hit
    if let Some(without_www) = clean_host.strip_prefix("www.") {
        if !without_www.is_empty() {
            let alt_path = dir.join(format!("{without_www}.png"));
            let _ = image.save_with_format(alt_path, image::ImageFormat::Png);
        }
    } else {
        let with_www = format!("www.{clean_host}");
        let alt_path = dir.join(format!("{with_www}.png"));
        let _ = image.save_with_format(alt_path, image::ImageFormat::Png);
    }
}

/// GPUI uploads `RenderImage` frames as BGRA, while favicons are decoded as
/// RGBA. Swap the red/blue channels so icons keep their true colours.
fn rgba_to_bgra_in_place(buffer: &mut image::RgbaImage) {
    for pixel in buffer.pixels_mut() {
        pixel.0.swap(0, 2);
    }
}

pub fn load_favicon_from_disk(host: &str) -> Option<Arc<RenderImage>> {
    let dir = favicons_dir();
    let clean_host = host.trim().to_ascii_lowercase();
    if clean_host.is_empty() {
        return None;
    }
    let direct_path = dir.join(format!("{clean_host}.png"));
    let final_path = if direct_path.exists() {
        direct_path
    } else if let Some(without_www) = clean_host.strip_prefix("www.") {
        dir.join(format!("{without_www}.png"))
    } else {
        dir.join(format!("www.{clean_host}.png"))
    };

    let decoded = image::open(final_path).ok()?;
    let mut rgba = decoded.to_rgba8();
    rgba_to_bgra_in_place(&mut rgba);
    let frame = image::Frame::new(rgba);
    Some(Arc::new(RenderImage::new(smallvec::smallvec![frame])))
}

pub fn favicon_url_score(url: &str) -> i32 {
    let lower = url.to_ascii_lowercase();
    let mut score = 50;

    if lower.contains("192")
        || lower.contains("180")
        || lower.contains("128")
        || lower.contains("apple-touch")
    {
        score += 40;
    } else if lower.contains("64") || lower.contains("48") || lower.contains("32") {
        score += 30;
    } else if lower.contains("16") {
        score -= 10;
    }

    if lower.contains(".svg") {
        score += 35;
    } else if lower.contains(".png") {
        score += 30;
    } else if lower.contains(".webp") {
        score += 25;
    } else if lower.ends_with(".ico") || lower.contains(".ico?") {
        score -= 20;
    }

    if lower.contains("branding") || lower.contains("searchlogo") || lower.contains("logo") {
        score += 15;
    }

    score
}

/// Hosts are considered the same favicon target when they differ only by a
/// leading `www.` (favicons are stored under both spellings on disk).
pub fn same_favicon_host(left: &str, right: &str) -> bool {
    let left = left.trim().to_ascii_lowercase();
    let right = right.trim().to_ascii_lowercase();
    left == right || left.trim_start_matches("www.") == right.trim_start_matches("www.")
}

/// A network result is fresher than a cached icon, so it may replace it. A
/// page-declared favicon still outranks the origin `/favicon.ico` fallback.
pub fn should_store_favicon(
    matches_current: bool,
    incoming_preferred: bool,
    existing_preferred: bool,
    host_mismatch: bool,
) -> bool {
    matches_current && (incoming_preferred || !existing_preferred || host_mismatch)
}

fn favicon_debug(message: std::fmt::Arguments<'_>) {
    if std::env::var_os("ANARC_DEBUG_FAVICON").is_some_and(|value| value == "1") {
        eprintln!("Lumen favicon: {message}");
    }
}

pub fn log_favicon_row_rendered(tab_id: &str, has_image: bool) {
    favicon_debug(format_args!("row rendered tab={tab_id} image={has_image}"));
}

pub fn detect_system_refresh_rate() -> Option<i32> {
    if let Ok(val) = std::env::var("ANARC_FPS")
        && let Ok(fps) = val.parse::<i32>()
        && (30..=360).contains(&fps)
    {
        return Some(fps);
    }
    #[cfg(target_os = "linux")]
    {
        // Try hyprctl monitors if on Hyprland
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
            && let Ok(output) = std::process::Command::new("hyprctl")
                .arg("monitors")
                .output()
            && let Ok(text) = std::str::from_utf8(&output.stdout)
        {
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(at_idx) = trimmed.find('@') {
                    let after_at = &trimmed[at_idx + 1..];
                    let num_str: String = after_at
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    if let Ok(hz) = num_str.parse::<f64>()
                        && (30.0..=360.0).contains(&hz)
                    {
                        return Some(hz.round() as i32);
                    }
                }
            }
        }
        // Fallback: try xrandr
        if let Ok(output) = std::process::Command::new("xrandr").output()
            && let Ok(text) = std::str::from_utf8(&output.stdout)
        {
            for line in text.lines() {
                if line.contains('*') {
                    for part in line.split_whitespace() {
                        if part.contains('*') {
                            let clean: String = part
                                .chars()
                                .filter(|c| c.is_ascii_digit() || *c == '.')
                                .collect();
                            if let Ok(hz) = clean.parse::<f64>()
                                && (30.0..=360.0).contains(&hz)
                            {
                                return Some(hz.round() as i32);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Every OSR frame is a full-window `Vec` copy plus a fresh GPU texture upload.
/// Riding a 144 Hz panel burns ~1.2 GB/s of memory bandwidth and blocks CEF's
/// compositor thread on the copy, which is what made heavy pages (chat apps,
/// long documents) feel sluggish. Web content is paced well below this anyway,
/// so cap the OSR surface at 60 fps; `ANARC_OSR_FPS` overrides it for benchmarks.
const OSR_MAX_FRAME_RATE: i32 = 60;

fn osr_frame_rate_cap() -> i32 {
    std::env::var("ANARC_OSR_FPS")
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(OSR_MAX_FRAME_RATE)
        .clamp(1, 240)
}

pub fn frame_rate_for_refresh(refresh_interval: Option<std::time::Duration>) -> i32 {
    let rate = refresh_interval
        .filter(|interval| !interval.is_zero())
        .map(|interval| (1.0 / interval.as_secs_f64()).round() as i32)
        .or({
            #[cfg(test)]
            {
                None
            }
            #[cfg(not(test))]
            {
                detect_system_refresh_rate()
            }
        })
        .unwrap_or(60);
    rate.clamp(1, osr_frame_rate_cap())
}

/// Identity providers and OAuth endpoints a sign-in popup points at.
const AUTH_POPUP_HOSTS: &[&str] = &[
    "accounts.google.com",
    "accounts.youtube.com",
    "appleid.apple.com",
    "login.microsoftonline.com",
    "login.live.com",
    "login.microsoft.com",
    "login.yahoo.com",
    "id.atlassian.com",
    "auth0.com",
    "okta.com",
];

/// OAuth endpoints that stay on a general-purpose host.
const AUTH_POPUP_PATHS: &[(&str, &str)] = &[
    ("github.com", "/login/oauth"),
    ("gitlab.com", "/oauth"),
    ("bitbucket.org", "/site/oauth2"),
    ("discord.com", "/oauth2/authorize"),
    ("slack.com", "/oauth"),
];

/// True when a popup URL belongs to a sign-in flow. Those windows must keep
/// their `window.opener` (identity providers hand the result back through
/// `postMessage` and shut the window with `window.close()`), so they are the
/// ones CEF is allowed to create itself.
pub fn is_auth_flow_url(url: &str) -> bool {
    let Some(host) = crate::shield::url_host(url) else {
        return false;
    };
    if AUTH_POPUP_HOSTS
        .iter()
        .any(|known| host == *known || host.ends_with(&format!(".{known}")))
    {
        return true;
    }
    // OAuth providers that identify the flow by path rather than by
    // `response_type`, so an invite or repo link on the same host stays a peek.
    if AUTH_POPUP_PATHS
        .iter()
        .any(|(known_host, prefix)| host == *known_host && url_after_host(url).starts_with(prefix))
    {
        return true;
    }
    let lower = url.to_ascii_lowercase();
    lower.contains("response_type=")
        && (lower.contains("client_id=") || lower.contains("redirect_uri="))
}

fn url_after_host(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, rest)| rest.find('/').map(|index| &rest[index..]))
        .unwrap_or("/")
}

cef::wrap_app! {
    struct LumenCefApp;

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(registrar) = registrar {
                panel_scheme::register_panel_scheme(registrar);
            }
        }

        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(command_line) = command_line {
                #[cfg(target_os = "linux")]
                {
                    let name = CefString::from("ozone-platform");
                    let value = CefString::from("x11");
                    command_line.append_switch_with_value(Some(&name), Some(&value));
                }
                let name = CefString::from("renderer-process-limit");
                let value = CefString::from("4");
                command_line.append_switch_with_value(Some(&name), Some(&value));
                let name = CefString::from("user-agent");
                let value = CefString::from(
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36",
                );
                command_line.append_switch_with_value(Some(&name), Some(&value));
                for name in [
                    "disable-background-networking",
                    "disable-component-update",
                    "disable-sync",
                    "disable-breakpad",
                    "no-first-run",
                    "enable-smooth-scrolling",
                    "enable-gpu-rasterization",
                    "ignore-gpu-blocklist",
                    "enable-zero-copy",
                    // Lumen's chrome is dark, so web content should request the
                    // dark color scheme by default (prefers-color-scheme: dark).
                    "force-dark-mode",
                    "disable-chrome-runtime",
                ] {
                    let name = CefString::from(name);
                    command_line.append_switch(Some(&name));
                }
                let current_exe = std::env::current_exe()
                    .unwrap_or_else(|_| std::path::PathBuf::from("anarc"));
                let subprocess_switch = CefString::from("browser-subprocess-path");
                let subprocess_path = CefString::from(current_exe.to_string_lossy().as_ref());
                command_line.append_switch_with_value(Some(&subprocess_switch), Some(&subprocess_path));
                let name = CefString::from("autoplay-policy");
                let value = CefString::from("user-gesture-required");
                command_line.append_switch_with_value(Some(&name), Some(&value));
                let name = CefString::from("enable-features");
                let value = CefString::from(
                    "OverlayScrollbar,OverlayScrollbars,CanvasOopRasterization,VaapiIgnoreDriverChecks",
                );
                command_line.append_switch_with_value(Some(&name), Some(&value));
                let name = CefString::from("disable-features");
                let value = CefString::from(
                    "Translate,MediaRouter,OptimizationHints,AutofillServerCommunication,InterestFeedContentSuggestions",
                );
                command_line.append_switch_with_value(
                    Some(&name),
                    Some(&value),
                );
                let name = CefString::from("disk-cache-size");
                let value = CefString::from("104857600");
                command_line.append_switch_with_value(Some(&name), Some(&value));
            }
        }
    }
}

#[derive(Default)]
pub struct BrowserSharedState {
    pub width: i32,
    pub height: i32,
    pub image: Option<Arc<RenderImage>>,
    pub edge_color: Option<u32>,
    pub last_edge_color_instant: Option<std::time::Instant>,
    pub dirty: bool,
    pub root_dirty: bool,
    pub title: Option<String>,
    pub url: Option<String>,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub favicon: Option<Arc<RenderImage>>,
    pub favicon_host: Option<String>,
    /// True when the current favicon came from a page-declared icon rather than
    /// the origin `/favicon.ico` fallback, so the fallback cannot downgrade it.
    pub favicon_preferred: bool,
    pub current_url: Option<String>,
    pub current_host: Option<String>,
    /// Superseded frames that must be evicted from GPUI's sprite atlas on the UI thread.
    pub retired: Vec<Arc<RenderImage>>,
    pub screen_width: i32,
    pub screen_height: i32,
    pub screen_scale: f32,
    pub screen_origin_x: i32,
    pub screen_origin_y: i32,
    pub cursor: i32,
    pub target_zoom_level: Option<f64>,
    /// Blink's visual-viewport page scale, driven by trackpad pinch. Unlike
    /// `target_zoom_level` this magnifies the rendered page without reflowing
    /// the layout, and it resets to 1 on every navigation.
    pub page_scale: f64,
}

/// Fresh shared state for a browser CEF is about to create. Tabs pass their
/// own size and zoom level; windows the page opened itself (OAuth popups) keep
/// the default zoom.
pub(super) fn new_shared_state(
    width: i32,
    height: i32,
    url: &str,
    zoom_level: f64,
) -> BrowserSharedState {
    let initial_host = crate::shield::url_host(url);
    let initial_favicon = initial_host.as_deref().and_then(load_favicon_from_disk);
    BrowserSharedState {
        width: width.max(1),
        height: height.max(1),
        image: None,
        edge_color: None,
        last_edge_color_instant: None,
        dirty: false,
        root_dirty: false,
        title: None,
        url: None,
        is_loading: true,
        can_go_back: false,
        can_go_forward: false,
        favicon: initial_favicon,
        favicon_host: initial_host.clone(),
        favicon_preferred: false,
        current_url: Some(url.to_string()),
        current_host: initial_host,
        retired: Vec::new(),
        screen_width: width.max(1),
        screen_height: height.max(1),
        screen_scale: 1.0,
        screen_origin_x: 0,
        screen_origin_y: 0,
        cursor: 0,
        target_zoom_level: Some(zoom_level),
        page_scale: 1.0,
    }
}

fn median_edge_color_bgra(pixels: &[u8], width: usize, height: usize) -> Option<u32> {
    if width == 0 || height == 0 || pixels.len() < width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let step = (height / 128).max(1);
    let sample_count = height.div_ceil(step);
    let mut red = Vec::with_capacity(sample_count);
    let mut green = Vec::with_capacity(sample_count);
    let mut blue = Vec::with_capacity(sample_count);
    let x = width - 1;
    for y in (0..height).step_by(step) {
        let offset = (y * width + x) * 4;
        blue.push(pixels[offset]);
        green.push(pixels[offset + 1]);
        red.push(pixels[offset + 2]);
    }
    let median = |values: &mut [u8]| {
        let middle = values.len() / 2;
        values.select_nth_unstable(middle);
        values[middle]
    };
    Some(
        (u32::from(median(&mut red)) << 16)
            | (u32::from(median(&mut green)) << 8)
            | u32::from(median(&mut blue)),
    )
}

mod callbacks;
use callbacks::{
    HttpsFirstState, LumenClient, LumenContextMenuHandler, LumenDialogHandler, LumenDisplayHandler,
    LumenDownloadHandler, LumenLifeSpanHandler, LumenLoadHandler, LumenPermissionHandler,
    LumenRenderHandler, LumenRequestHandler, remember_https_upgrade,
};

pub struct BrowserEntry {
    pub browser: Browser,
    pub state: Arc<Mutex<BrowserSharedState>>,
}

pub struct Engine {
    browsers: HashMap<String, BrowserEntry>,
    resize_state: HashMap<String, (Instant, i32, i32)>,
    pending_browsers: HashMap<String, Arc<Mutex<BrowserSharedState>>>,
    canceled_browsers: HashSet<String>,
    contexts: HashMap<String, RequestContext>,
    initialized: bool,
    args: Args,
    app: Option<cef::App>,
    visible: Vec<String>,
    lifecycle: TabLifecycle,
    events: Arc<Mutex<EngineEvents>>,
    retired_images: Vec<Arc<RenderImage>>,
    ui_update_wake: async_channel::Sender<()>,
    ui_update_receiver: async_channel::Receiver<()>,
    windowless_frame_rate: i32,
    close_state: Arc<(Mutex<usize>, Condvar)>,
    shutdown_requested: Arc<std::sync::atomic::AtomicBool>,
    shield: Arc<crate::shield::ShieldState>,
    https_state: Arc<Mutex<HttpsFirstState>>,
    panel_packages: panel_scheme::PanelRegistry,
    /// Names the browsers CEF creates for page popups so their ids stay unique
    /// without the app having to know about them ahead of time.
    popup_serial: Arc<std::sync::atomic::AtomicU64>,
}

fn resize_should_send(
    previous: Option<&(Instant, i32, i32)>,
    requested: (i32, i32),
    now: Instant,
    force: bool,
) -> bool {
    previous.is_none_or(|(last_sent, width, height)| {
        (*width != requested.0 || *height != requested.1)
            // Resize at least twice as often as the 60 fps paint cadence so a
            // moving split divider cannot leave the panes on stale dimensions
            // for more than one OSR frame.
            && (force || now.duration_since(*last_sent) >= Duration::from_millis(8))
    })
}

fn resize_debug(message: std::fmt::Arguments<'_>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var("ANARC_DEBUG_RESIZE").as_deref() == Ok("1")) {
        eprintln!("[resize-debug] {message}");
    }
}

mod browser;
mod cef_runtime;
pub mod clipboard;
mod navigation;
mod panel_scheme;

pub fn windows_key_code_for_key(key: &str) -> i32 {
    match key.to_ascii_lowercase().as_str() {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0d,
        "escape" | "esc" => 0x1b,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" | "arrowleft" => 0x25,
        "up" | "arrowup" => 0x26,
        "right" | "arrowright" => 0x27,
        "down" | "arrowdown" => 0x28,
        "insert" => 0x2d,
        "delete" => 0x2e,
        // Punctuation and symbols - Windows OEM Virtual Key Codes
        ";" | ":" => 0xba,
        "=" | "+" => 0xbb,
        "," | "<" => 0xbc,
        "-" | "_" => 0xbd,
        "." | ">" => 0xbe,
        "/" | "?" => 0xbf,
        "`" | "~" => 0xc0,
        "[" | "{" => 0xdb,
        "\\" | "|" => 0xdc,
        "]" | "}" => 0xdd,
        "'" | "\"" => 0xde,
        // Function keys
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7a,
        "f12" => 0x7b,
        value if value.len() == 1 => {
            let b = value.as_bytes()[0];
            if b.is_ascii_alphanumeric() {
                b.to_ascii_uppercase() as i32
            } else {
                b as i32
            }
        }
        _ => 0,
    }
}

fn cef_mouse_modifiers(modifiers: &gpui_kit::Modifiers) -> u32 {
    (if modifiers.shift { 2 } else { 0 })
        | (if modifiers.control { 4 } else { 0 })
        | (if modifiers.alt { 8 } else { 0 })
        | (if modifiers.platform { 128 } else { 0 })
}

/// CEF/Chromium `cef_event_flags_t` bits for the buttons currently held down.
/// Click events get this for free (CEF derives it from the button type), but
/// move events only carry what the embedder puts in `MouseEvent::modifiers`.
/// Without it Blink sees `Button::kNoButton` on every move, so drag-select
/// (and every other drag gesture in page content) never starts.
fn cef_button_flag(button: gpui_kit::MouseButton) -> u32 {
    match button {
        gpui_kit::MouseButton::Left => 1,    // EVENTFLAG_LEFT_MOUSE_BUTTON
        gpui_kit::MouseButton::Middle => 16, // EVENTFLAG_MIDDLE_MOUSE_BUTTON
        gpui_kit::MouseButton::Right => 32,  // EVENTFLAG_RIGHT_MOUSE_BUTTON
        gpui_kit::MouseButton::Navigate(_) => 0,
    }
}

fn cef_mouse_flags(
    modifiers: &gpui_kit::Modifiers,
    pressed_button: Option<gpui_kit::MouseButton>,
) -> u32 {
    cef_mouse_modifiers(modifiers) | pressed_button.map_or(0, cef_button_flag)
}

fn cef_key_event(
    key: &str,
    modifiers: &gpui_kit::Modifiers,
    event_type: KeyEventType,
    character: u16,
    focus_on_editable_field: bool,
) -> KeyEvent {
    let mut flags = 0;
    if modifiers.shift {
        flags |= 1 << 1;
    }
    if modifiers.control {
        flags |= 1 << 2;
    }
    if modifiers.alt {
        flags |= 1 << 3;
    }
    if modifiers.platform {
        flags |= 1 << 7;
    }
    let code = windows_key_code_for_key(key);
    KeyEvent {
        size: std::mem::size_of::<KeyEvent>(),
        type_: event_type,
        modifiers: flags,
        windows_key_code: code,
        native_key_code: native_key_code(key, code),
        is_system_key: 0,
        character,
        unmodified_character: character,
        focus_on_editable_field: focus_on_editable_field as i32,
    }
}

fn native_key_code(key: &str, fallback: i32) -> i32 {
    #[cfg(target_os = "linux")]
    {
        let _ = fallback;
        static KEY_CODES: std::sync::OnceLock<Mutex<HashMap<String, i32>>> =
            std::sync::OnceLock::new();
        let key_name = key.to_ascii_lowercase();
        let cache = KEY_CODES.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(codes) = cache.lock()
            && let Some(code) = codes.get(&key_name)
        {
            return *code;
        }

        let display = unsafe { x11::xlib::XOpenDisplay(std::ptr::null()) };
        if display.is_null() {
            return 0;
        }
        let keysym = match key_name.as_str() {
            "backspace" => x11::keysym::XK_BackSpace,
            "tab" => x11::keysym::XK_Tab,
            "enter" | "return" => x11::keysym::XK_Return,
            "escape" | "esc" => x11::keysym::XK_Escape,
            "space" => x11::keysym::XK_space,
            "home" => x11::keysym::XK_Home,
            "left" | "arrowleft" => x11::keysym::XK_Left,
            "up" | "arrowup" => x11::keysym::XK_Up,
            "right" | "arrowright" => x11::keysym::XK_Right,
            "down" | "arrowdown" => x11::keysym::XK_Down,
            "pageup" => x11::keysym::XK_Page_Up,
            "pagedown" => x11::keysym::XK_Page_Down,
            "end" => x11::keysym::XK_End,
            "delete" => x11::keysym::XK_Delete,
            "insert" => x11::keysym::XK_Insert,
            value if value.len() == 1 => value.as_bytes()[0] as u32,
            _ => 0,
        };
        let code = if keysym == 0 {
            0
        } else {
            unsafe { x11::xlib::XKeysymToKeycode(display, keysym as c_ulong) as i32 }
        };
        unsafe { x11::xlib::XCloseDisplay(display) };
        if let Ok(mut codes) = cache.lock() {
            codes.insert(key_name, code);
        }
        code
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = key;
        fallback
    }
}

/// Run CEF's subprocess entry point before GPUI starts, as required for renderer
/// and GPU child processes. `None` means this is the browser process.
pub fn execute_subprocess() -> Option<i32> {
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let mut app = LumenCefApp::new();
    let result = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    (result >= 0).then_some(result)
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown_requested
            .store(true, std::sync::atomic::Ordering::Release);
        for entry in self.browsers.values() {
            if let Some(host) = entry.browser.host() {
                host.close_browser(1);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            for (_, browser) in events.created.drain(..) {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            }
        }
        if self.initialized {
            let (count, wake) = &*self.close_state;
            if let Ok(count) = count.lock() {
                let (count, result) = wake
                    .wait_timeout_while(count, Duration::from_secs(10), |count| *count != 0)
                    .unwrap();
                if result.timed_out() && *count != 0 {
                    eprintln!("Lumen CEF shutdown deferred: {count} browsers still closing");
                    return;
                }
            }
            cef::shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use callbacks::fallback_http_after_upgrade_failure;

    #[test]
    fn mouse_modifiers_map_to_cef_flags() {
        let modifiers = gpui_kit::Modifiers {
            shift: true,
            control: true,
            alt: false,
            platform: true,
            function: false,
        };
        assert_eq!(cef_mouse_modifiers(&modifiers), 2 | 4 | 128);
    }

    #[test]
    fn frame_rate_uses_refresh_with_fallback_and_cap() {
        assert_eq!(frame_rate_for_refresh(None), 60);
        assert_eq!(
            frame_rate_for_refresh(Some(std::time::Duration::from_millis(144))),
            7
        );
        assert_eq!(
            frame_rate_for_refresh(Some(std::time::Duration::from_millis(240))),
            4
        );
        // A 240 Hz panel asks for 240 fps of full-frame OSR copies; the surface
        // is capped well below that unless explicitly overridden.
        assert!(frame_rate_for_refresh(Some(std::time::Duration::from_millis(4))) <= 240);
        assert_eq!(
            frame_rate_for_refresh(Some(std::time::Duration::from_millis(4))),
            osr_frame_rate_cap()
        );
        assert_eq!(
            OSR_MAX_FRAME_RATE, 60,
            "OSR copies are bandwidth bound; raise only with measurements"
        );
    }

    #[test]
    fn auth_flow_detection_covers_identity_providers_and_oauth_authorize_urls() {
        assert!(is_auth_flow_url(
            "https://accounts.google.com/v3/signin/accountchooser?client_id=a&redirect_uri=b"
        ));
        assert!(is_auth_flow_url(
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?response_type=code&client_id=1"
        ));
        assert!(is_auth_flow_url(
            "https://idp.example.com/authorize?response_type=code&redirect_uri=https%3A%2F%2Fapp.example.com"
        ));
        assert!(is_auth_flow_url(
            "https://github.com/login/oauth/authorize?client_id=abc&redirect_uri=https%3A%2F%2Fapp.example.com"
        ));
        assert!(!is_auth_flow_url("https://github.com/rust-lang/rust"));
        assert!(!is_auth_flow_url("https://accounts.example.com/oauth"));
        assert!(!is_auth_flow_url("https://example.com/docs?page=2"));
        assert!(!is_auth_flow_url("about:blank"));
    }

    #[test]
    fn forgetting_a_page_closed_browser_waits_for_pending_creation_instead_of_double_closing() {
        let mut engine = Engine::new();
        let state = Arc::new(Mutex::new(new_shared_state(
            10,
            10,
            "https://example.com",
            1.0,
        )));
        engine
            .pending_browsers
            .insert("personal-popup-1".into(), state);
        engine.forget_closed_browser("personal-popup-1");
        assert!(!engine.pending_browsers.contains_key("personal-popup-1"));
        assert!(engine.canceled_browsers.contains("personal-popup-1"));
        assert!(!engine.has_browser("personal-popup-1"));
    }

    #[test]
    fn mouse_button_flags_match_cef_event_flags() {
        assert_eq!(cef_button_flag(gpui_kit::MouseButton::Left), 1);
        assert_eq!(cef_button_flag(gpui_kit::MouseButton::Middle), 16);
        assert_eq!(cef_button_flag(gpui_kit::MouseButton::Right), 32);
        assert_eq!(
            cef_mouse_flags(
                &gpui_kit::Modifiers::default(),
                Some(gpui_kit::MouseButton::Left)
            ),
            1
        );
        let shift = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            cef_mouse_flags(&shift, Some(gpui_kit::MouseButton::Left)),
            1 | 2
        );
        assert_eq!(cef_mouse_flags(&shift, None), 2);
    }

    #[test]
    fn system_refresh_rate_detection_extracts_display_hz() {
        let detected = detect_system_refresh_rate();
        // On Linux Hyprland / xrandr environments, this detects a valid refresh rate (e.g. 144Hz)
        if let Some(hz) = detected {
            assert!((30..=360).contains(&hz));
        }
    }

    #[test]
    fn origin_favicon_url_uses_only_http_origins() {
        assert_eq!(
            origin_favicon_url("https://www.google.com/search?q=lumen"),
            Some("https://www.google.com/favicon.ico".to_owned())
        );
        assert_eq!(
            origin_favicon_url("http://localhost:8080/page"),
            Some("http://localhost:8080/favicon.ico".to_owned())
        );
        assert_eq!(origin_favicon_url("about:blank"), None);
        assert_eq!(origin_favicon_url("file:///tmp/page.html"), None);
    }

    #[test]
    fn resize_coalescing_tracks_drag_within_one_frame_and_flushes_final_size() {
        let start = Instant::now();
        let sent = (start, 800, 600);
        assert!(!resize_should_send(
            Some(&sent),
            (810, 600),
            start + Duration::from_millis(4),
            false,
        ));
        assert!(resize_should_send(
            Some(&sent),
            (810, 600),
            start + Duration::from_millis(8),
            false,
        ));
        assert!(resize_should_send(
            Some(&sent),
            (810, 600),
            start + Duration::from_millis(4),
            true,
        ));
        assert!(!resize_should_send(
            Some(&sent),
            (800, 600),
            start + Duration::from_millis(8),
            true,
        ));
    }

    #[test]
    fn frame_edge_color_is_channel_median_converted_from_bgra_to_rgb() {
        let pixels = [
            0, 0, 0, 255, 1, 2, 30, 255, 0, 0, 0, 255, 4, 5, 60, 255, 0, 0, 0, 255, 7, 8, 90, 255,
        ];
        assert_eq!(median_edge_color_bgra(&pixels, 2, 3), Some(0x3c0504));
        assert_eq!(median_edge_color_bgra(&pixels[..7], 2, 3), None);
    }

    #[test]
    fn https_first_falls_back_once_per_failed_upgraded_host() {
        let mut state = HttpsFirstState::default();
        remember_https_upgrade(&mut state, "https://example.test/path");
        assert_eq!(
            fallback_http_after_upgrade_failure(&mut state, "https://example.test/path"),
            Some("http://example.test/path".to_owned())
        );
        assert!(state.fallback_hosts.contains("example.test"));
        assert_eq!(
            fallback_http_after_upgrade_failure(&mut state, "https://example.test/other"),
            None
        );
    }

    #[test]
    fn engine_navigation_flags_default_to_false_for_missing_or_new_tabs() {
        let engine = Engine::new();
        assert!(!engine.can_go_back("non-existent-tab"));
        assert!(!engine.can_go_forward("non-existent-tab"));
    }

    #[test]
    fn windows_key_code_maps_dots_punctuation_and_letters() {
        assert_eq!(windows_key_code_for_key("."), 0xbe); // VK_OEM_PERIOD (not VK_DELETE 0x2e!)
        assert_eq!(windows_key_code_for_key(">"), 0xbe);
        assert_eq!(windows_key_code_for_key(","), 0xbc); // VK_OEM_COMMA
        assert_eq!(windows_key_code_for_key("<"), 0xbc);
        assert_eq!(windows_key_code_for_key("-"), 0xbd); // VK_OEM_MINUS
        assert_eq!(windows_key_code_for_key("_"), 0xbd);
        assert_eq!(windows_key_code_for_key("/"), 0xbf); // VK_OEM_2
        assert_eq!(windows_key_code_for_key("?"), 0xbf);
        assert_eq!(windows_key_code_for_key(";"), 0xba); // VK_OEM_1
        assert_eq!(windows_key_code_for_key(":"), 0xba);
        assert_eq!(windows_key_code_for_key("="), 0xbb); // VK_OEM_PLUS
        assert_eq!(windows_key_code_for_key("+"), 0xbb);
        assert_eq!(windows_key_code_for_key("["), 0xdb); // VK_OEM_4
        assert_eq!(windows_key_code_for_key("{"), 0xdb);
        assert_eq!(windows_key_code_for_key("\\"), 0xdc); // VK_OEM_5
        assert_eq!(windows_key_code_for_key("|"), 0xdc);
        assert_eq!(windows_key_code_for_key("]"), 0xdd); // VK_OEM_6
        assert_eq!(windows_key_code_for_key("}"), 0xdd);
        assert_eq!(windows_key_code_for_key("'"), 0xde); // VK_OEM_7
        assert_eq!(windows_key_code_for_key("\""), 0xde);
        assert_eq!(windows_key_code_for_key("`"), 0xc0); // VK_OEM_3
        assert_eq!(windows_key_code_for_key("~"), 0xc0);
        assert_eq!(windows_key_code_for_key("delete"), 0x2e); // VK_DELETE
        assert_eq!(windows_key_code_for_key("insert"), 0x2d); // VK_INSERT
        assert_eq!(windows_key_code_for_key("a"), 0x41);
        assert_eq!(windows_key_code_for_key("Z"), 0x5a);
        assert_eq!(windows_key_code_for_key("0"), 0x30);
        assert_eq!(windows_key_code_for_key("9"), 0x39);
    }

    #[test]
    fn resolve_favicon_url_handles_relative_and_absolute_links() {
        assert_eq!(
            resolve_favicon_url("https://github.com/torvalds/linux", "/favicon.svg"),
            Some("https://github.com/favicon.svg".to_owned())
        );
        assert_eq!(
            resolve_favicon_url("https://example.com/blog/post", "icon.png"),
            Some("https://example.com/blog/icon.png".to_owned())
        );
        assert_eq!(
            resolve_favicon_url("https://example.com", "//cdn.example.com/fav.ico"),
            Some("https://cdn.example.com/fav.ico".to_owned())
        );
        assert_eq!(
            resolve_favicon_url("https://example.com", "https://other.com/fav.ico"),
            Some("https://other.com/fav.ico".to_owned())
        );
        assert_eq!(
            resolve_favicon_url("https://example.com", "data:image/png;base64,abc"),
            Some("data:image/png;base64,abc".to_owned())
        );
    }

    #[test]
    fn decode_base64_decodes_bytes() {
        let input = "SGVsbG8gV29ybGQ="; // "Hello World"
        let decoded = decode_base64(input).unwrap();
        assert_eq!(decoded, b"Hello World");
    }

    #[test]
    fn favicon_url_score_prioritizes_modern_high_res_over_legacy_ico() {
        let legacy_ico = "https://example.com/favicon.ico";
        let modern_png = "https://example.com/icons/favicon-32x32.png";
        let touch_icon = "https://example.com/apple-touch-icon-180x180.png";
        let vector_svg = "https://example.com/icon.svg";

        assert!(favicon_url_score(modern_png) > favicon_url_score(legacy_ico));
        assert!(favicon_url_score(touch_icon) > favicon_url_score(legacy_ico));
        assert!(favicon_url_score(vector_svg) > favicon_url_score(legacy_ico));
    }

    #[test]
    fn disk_favicon_resolution_handles_www_and_bare_domain_symmetrically() {
        let buffer = image::ImageBuffer::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]));
        save_favicon_to_disk("www.test-domain-sym.com", &buffer);

        let loaded_with_www = load_favicon_from_disk("www.test-domain-sym.com");
        let loaded_bare = load_favicon_from_disk("test-domain-sym.com");

        assert!(loaded_with_www.is_some());
        assert!(loaded_bare.is_some());
    }

    #[test]
    fn favicon_pixels_are_converted_from_rgba_to_bgra_for_gpui() {
        let mut buffer = image::ImageBuffer::from_pixel(1, 1, image::Rgba([255, 0, 51, 255]));
        rgba_to_bgra_in_place(&mut buffer);
        assert_eq!(buffer.get_pixel(0, 0).0, [51, 0, 255, 255]);
    }

    #[test]
    fn disk_favicon_loads_with_bgra_channel_order() {
        let red = image::ImageBuffer::from_pixel(8, 8, image::Rgba([255, 0, 51, 255]));
        save_favicon_to_disk("www.test-bgra-order.com", &red);

        let image = load_favicon_from_disk("test-bgra-order.com").expect("cached favicon");
        let bytes = image.as_bytes(0).expect("frame bytes");
        assert_eq!(&bytes[..4], &[51, 0, 255, 255]);
    }

    #[test]
    fn favicon_hosts_match_across_www_and_case() {
        assert!(same_favicon_host("www.github.com", "github.com"));
        assert!(same_favicon_host("GitHub.com", "github.com"));
        assert!(same_favicon_host("github.com", "github.com"));
        assert!(!same_favicon_host("github.com", "gitlab.com"));
        assert!(!same_favicon_host("www.github.com", "gist.github.com"));
    }

    #[test]
    fn fresher_network_favicon_replaces_cached_one_but_not_a_preferred_icon() {
        // Cached (non-preferred) icon for the same host: the freshly downloaded
        // fallback must replace it so the tab stops showing the stale icon.
        assert!(should_store_favicon(true, false, false, false));
        // A page-declared icon must survive the low-res origin fallback.
        assert!(!should_store_favicon(true, false, true, false));
        // A page-declared icon always wins.
        assert!(should_store_favicon(true, true, true, false));
        // Diagnostics: never store an icon for a host the tab left.
        assert!(!should_store_favicon(false, true, false, true));
    }
}
