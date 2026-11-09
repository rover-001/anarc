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

#[derive(Default)]
struct EngineEvents {
    popups: Vec<(String, String)>,
    downloads: Vec<DownloadRecord>,
    created: Vec<(String, Browser)>,
}

fn origin_favicon_url(page_url: &str) -> Option<String> {
    let (scheme, remainder) = page_url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = remainder
        .split(|character| matches!(character, '/' | '?' | '#'))
        .next()?;
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
        let scheme = if page_url.starts_with("http://") { "http" } else { "https" };
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
    let frame = image::Frame::new(decoded.to_rgba8());
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

fn favicon_debug(message: std::fmt::Arguments<'_>) {
    if std::env::var_os("LUMEN_DEBUG_FAVICON").is_some_and(|value| value == "1") {
        eprintln!("Lumen favicon: {message}");
    }
}

pub fn log_favicon_row_rendered(tab_id: &str, has_image: bool) {
    favicon_debug(format_args!("row rendered tab={tab_id} image={has_image}"));
}

pub fn detect_system_refresh_rate() -> Option<i32> {
    if let Ok(val) = std::env::var("LUMEN_FPS") {
        if let Ok(fps) = val.parse::<i32>() {
            if (30..=360).contains(&fps) {
                return Some(fps);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Try hyprctl monitors if on Hyprland
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            if let Ok(output) = std::process::Command::new("hyprctl")
                .arg("monitors")
                .output()
            {
                if let Ok(text) = std::str::from_utf8(&output.stdout) {
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if let Some(at_idx) = trimmed.find('@') {
                            let after_at = &trimmed[at_idx + 1..];
                            let num_str: String = after_at
                                .chars()
                                .take_while(|c| c.is_ascii_digit() || *c == '.')
                                .collect();
                            if let Ok(hz) = num_str.parse::<f64>() {
                                if (30.0..=360.0).contains(&hz) {
                                    return Some(hz.round() as i32);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Fallback: try xrandr
        if let Ok(output) = std::process::Command::new("xrandr").output() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                for line in text.lines() {
                    if line.contains('*') {
                        for part in line.split_whitespace() {
                            if part.contains('*') {
                                let clean: String = part
                                    .chars()
                                    .filter(|c| c.is_ascii_digit() || *c == '.')
                                    .collect();
                                if let Ok(hz) = clean.parse::<f64>() {
                                    if (30.0..=360.0).contains(&hz) {
                                        return Some(hz.round() as i32);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn frame_rate_for_refresh(refresh_interval: Option<std::time::Duration>) -> i32 {
    let rate = refresh_interval
        .filter(|interval| !interval.is_zero())
        .map(|interval| (1.0 / interval.as_secs_f64()).round() as i32)
        .or_else(|| {
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
    rate.clamp(1, 240)
}

cef::wrap_app! {
    struct LumenCefApp;

    impl App {
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
                    "disable-gpu-shader-disk-cache",
                    "enable-smooth-scrolling",
                ] {
                    let name = CefString::from(name);
                    command_line.append_switch(Some(&name));
                }
                let name = CefString::from("autoplay-policy");
                let value = CefString::from("user-gesture-required");
                command_line.append_switch_with_value(Some(&name), Some(&value));
                let name = CefString::from("enable-features");
                let value = CefString::from("OverlayScrollbar,OverlayScrollbars");
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
    pub dirty: bool,
    pub root_dirty: bool,
    pub title: Option<String>,
    pub url: Option<String>,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub favicon: Option<Arc<RenderImage>>,
    pub favicon_host: Option<String>,
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
}

fn median_edge_color_bgra(pixels: &[u8], width: usize, height: usize) -> Option<u32> {
    if width == 0 || height == 0 || pixels.len() < width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let mut red = Vec::with_capacity(height);
    let mut green = Vec::with_capacity(height);
    let mut blue = Vec::with_capacity(height);
    let x = width - 1;
    for y in 0..height {
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

cef::wrap_load_handler! {
    struct LumenLoadHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        https_state: Arc<Mutex<HttpsFirstState>>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: i32,
            can_go_back: i32,
            can_go_forward: i32,
        ) {
            if let Ok(mut state) = self.shared.lock() {
                state.is_loading = is_loading != 0;
                state.can_go_back = can_go_back != 0;
                state.can_go_forward = can_go_forward != 0;
                state.dirty = true;
            }
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: i32,
        ) {
            let Some(frame) = frame else { return; };
            if frame.is_main() == 0 { return; }
            if let Some(browser) = browser {
                if let Some(host) = browser.host() {
                    if let Ok(state) = self.shared.lock() {
                        if let Some(zoom) = state.target_zoom_level {
                            host.set_zoom_level(zoom.clamp(-5.0, 5.0));
                        }
                    }
                }
            }
            let url = CefStringUtf16::from(&frame.url()).to_string();
            if let Ok(mut state) = self.https_state.lock() {
                clear_https_upgrade(&mut state, &url);
            }
        }

        fn on_load_error(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            _error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if error_code == Errorcode::ABORTED { return; }
            if frame.as_ref().is_some_and(|frame| frame.is_main() == 0) { return; }
            let Some(failed_url) = failed_url.map(ToString::to_string) else { return; };
            let http_url = self.https_state.lock().ok().and_then(|mut state| {
                fallback_http_after_upgrade_failure(&mut state, &failed_url)
            });
            if let Some(http_url) = http_url {
                if frame.as_ref().is_some_and(|frame| frame.is_main() != 0) {
                    if let Some(frame) = frame {
                        frame.load_url(Some(&CefString::from(http_url.as_str())));
                    }
                } else if let Some(frame) = browser.and_then(|browser| browser.main_frame()) {
                    frame.load_url(Some(&CefString::from(http_url.as_str())));
                }
            }
        }
    }
}

#[derive(Default)]
struct HttpsFirstState {
    upgraded_hosts: std::collections::HashSet<String>,
    fallback_hosts: std::collections::HashSet<String>,
}

fn remember_https_upgrade(state: &mut HttpsFirstState, https_url: &str) {
    if let Some(host) = crate::shield::url_host(https_url) {
        state.upgraded_hosts.insert(host);
    }
}

fn clear_https_upgrade(state: &mut HttpsFirstState, https_url: &str) {
    if let Some(host) = crate::shield::url_host(https_url) {
        state.upgraded_hosts.remove(&host);
    }
}

fn fallback_http_after_upgrade_failure(
    state: &mut HttpsFirstState,
    failed_url: &str,
) -> Option<String> {
    let tail = failed_url.strip_prefix("https://")?;
    let host = crate::shield::url_host(failed_url)?;
    if !state.upgraded_hosts.remove(&host) {
        return None;
    }
    state.fallback_hosts.insert(host);
    Some(format!("http://{tail}"))
}

cef::wrap_resource_request_handler! {
    struct LumenResourceRequestHandler {
        shield: Arc<crate::shield::ShieldState>,
        tab_id: String,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let (Some(frame), Some(request)) = (frame, request) else { return ReturnValue::CONTINUE; };
            let source = CefStringUtf16::from(&frame.url()).to_string();
            let url = CefStringUtf16::from(&request.url()).to_string();
            let method = CefStringUtf16::from(&request.method()).to_string();
            let resource_type = match request.resource_type() {
                ResourceType::MAIN_FRAME => "document",
                ResourceType::SUB_FRAME => "subdocument",
                ResourceType::STYLESHEET => "stylesheet",
                ResourceType::SCRIPT => "script",
                ResourceType::IMAGE => "image",
                ResourceType::FONT_RESOURCE => "font",
                ResourceType::MEDIA => "media",
                ResourceType::OBJECT | ResourceType::PLUGIN_RESOURCE => "object",
                ResourceType::XHR => "xmlhttprequest",
                ResourceType::PING => "ping",
                _ => "other",
            };
            if self.shield.should_block(&url, &source, resource_type, &method, &self.tab_id) {
                ReturnValue::CANCEL
            } else {
                ReturnValue::CONTINUE
            }
        }
    }
}

cef::wrap_request_handler! {
    struct LumenRequestHandler {
        shield: Arc<crate::shield::ShieldState>,
        tab_id: String,
        https_state: Arc<Mutex<HttpsFirstState>>,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let (Some(frame), Some(request)) = (frame, request) else { return 0; };
            if frame.is_main() == 0 { return 0; }
            let url = CefStringUtf16::from(&request.url()).to_string();
            if let Some(upgraded) = crate::shield::upgrade_http_url(&url) {
                let host = crate::shield::url_host(&url).unwrap_or_default();
                let skip = self.https_state.lock().is_ok_and(|state| state.fallback_hosts.contains(&host));
                // CEF documents this request as immutable in OnBeforeBrowse. Reload GET
                // navigations through the main frame; leave other methods untouched.
                let method = CefStringUtf16::from(&request.method()).to_string();
                if !skip && method.eq_ignore_ascii_case("GET") {
                    if let Ok(mut state) = self.https_state.lock() { remember_https_upgrade(&mut state, &upgraded); }
                    frame.load_url(Some(&CefString::from(upgraded.as_str())));
                    return 1;
                }
            }
            0
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            Some(LumenResourceRequestHandler::new(self.shield.clone(), self.tab_id.clone()))
        }
    }
}

cef::wrap_context_menu_handler! {
    struct LumenContextMenuHandler;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // CEF's OSR menu paints use PET_POPUP, which this client intentionally ignores.
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

cef::wrap_download_image_callback! {
    struct LumenFaviconCallback {
        shared: Arc<Mutex<BrowserSharedState>>,
        tab_id: String,
        wake: async_channel::Sender<()>,
        preferred: bool,
        page_host: Option<String>,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            image_url: Option<&CefString>,
            http_status_code: i32,
            favicon_image: Option<&mut Image>,
        ) {
            favicon_debug(format_args!(
                "download finished tab={} url={} status={http_status_code}",
                self.tab_id,
                image_url.map(ToString::to_string).unwrap_or_default()
            ));
            let Some(favicon_image) = favicon_image else {
                favicon_debug(format_args!("no image for tab={}", self.tab_id));
                return;
            };
            if favicon_image.is_empty() != 0 {
                favicon_debug(format_args!("empty image for tab={}", self.tab_id));
                return;
            }
            if !(200..300).contains(&http_status_code) && http_status_code != 0 && http_status_code != 304 {
                favicon_debug(format_args!("non-success HTTP status for tab={}: {http_status_code}", self.tab_id));
                return;
            }

            let mut decoded_buffer: Option<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> = None;
            for scale in [1.0f32, 2.0, 0.0] {
                if let Some(png) = favicon_image.as_png(scale, 1, None, None) {
                    let size = png.size();
                    if size > 0 && size <= 512 * 1024 {
                        let mut bytes = vec![0u8; size];
                        let copied = png.data(Some(&mut bytes), 0).min(size);
                        bytes.truncate(copied);
                        if let Ok(decoded) = image::load_from_memory(&bytes) {
                            decoded_buffer = Some(decoded.to_rgba8());
                            break;
                        }
                    }
                }
            }

            if decoded_buffer.is_none() {
                for scale in [1.0f32, 2.0, 0.0] {
                    let mut pw: ::std::os::raw::c_int = 0;
                    let mut ph: ::std::os::raw::c_int = 0;
                    if let Some(bitmap) = favicon_image.as_bitmap(
                        scale,
                        ColorType::RGBA_8888,
                        AlphaType::PREMULTIPLIED,
                        Some(&mut pw),
                        Some(&mut ph),
                    ) {
                        let size = bitmap.size();
                        let expected = (pw as usize) * (ph as usize) * 4;
                        if pw > 0 && ph > 0 && size >= expected {
                            let mut bytes = vec![0u8; size];
                            bitmap.data(Some(&mut bytes), 0);
                            bytes.truncate(expected);
                            if let Some(img_buffer) =
                                image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(pw as u32, ph as u32, bytes)
                            {
                                decoded_buffer = Some(img_buffer);
                                break;
                            }
                        }
                    }
                }
            }

            let Some(img_buffer) = decoded_buffer else {
                favicon_debug(format_args!("failed to decode favicon for tab={}", self.tab_id));
                return;
            };

            let frame = image::Frame::new(img_buffer.clone());
            let render_image = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
            if let Ok(mut state) = self.shared.lock() {
                let img_url_string = image_url.map(|s| s.to_string());
                let target_host = self
                    .page_host
                    .clone()
                    .or_else(|| state.current_host.clone())
                    .or_else(|| state.current_url.as_deref().and_then(crate::shield::url_host))
                    .or_else(|| state.url.as_deref().and_then(crate::shield::url_host))
                    .or_else(|| img_url_string.as_deref().and_then(crate::shield::url_host));

                if let Some(ref host) = target_host {
                    save_favicon_to_disk(host, &img_buffer);
                }

                let matches_current = match (&state.current_host, &target_host) {
                    (Some(curr), Some(target)) => {
                        curr == target
                            || curr.trim_start_matches("www.") == target.trim_start_matches("www.")
                    }
                    _ => true,
                };

                if matches_current
                    && (self.preferred
                        || state.favicon.is_none()
                        || state.favicon_host != target_host)
                {
                    state.favicon = Some(render_image);
                    state.favicon_host = target_host;
                    state.dirty = true;
                    state.root_dirty = true;
                    favicon_debug(format_args!("stored image for tab={}", self.tab_id));
                    let _ = self.wake.try_send(());
                } else {
                    favicon_debug(format_args!("kept existing image for tab={}", self.tab_id));
                }
            }
        }
    }
}

cef::wrap_render_handler! {
    struct LumenRenderHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        paint_wake: async_channel::Sender<()>,
        tab_id: String,
    }

    impl RenderHandler {
        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> i32 {
            let Some(info) = info else { return 0; };
            let state = self.shared.lock().unwrap();
            info.device_scale_factor = state.screen_scale;
            info.depth = 24;
            info.depth_per_component = 8;
            info.is_monochrome = 0;
            info.rect = Rect {
                x: state.screen_origin_x,
                y: state.screen_origin_y,
                width: state.screen_width.max(state.width),
                height: state.screen_height.max(state.height),
            };
            info.available_rect = info.rect.clone();
            1
        }

        fn screen_point(
            &self,
            _browser: Option<&mut Browser>,
            view_x: i32,
            view_y: i32,
            screen_x: Option<&mut i32>,
            screen_y: Option<&mut i32>,
        ) -> i32 {
            let state = self.shared.lock().unwrap();
            let scale = state.screen_scale.max(1.0);
            if let Some(x) = screen_x {
                *x = state.screen_origin_x + (view_x as f32 / scale) as i32;
            }
            if let Some(y) = screen_y {
                *y = state.screen_origin_y + (view_y as f32 / scale) as i32;
            }
            1
        }

        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let state = self.shared.lock().unwrap();
                rect.x = 0;
                rect.y = 0;
                rect.width = state.width.max(1);
                rect.height = state.height.max(1);
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            // Popup widgets (e.g. <select> dropdowns) arrive as separate PET_POPUP
            // buffers; they must not replace the main view frame.
            if type_ != PaintElementType::VIEW {
                return;
            }
            if width <= 0 || height <= 0 || buffer.is_null() {
                return;
            }
            let len = (width as usize) * (height as usize) * 4;
            resize_debug(format_args!(
                "paint tab={} frame={}x{}",
                self.tab_id, width, height
            ));
            let slice = unsafe { std::slice::from_raw_parts(buffer, len) };
            let copy_started = crate::stats::frame_copy_started();
            let pixels = slice.to_vec();
            let edge_color = median_edge_color_bgra(&pixels, width as usize, height as usize);
            crate::stats::record_osr_paint(copy_started);
            if let Some(img_buffer) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                width as u32,
                height as u32,
                pixels,
            ) {
                let frame = image::Frame::new(img_buffer);
                let render_image = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
                let mut state = self.shared.lock().unwrap();
                if let Some(previous) = state.image.replace(render_image) {
                    state.retired.push(previous);
                }
                state.edge_color = edge_color;
                state.dirty = true;
            }
            let _ = self.paint_wake.try_send(());
        }
    }
}

cef::wrap_display_handler! {
    struct LumenDisplayHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        tab_id: String,
        ui_update_wake: async_channel::Sender<()>,
    }

    impl DisplayHandler {
        fn on_favicon_urlchange(
            &self,
            browser: Option<&mut Browser>,
            icon_urls: Option<&mut CefStringList>,
        ) {
            let Some(icon_urls) = icon_urls else {
                return;
            };
            let (page_url, page_host) = self
                .shared
                .lock()
                .ok()
                .map(|state| {
                    let url = state
                        .current_url
                        .clone()
                        .or_else(|| state.url.clone())
                        .unwrap_or_default();
                    let host = state
                        .current_host
                        .clone()
                        .or_else(|| crate::shield::url_host(&url));
                    (url, host)
                })
                .unwrap_or_default();

            let mut urls: Vec<String> = std::mem::take(icon_urls).into_iter().collect();
            urls.sort_by_key(|u| std::cmp::Reverse(favicon_url_score(u)));

            let resolved_url = urls.iter().find_map(|raw_url| {
                resolve_favicon_url(&page_url, raw_url)
            });
            let Some(url) = resolved_url else {
                favicon_debug(format_args!("no valid icon URL for tab={}", self.tab_id));
                return;
            };
            favicon_debug(format_args!("URL found tab={} url={url}", self.tab_id));
            if url.starts_with("data:image/") {
                if let Some(comma) = url.find(',') {
                    let data_part = &url[comma + 1..];
                    if let Some(bytes) = decode_base64(data_part) {
                        if let Ok(decoded) = image::load_from_memory(&bytes) {
                            let rgba = decoded.to_rgba8();
                            let target_host = page_host.clone().or_else(|| crate::shield::url_host(&page_url));
                            if let Some(ref host) = target_host {
                                save_favicon_to_disk(host, &rgba);
                            }
                            let frame = image::Frame::new(rgba);
                            let render_image = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
                            if let Ok(mut state) = self.shared.lock() {
                                state.favicon = Some(render_image);
                                state.favicon_host = target_host;
                                state.dirty = true;
                                state.root_dirty = true;
                                let _ = self.ui_update_wake.try_send(());
                            }
                            return;
                        }
                    }
                }
            }
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                favicon_debug(format_args!("no browser host for icon tab={}", self.tab_id));
                return;
            };
            let mut callback = LumenFaviconCallback::new(
                self.shared.clone(),
                self.tab_id.clone(),
                self.ui_update_wake.clone(),
                true,
                page_host,
            );
            favicon_debug(format_args!("download started tab={} url={url}", self.tab_id));
            host.download_image(
                Some(&CefString::from(url.as_str())),
                1,
                64,
                0,
                Some(&mut callback),
            );
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(title) = title {
                let title = title.to_string();
                let mut state = self.shared.lock().unwrap();
                state.title = Some(title.clone());
                if let Some(url) = state.url.as_deref() {
                    crate::history::update_title(url, &title);
                }
                state.dirty = true;
                state.root_dirty = true;
            }
        }

        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: c_ulong,
            cursor_type: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> i32 {
            let cursor = match cursor_type {
                CursorType::IBEAM | CursorType::VERTICALTEXT => 1,
                CursorType::CROSS => 2,
                CursorType::HAND => 3,
                CursorType::EASTWESTRESIZE | CursorType::COLUMNRESIZE => 5,
                CursorType::NORTHSOUTHRESIZE | CursorType::ROWRESIZE => 6,
                CursorType::GRAB => 7,
                CursorType::GRABBING => 8,
                CursorType::NOTALLOWED | CursorType::NODROP => 9,
                CursorType::ALIAS | CursorType::DND_LINK => 10,
                CursorType::COPY | CursorType::DND_COPY => 11,
                CursorType::CONTEXTMENU => 12,
                _ => 0,
            };
            if let Ok(mut state) = self.shared.lock() {
                state.cursor = cursor;
                state.dirty = true;
            }
            1
        }

        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if let Some(url) = url {
                let page_url = url.to_string();
                let is_main_frame = frame.as_ref().is_some_and(|frame| frame.is_main() != 0);
                if !is_main_frame {
                    return;
                }
                let new_host = crate::shield::url_host(&page_url);
                if let Ok(mut state) = self.shared.lock() {
                    state.url = Some(page_url.clone());
                    state.current_url = Some(page_url.clone());
                    state.title = None;
                    let host_changed = match (&state.current_host, &new_host) {
                        (Some(curr), Some(new_h)) => {
                            curr != new_h
                                && curr.trim_start_matches("www.") != new_h.trim_start_matches("www.")
                        }
                        (None, Some(_)) => true,
                        (Some(_), None) => true,
                        (None, None) => false,
                    };
                    if host_changed {
                        state.current_host = new_host.clone();
                        state.favicon = None;
                        state.favicon_host = None;
                        if let Some(ref host) = new_host {
                            if let Some(cached) = load_favicon_from_disk(host) {
                                state.favicon = Some(cached);
                                state.favicon_host = Some(host.clone());
                            }
                        }
                    } else if state.favicon.is_none() {
                        if let Some(ref host) = new_host {
                            if let Some(cached) = load_favicon_from_disk(host) {
                                state.favicon = Some(cached);
                                state.favicon_host = Some(host.clone());
                            }
                        }
                    }
                    crate::history::record_visit(&page_url, "");
                    state.dirty = true;
                    state.root_dirty = true;
                }
                let _ = self.ui_update_wake.try_send(());
                if let Some(favicon_url) = origin_favicon_url(&page_url)
                    && let Some(host) = browser.and_then(|browser| browser.host())
                {
                    let mut callback = LumenFaviconCallback::new(
                        self.shared.clone(),
                        self.tab_id.clone(),
                        self.ui_update_wake.clone(),
                        false,
                        new_host,
                    );
                    favicon_debug(format_args!(
                        "fallback download started tab={} url={favicon_url}",
                        self.tab_id
                    ));
                    host.download_image(
                        Some(&CefString::from(favicon_url.as_str())),
                        1,
                        64,
                        0,
                        Some(&mut callback),
                    );
                } else {
                    favicon_debug(format_args!(
                        "no HTTP origin fallback for tab={} page={page_url}",
                        self.tab_id
                    ));
                }
            }
        }
    }
}

cef::wrap_life_span_handler! {
    struct LumenLifeSpanHandler {
        events: Arc<Mutex<EngineEvents>>,
        space_id: String,
        tab_id: String,
        paint_wake: async_channel::Sender<()>,
        close_state: Arc<(Mutex<usize>, Condvar)>,
        shutdown_requested: Arc<std::sync::atomic::AtomicBool>,
        frame_rate: i32,
        initial_zoom_level: f64,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return; };
            if let Some(host) = browser.host() {
                host.set_windowless_frame_rate(self.frame_rate);
                host.set_zoom_level(self.initial_zoom_level.clamp(-5.0, 5.0));
                if std::env::var_os("LUMEN_STATS").is_some_and(|value| value == "1") {
                    eprintln!("Lumen CEF OSR frame rate: {} fps", host.windowless_frame_rate());
                }
            }
            if self.shutdown_requested.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            } else if let Ok(mut events) = self.events.lock() {
                if self.shutdown_requested.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some(host) = browser.host() { host.close_browser(1); }
                } else {
                    events.created.push((self.tab_id.clone(), browser.clone()));
                    let _ = self.paint_wake.try_send(());
                }
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            let (count, wake) = &*self.close_state;
            if let Ok(mut count) = count.lock() {
                *count = count.saturating_sub(1);
                wake.notify_all();
            }
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let url = target_url.map(ToString::to_string).unwrap_or_default();
            if let Ok(mut events) = self.events.lock() {
                events.popups.push((
                    self.space_id.clone(),
                    if url.is_empty() { "about:blank".to_owned() } else { url },
                ));
            }
            1
        }
    }
}

cef::wrap_download_handler! {
    struct LumenDownloadHandler {
        events: Arc<Mutex<EngineEvents>>,
    }

    impl DownloadHandler {
        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> i32 {
            let (Some(item), Some(callback)) = (item, callback) else { return 0; };
            let suggested_file_name = item.suggested_file_name();
            let name = suggested_name
                .map(ToString::to_string)
                .unwrap_or_else(|| CefStringUtf16::from(&suggested_file_name).to_string());
            let mut events = self.events.lock().ok();
            let reserved = events
                .as_ref()
                .map(|events| events.downloads.as_slice())
                .unwrap_or_default();
            let path = unique_download_path(&name, reserved);
            let cef_path = CefString::from(path.to_string_lossy().as_ref());
            let record = DownloadRecord {
                id: item.id(),
                url: CefStringUtf16::from(&item.url()).to_string(),
                path,
                received_bytes: 0,
                total_bytes: item.total_bytes(),
                complete: false,
                canceled: false,
            };
            if let Some(events) = events.as_mut() {
                events.downloads.push(record.clone());
            }
            drop(events);
            callback.cont(Some(&cef_path), 0);
            eprintln!("Lumen download started: {} -> {}", record.url, record.path.display());
            0
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            item: Option<&mut DownloadItem>,
            _callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = item else { return; };
            let received = item.received_bytes();
            let total = item.total_bytes();
            let complete = item.is_complete() != 0;
            let canceled = item.is_canceled() != 0 || item.is_interrupted() != 0;
            if let Ok(mut events) = self.events.lock() {
                if let Some(record) = events.downloads.iter_mut().find(|record| record.id == item.id()) {
                    record.received_bytes = received;
                    record.total_bytes = total;
                    record.complete = complete;
                    record.canceled = canceled;
                    if complete || canceled {
                        eprintln!("Lumen download {}: {} ({}/{})", if complete { "complete" } else { "failed" }, record.path.display(), received, total);
                    } else {
                        eprintln!("Lumen download progress: {} ({}/{})", record.path.display(), received, total);
                    }
                }
            }
        }
    }
}

fn unique_download_path(name: &str, reserved: &[DownloadRecord]) -> PathBuf {
    let downloads = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Downloads");
    if let Err(error) = std::fs::create_dir_all(&downloads) {
        eprintln!(
            "Lumen cannot create download directory {}: {error}",
            downloads.display()
        );
    }
    let filename = Path::new(name)
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("download"));
    let candidate = downloads.join(filename);
    if !candidate.exists() && !reserved.iter().any(|record| record.path == candidate) {
        return candidate;
    }
    let stem = Path::new(filename)
        .file_stem()
        .unwrap_or(filename)
        .to_string_lossy();
    let extension = Path::new(filename)
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|index| downloads.join(format!("{stem} ({index}){extension}")))
        .find(|path| !path.exists() && !reserved.iter().any(|record| record.path == *path))
        .unwrap_or(candidate)
}

cef::wrap_permission_handler! {
    struct LumenPermissionHandler;

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _origin: Option<&CefString>,
            _permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            if let Some(callback) = callback { callback.cont(0); }
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            _origin: Option<&CefString>,
            _permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            if let Some(callback) = callback { callback.cont(PermissionRequestResult::DENY); }
            1
        }
    }
}

cef::wrap_client! {
    struct LumenClient {
        render_handler: RenderHandler,
        display_handler: DisplayHandler,
        life_span_handler: LifeSpanHandler,
        download_handler: DownloadHandler,
        permission_handler: PermissionHandler,
        load_handler: LoadHandler,
        context_menu_handler: ContextMenuHandler,
        request_handler: RequestHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display_handler.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life_span_handler.clone()) }
        fn download_handler(&self) -> Option<DownloadHandler> { Some(self.download_handler.clone()) }
        fn permission_handler(&self) -> Option<PermissionHandler> { Some(self.permission_handler.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(self.context_menu_handler.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> { Some(self.request_handler.clone()) }
    }
}

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
    if *ENABLED.get_or_init(|| std::env::var("LUMEN_DEBUG_RESIZE").as_deref() == Ok("1")) {
        eprintln!("[resize-debug] {message}");
    }
}

impl Engine {
    pub fn new() -> Self {
        let (ui_update_wake, ui_update_receiver) = async_channel::bounded(1);
        Self {
            browsers: HashMap::new(),
            resize_state: HashMap::new(),
            pending_browsers: HashMap::new(),
            canceled_browsers: HashSet::new(),
            contexts: HashMap::new(),
            initialized: false,
            args: Args::new(),
            app: None,
            visible: Vec::new(),
            lifecycle: TabLifecycle::default(),
            events: Arc::new(Mutex::new(EngineEvents::default())),
            retired_images: Vec::new(),
            ui_update_wake,
            ui_update_receiver,
            windowless_frame_rate: 60,
            close_state: Arc::new((Mutex::new(0), Condvar::new())),
            shutdown_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shield: crate::shield::ShieldState::start(),
            https_state: Arc::new(Mutex::new(HttpsFirstState::default())),
        }
    }

    /// Called only after the GPUI window has painted its initial frame.
    pub fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        let mut app = LumenCefApp::new();
        let subprocess = cef::execute_process(
            Some(self.args.as_main_args()),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        if subprocess >= 0 {
            return Err(format!("CEF subprocess exited with code {subprocess}"));
        }

        let mut settings = Settings::default();
        settings.no_sandbox = 1;
        settings.external_message_pump = 0;
        settings.multi_threaded_message_loop = 1;
        settings.windowless_rendering_enabled = 1;
        settings.locale = CefString::from("en-US");
        settings.user_agent = CefString::from(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36",
        );
        let cache_root = crate::paths::cef_root_dir();
        std::fs::create_dir_all(&cache_root)
            .map_err(|error| format!("cannot create CEF cache directory: {error}"))?;
        settings.root_cache_path = CefString::from(cache_root.to_string_lossy().as_ref());
        let initialized = cef::initialize(
            Some(self.args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        if initialized == 0 {
            return Err(
                "CEF initialization failed; check CEF resources and shared libraries".into(),
            );
        }
        self.app = Some(app);
        self.initialized = true;
        Ok(())
    }

    pub fn set_windowless_frame_rate(&mut self, frame_rate: i32) {
        self.windowless_frame_rate = frame_rate.clamp(1, 240);
        for entry in self.browsers.values() {
            if let Some(host) = entry.browser.host() {
                host.set_windowless_frame_rate(self.windowless_frame_rate);
            }
        }
    }

    pub fn take_created_browsers(&mut self) -> bool {
        let created = self
            .events
            .lock()
            .ok()
            .map(|mut events| std::mem::take(&mut events.created))
            .unwrap_or_default();
        let mut changed = false;
        for (id, browser) in created {
            if self.canceled_browsers.remove(&id) {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
                self.pending_browsers.remove(&id);
                changed = true;
                continue;
            }
            if let Some(state) = self.pending_browsers.remove(&id) {
                let dimensions = state.lock().ok().map(|state| (state.width, state.height));
                if let Some((width, height)) = dimensions {
                    self.resize_state
                        .insert(id.clone(), (Instant::now(), width, height));
                }
                if let Ok(guard) = state.lock() {
                    if let Some(zoom) = guard.target_zoom_level {
                        if let Some(host) = browser.host() {
                            host.set_zoom_level(zoom.clamp(-5.0, 5.0));
                        }
                    }
                }
                self.browsers
                    .insert(id.clone(), BrowserEntry { browser, state });
                self.lifecycle.opened(&id, Instant::now());
                changed = true;
            }
        }
        changed
    }

    pub fn ui_update_receiver(&self) -> async_channel::Receiver<()> {
        self.ui_update_receiver.clone()
    }

    pub fn take_dirty(&self, active_tab_id: &str) -> (bool, bool) {
        self.take_dirty_for(&[active_tab_id])
    }

    pub fn take_dirty_for(&self, active_tab_ids: &[&str]) -> (bool, bool) {
        self.browsers
            .iter()
            .fold((false, false), |(dirty, root_dirty), (id, entry)| {
                let (entry_dirty, entry_root_dirty) =
                    entry.state.lock().ok().map_or((false, false), |mut state| {
                        let is_visible = active_tab_ids.contains(&id.as_str());
                        let entry_dirty = is_visible && state.dirty;
                        if is_visible {
                            state.dirty = false;
                        }
                        let entry_root_dirty = state.root_dirty;
                        state.root_dirty = false;
                        (entry_dirty, entry_root_dirty)
                    });
                (dirty || entry_dirty, root_dirty || entry_root_dirty)
            })
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn ensure_browser(
        &mut self,
        space_id: &str,
        id: &str,
        url: &str,
        width: i32,
        height: i32,
        background_color: u32,
        initial_zoom_level: f64,
    ) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if !self.browsers.contains_key(id) && !self.pending_browsers.contains_key(id) {
            let initial_host = crate::shield::url_host(url);
            let initial_favicon = initial_host.as_deref().and_then(load_favicon_from_disk);
            let shared = Arc::new(Mutex::new(BrowserSharedState {
                width: width.max(1),
                height: height.max(1),
                image: None,
                edge_color: None,
                dirty: false,
                root_dirty: false,
                title: None,
                url: None,
                is_loading: true,
                can_go_back: false,
                can_go_forward: false,
                favicon: initial_favicon,
                favicon_host: initial_host.clone(),
                current_url: Some(url.to_string()),
                current_host: initial_host,
                retired: Vec::new(),
                screen_width: width.max(1),
                screen_height: height.max(1),
                screen_scale: 1.0,
                screen_origin_x: 0,
                screen_origin_y: 0,
                cursor: 0,
                target_zoom_level: Some(initial_zoom_level),
            }));
            let render_handler =
                LumenRenderHandler::new(shared.clone(), self.ui_update_wake.clone(), id.to_owned());
            let display_handler = LumenDisplayHandler::new(
                shared.clone(),
                id.to_owned(),
                self.ui_update_wake.clone(),
            );
            let life_span_handler = LumenLifeSpanHandler::new(
                self.events.clone(),
                space_id.to_owned(),
                id.to_owned(),
                self.ui_update_wake.clone(),
                self.close_state.clone(),
                self.shutdown_requested.clone(),
                self.windowless_frame_rate,
                initial_zoom_level,
            );
            let download_handler = LumenDownloadHandler::new(self.events.clone());
            let permission_handler = LumenPermissionHandler::new();
            let load_handler = LumenLoadHandler::new(shared.clone(), self.https_state.clone());
            let context_menu_handler = LumenContextMenuHandler::new();
            let request_handler = LumenRequestHandler::new(
                self.shield.clone(),
                id.to_owned(),
                self.https_state.clone(),
            );
            let mut client = LumenClient::new(
                render_handler,
                display_handler,
                life_span_handler,
                download_handler,
                permission_handler,
                load_handler,
                context_menu_handler,
                request_handler,
            );

            let window_info = WindowInfo::default().set_as_windowless(0);
            let start_url = CefString::from(url);

            if !self.contexts.contains_key(space_id) {
                let cache_dir = crate::paths::profile_dir(space_id);
                let _ = std::fs::create_dir_all(&cache_dir);

                let mut settings = RequestContextSettings::default();
                settings.cache_path = CefString::from(cache_dir.to_string_lossy().as_ref());
                settings.persist_session_cookies = 1;

                if let Some(ctx) = cef::request_context_create_context(Some(&settings), None) {
                    self.contexts.insert(space_id.to_string(), ctx);
                }
            }

            let mut request_ctx = self.contexts.get_mut(space_id);

            let mut browser_settings = BrowserSettings::default();
            browser_settings.background_color = background_color;
            // CEF's OSR default is 30 fps, which makes scrolling and animations visibly choppy.
            browser_settings.windowless_frame_rate = self.windowless_frame_rate;

            let (count, _) = &*self.close_state;
            *count.lock().unwrap() += 1;
            let started = cef::browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&start_url),
                Some(&browser_settings),
                None,
                request_ctx.as_deref_mut(),
            );
            if started == 0 {
                *count.lock().unwrap() -= 1;
                return Err("CEF failed to start asynchronous browser creation".into());
            }
            self.pending_browsers.insert(id.to_owned(), shared);
        } else {
            self.resize_browser(id, width, height);
        }
        Ok(())
    }

    /// Marks only `tab_id` as visible. Hidden OSR browsers stop producing frames,
    /// so background tabs no longer burn CPU or trigger UI re-renders.
    pub fn set_visible(&mut self, tab_id: &str) {
        self.set_visible_tabs(&[tab_id]);
    }

    pub fn set_visible_tabs(&mut self, tab_ids: &[&str]) {
        let next = tab_ids
            .iter()
            .filter(|id| self.browsers.contains_key(**id))
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>();
        if self.visible == next {
            return;
        }
        let now = Instant::now();
        for previous in &self.visible {
            if !next.contains(previous) {
                self.lifecycle.deactivated(previous, now);
            }
        }
        for (id, entry) in &self.browsers {
            if let Some(host) = entry.browser.host() {
                host.was_hidden((!next.contains(id)) as i32);
            }
        }
        for tab_id in &next {
            if !self.visible.contains(tab_id) {
                self.lifecycle.activated(tab_id, now);
            }
        }
        self.visible = next;
    }

    pub fn hide_all(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let previous = std::mem::take(&mut self.visible);
        let now = Instant::now();
        for id in previous {
            self.lifecycle.deactivated(&id, now);
        }
        for entry in self.browsers.values() {
            if let Some(host) = entry.browser.host() {
                host.was_hidden(1);
            }
        }
    }

    pub fn close_browser(&mut self, tab_id: &str) {
        self.close_browser_entry(tab_id);
        self.lifecycle.closed(tab_id);
        self.resize_state.remove(tab_id);
        self.visible.retain(|id| id != tab_id);
    }

    fn close_browser_entry(&mut self, tab_id: &str) {
        if self.pending_browsers.contains_key(tab_id) {
            self.canceled_browsers.insert(tab_id.to_owned());
        }
        if let Some(entry) = self.browsers.remove(tab_id) {
            if let Some(host) = entry.browser.host() {
                host.close_browser(1);
            }
            if let Ok(mut state) = entry.state.lock() {
                self.retired_images.extend(state.retired.drain(..));
                if let Some(image) = state.image.take() {
                    self.retired_images.push(image);
                }
                if let Some(image) = state.favicon.take() {
                    self.retired_images.push(image);
                }
            }
        }
    }

    pub fn discard_inactive(&mut self) -> Vec<String> {
        let expired = self
            .lifecycle
            .discard_expired_many(Instant::now(), &self.visible);
        for tab_id in &expired {
            self.close_browser_entry(tab_id);
        }
        expired
    }

    pub fn tab_state(&self, tab_id: &str) -> Option<TabState> {
        self.lifecycle.state(tab_id)
    }

    pub fn has_browser(&self, tab_id: &str) -> bool {
        self.browsers.contains_key(tab_id)
    }

    pub fn set_screen_metrics(
        &self,
        tab_id: &str,
        screen_width: i32,
        screen_height: i32,
        scale: f32,
        origin_x: i32,
        origin_y: i32,
    ) {
        if let Some(entry) = self.browsers.get(tab_id)
            && let Ok(mut state) = entry.state.lock()
        {
            let changed = state.screen_width != screen_width.max(1)
                || state.screen_height != screen_height.max(1)
                || (state.screen_scale - scale.max(1.0)).abs() > f32::EPSILON
                || state.screen_origin_x != origin_x
                || state.screen_origin_y != origin_y;
            state.screen_width = screen_width.max(1);
            state.screen_height = screen_height.max(1);
            state.screen_scale = scale.max(1.0);
            state.screen_origin_x = origin_x;
            state.screen_origin_y = origin_y;
            drop(state);
            if changed {
                if let Some(host) = entry.browser.host() {
                    host.notify_screen_info_changed();
                }
            }
        }
    }

    pub fn take_popups(&self) -> Vec<(String, String)> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut events.popups))
            .unwrap_or_default()
    }

    pub fn downloads(&self) -> Vec<DownloadRecord> {
        self.events
            .lock()
            .map(|events| events.downloads.clone())
            .unwrap_or_default()
    }

    pub fn go_back(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id).map(|entry| entry.browser.clone())
            && browser.can_go_back() != 0
        {
            browser.go_back();
        }
    }

    pub fn go_forward(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id).map(|entry| entry.browser.clone())
            && browser.can_go_forward() != 0
        {
            browser.go_forward();
        }
    }

    pub fn stop_load(&self, tab_id: &str) {
        if let Some(browser) = self.browsers.get(tab_id) {
            browser.browser.stop_load();
        }
    }

    pub fn is_loading(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.is_loading)
                .unwrap_or(false)
        })
    }

    pub fn can_go_back(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.can_go_back)
                .unwrap_or(false)
        })
    }

    pub fn can_go_forward(&self, tab_id: &str) -> bool {
        self.browsers.get(tab_id).is_some_and(|entry| {
            entry
                .state
                .lock()
                .map(|state| state.can_go_forward)
                .unwrap_or(false)
        })
    }

    pub fn find(&self, tab_id: &str, query: &str, forward: bool, next: bool) {
        if let Some(host) = self
            .browsers
            .get(tab_id)
            .and_then(|entry| entry.browser.host())
        {
            host.find(
                Some(&CefString::from(query)),
                forward as i32,
                0,
                next as i32,
            );
        }
    }

    pub fn stop_finding(&self, tab_id: &str) {
        if let Some(host) = self
            .browsers
            .get(tab_id)
            .and_then(|entry| entry.browser.host())
        {
            host.stop_finding(1);
        }
    }

    pub fn set_zoom_level(&self, tab_id: &str, level: f64) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Ok(mut state) = entry.state.lock() {
                state.target_zoom_level = Some(level);
            }
            if let Some(host) = entry.browser.host() {
                host.set_zoom_level(level.clamp(-5.0, 5.0));
            }
        } else if let Some(state) = self.pending_browsers.get(tab_id) {
            if let Ok(mut state) = state.lock() {
                state.target_zoom_level = Some(level);
            }
        }
    }

    pub fn get_image(&self, tab_id: &str) -> Option<Arc<RenderImage>> {
        self.browsers.get(tab_id).and_then(|entry| {
            entry
                .state
                .lock()
                .ok()
                .and_then(|state| state.image.clone())
        })
    }

    pub fn browser_size(&self, tab_id: &str) -> Option<(i32, i32)> {
        self.browsers
            .get(tab_id)
            .and_then(|entry| {
                entry
                    .state
                    .lock()
                    .ok()
                    .map(|state| (state.width, state.height))
            })
            .or_else(|| {
                self.pending_browsers
                    .get(tab_id)
                    .and_then(|state| state.lock().ok().map(|state| (state.width, state.height)))
            })
    }

    pub fn frame_edge_color(&self, tab_id: &str) -> Option<u32> {
        self.browsers
            .get(tab_id)
            .and_then(|entry| entry.state.lock().ok().and_then(|state| state.edge_color))
    }

    pub fn frame_matches_size(&self, tab_id: &str, width: i32, height: i32) -> bool {
        self.browsers
            .get(tab_id)
            .and_then(|entry| {
                entry.state.lock().ok().and_then(|state| {
                    state.image.as_ref().map(|image| {
                        let image_size = image.size(0);
                        let scale = state.screen_scale.max(f32::EPSILON);
                        (u32::from(image_size.width) as f32 / scale - width as f32).abs() < 1.0
                            && (u32::from(image_size.height) as f32 / scale - height as f32).abs()
                                < 1.0
                    })
                })
            })
            .unwrap_or(false)
    }

    pub fn get_cursor(&self, tab_id: &str) -> i32 {
        self.browsers
            .get(tab_id)
            .and_then(|entry| entry.state.lock().ok().map(|state| state.cursor))
            .unwrap_or(0)
    }

    /// Drains frames replaced since the last call so the caller can evict them from
    /// GPUI's sprite atlas (each OSR frame is a distinct RenderImage/texture).
    pub fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut retired = std::mem::take(&mut self.retired_images);
        retired.extend(
            self.browsers
                .values()
                .flat_map(|entry| {
                    entry
                        .state
                        .lock()
                        .map(|mut state| std::mem::take(&mut state.retired))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>(),
        );
        retired
    }

    pub fn get_favicon_image(&self, tab_id: &str, fallback_url: Option<&str>) -> Option<Arc<RenderImage>> {
        let expected_host = fallback_url.and_then(crate::shield::url_host);
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Ok(mut state) = entry.state.lock() {
                if let Some(ref expected) = expected_host {
                    let matches_current = state.current_host.as_deref().is_some_and(|h| {
                        h == expected.as_str()
                            || h.trim_start_matches("www.") == expected.trim_start_matches("www.")
                    });
                    let matches_favicon = state.favicon_host.as_deref().is_some_and(|h| {
                        h == expected.as_str()
                            || h.trim_start_matches("www.") == expected.trim_start_matches("www.")
                    });

                    if !matches_current && !matches_favicon && state.favicon.is_some() {
                        state.favicon = None;
                        state.favicon_host = None;
                    }
                }

                if let Some(img) = state.favicon.clone() {
                    return Some(img);
                }

                let target_host = expected_host
                    .or_else(|| state.current_host.clone())
                    .or_else(|| state.current_url.as_deref().and_then(crate::shield::url_host))
                    .or_else(|| state.url.as_deref().and_then(crate::shield::url_host));

                if let Some(ref host) = target_host {
                    if let Some(img) = load_favicon_from_disk(host) {
                        state.favicon = Some(img.clone());
                        state.favicon_host = Some(host.clone());
                        return Some(img);
                    }
                }
            }
        } else if let Some(ref host) = expected_host {
            return load_favicon_from_disk(host);
        }
        None
    }

    pub fn take_updates(&self, tab_id: &str) -> Option<(Option<String>, Option<String>)> {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Ok(mut state) = entry.state.lock() {
                if state.title.is_some() || state.url.is_some() {
                    let title = state.title.take();
                    let url = state.url.take();
                    return Some((title, url));
                }
            }
        }
        None
    }

    pub fn resize_browser(&mut self, tab_id: &str, width: i32, height: i32) {
        self.resize_browser_inner(tab_id, width, height, false);
    }

    pub fn flush_resize_browser(&mut self, tab_id: &str, width: i32, height: i32) {
        self.resize_browser_inner(tab_id, width, height, true);
    }

    fn resize_browser_inner(&mut self, tab_id: &str, width: i32, height: i32, force: bool) {
        let w = width.max(1);
        let h = height.max(1);
        let now = Instant::now();
        let should_resize = resize_should_send(self.resize_state.get(tab_id), (w, h), now, force);
        if should_resize {
            resize_debug(format_args!("resize tab={tab_id} requested={w}x{h}"));
            self.resize_state.insert(tab_id.to_owned(), (now, w, h));
            if let Some(entry) = self.browsers.get(tab_id) {
                let mut state = entry.state.lock().unwrap();
                state.width = w;
                state.height = h;
                drop(state);
                if let Some(host) = entry.browser.host() {
                    host.was_resized();
                }
            } else if let Some(state) = self.pending_browsers.get(tab_id) {
                let mut state = state.lock().unwrap();
                state.width = w;
                state.height = h;
            }
        }
    }

    pub fn send_mouse_click(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        button: MouseButtonType,
        mouse_up: bool,
        click_count: i32,
        modifiers: &gpui_kit::Modifiers,
    ) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent {
                    x,
                    y,
                    modifiers: cef_mouse_modifiers(modifiers),
                };
                host.send_mouse_click_event(Some(&event), button, mouse_up as i32, click_count);
            }
        }
    }

    pub fn send_mouse_move(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        mouse_leave: bool,
        modifiers: &gpui_kit::Modifiers,
    ) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent {
                    x,
                    y,
                    modifiers: cef_mouse_modifiers(modifiers),
                };
                host.send_mouse_move_event(Some(&event), mouse_leave as i32);
            }
        }
    }

    pub fn send_mouse_wheel(
        &self,
        tab_id: &str,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: &gpui_kit::Modifiers,
    ) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent {
                    x,
                    y,
                    modifiers: cef_mouse_modifiers(modifiers),
                };
                host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
                let _ = self.ui_update_wake.try_send(());
            }
        }
    }

    pub fn navigate(&mut self, id: &str, url: &str) {
        if let Some(entry) = self.browsers.get(id) {
            let new_host = crate::shield::url_host(url);
            if let Ok(mut state) = entry.state.lock() {
                state.current_url = Some(url.to_string());
                let host_changed = match (&state.current_host, &new_host) {
                    (Some(curr), Some(new_h)) => {
                        curr != new_h
                            && curr.trim_start_matches("www.") != new_h.trim_start_matches("www.")
                    }
                    (None, Some(_)) => true,
                    (Some(_), None) => true,
                    (None, None) => false,
                };
                if host_changed {
                    state.current_host = new_host.clone();
                    state.favicon = None;
                    state.favicon_host = None;
                    if let Some(ref host) = new_host {
                        if let Some(cached) = load_favicon_from_disk(host) {
                            state.favicon = Some(cached);
                            state.favicon_host = Some(host.clone());
                        }
                    }
                    state.root_dirty = true;
                }
            }
            if let Some(frame) = entry.browser.main_frame() {
                frame.load_url(Some(&CefString::from(url)));
            }
        }
    }

    pub fn navigate_http_first(&mut self, id: &str, https_url: &str) {
        if let Ok(mut state) = self.https_state.lock() {
            remember_https_upgrade(&mut state, https_url);
        }
        self.navigate(id, https_url);
    }

    pub fn toggle_shield_for_tab(&mut self, id: &str, url: &str) -> bool {
        let Some(host) = crate::shield::url_host(url) else {
            return false;
        };
        let allowed = self.shield.toggle_site(&host);
        self.reload(id, false);
        !allowed
    }

    pub fn blocked_request_counts(&self) -> HashMap<String, u64> {
        self.shield.blocked_counts()
    }

    pub fn execute_script(&self, id: &str, script: &str) {
        if let Some(frame) = self
            .browsers
            .get(id)
            .and_then(|entry| entry.browser.main_frame())
        {
            frame.execute_java_script(
                Some(&CefString::from(script)),
                Some(&CefString::from("lumen://reader-mode")),
                1,
            );
        }
    }

    pub fn reload(&self, id: &str, ignore_cache: bool) {
        if let Some(entry) = self.browsers.get(id) {
            if ignore_cache {
                entry.browser.reload_ignore_cache();
            } else {
                entry.browser.reload();
            }
        }
    }

    pub fn set_focus(&self, id: &str, focused: bool) {
        if let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) {
            host.set_focus(focused as i32);
        }
    }

    pub fn send_key_down(&self, id: &str, event: &KeyDownEvent) {
        let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) else {
            return;
        };
        let key = &event.keystroke.key;
        let character = event
            .keystroke
            .key_char
            .as_deref()
            .and_then(|value| value.encode_utf16().next())
            .or_else(|| {
                let mut chars = key.chars();
                let ch = chars.next()?;
                if chars.next().is_none() && !ch.is_control() {
                    let mut buf = [0u16; 2];
                    ch.encode_utf16(&mut buf);
                    Some(buf[0])
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let base = cef_key_event(
            key,
            &event.keystroke.modifiers,
            KeyEventType::RAWKEYDOWN,
            character,
            true,
        );
        host.send_key_event(Some(&base));
        if character != 0 {
            let text = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::CHAR,
                character,
                true,
            );
            host.send_key_event(Some(&text));
        }
    }

    pub fn send_key_up(&self, id: &str, event: &KeyUpEvent) {
        if let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) {
            let key = &event.keystroke.key;
            let key_event = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::KEYUP,
                0,
                true,
            );
            host.send_key_event(Some(&key_event));
        }
    }
}

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
        if let Ok(codes) = cache.lock() {
            if let Some(code) = codes.get(&key_name) {
                return *code;
            }
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
        assert_eq!(
            frame_rate_for_refresh(Some(std::time::Duration::from_millis(4))),
            240
        );
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
}
