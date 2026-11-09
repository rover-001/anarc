//! Modern windowless CEF boundary.
//! Uses CEF Off-Screen Rendering (OSR) directly into GPUI RenderImages.

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::c_ulong;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
                let value = CefString::from("OverlayScrollbar");
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
    pub dirty: bool,
    pub title: Option<String>,
    pub url: Option<String>,
    pub is_loading: bool,
    pub favicon: Option<Arc<RenderImage>>,
    /// Superseded frames that must be evicted from GPUI's sprite atlas on the UI thread.
    pub retired: Vec<Arc<RenderImage>>,
    pub screen_width: i32,
    pub screen_height: i32,
    pub screen_scale: f32,
    pub screen_origin_x: i32,
    pub screen_origin_y: i32,
}

cef::wrap_download_image_callback! {
    struct LumenFaviconCallback {
        shared: Arc<Mutex<BrowserSharedState>>,
        preferred: bool,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            http_status_code: i32,
            favicon_image: Option<&mut Image>,
        ) {
            if !(200..300).contains(&http_status_code) {
                return;
            }
            let Some(png) = favicon_image.and_then(|image| image.as_png(1.0, 1, None, None)) else {
                return;
            };
            let size = png.size();
            if size == 0 || size > 512 * 1024 {
                return;
            }
            let mut bytes = vec![0; size];
            let copied = png.data(Some(&mut bytes), 0).min(size);
            bytes.truncate(copied);
            let Ok(decoded) = image::load_from_memory(&bytes) else {
                return;
            };
            let frame = image::Frame::new(decoded.to_rgba8());
            let render_image = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
            if let Ok(mut state) = self.shared.lock() {
                if self.preferred || state.favicon.is_none() {
                    state.favicon = Some(render_image);
                    state.dirty = true;
                }
            }
        }
    }
}

cef::wrap_render_handler! {
    struct LumenRenderHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
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
            let slice = unsafe { std::slice::from_raw_parts(buffer, len) };
            let copy_started = crate::stats::frame_copy_started();
            let pixels = slice.to_vec();
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
                state.dirty = true;
                state.is_loading = false;
            }
        }
    }
}

cef::wrap_display_handler! {
    struct LumenDisplayHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
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
            let Some(url) = std::mem::take(icon_urls)
                .into_iter()
                .find(|url| url.starts_with("http://") || url.starts_with("https://"))
            else {
                return;
            };
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return;
            };
            let mut callback = LumenFaviconCallback::new(self.shared.clone(), true);
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
                let mut state = self.shared.lock().unwrap();
                state.title = Some(title.to_string());
                state.dirty = true;
            }
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
                if let Ok(mut state) = self.shared.lock() {
                    state.url = Some(page_url.clone());
                    state.dirty = true;
                }
                if let Some(favicon_url) = origin_favicon_url(&page_url)
                    && let Some(host) = browser.and_then(|browser| browser.host())
                {
                    let mut callback = LumenFaviconCallback::new(self.shared.clone(), false);
                    host.download_image(
                        Some(&CefString::from(favicon_url.as_str())),
                        1,
                        64,
                        0,
                        Some(&mut callback),
                    );
                }
            }
        }
    }
}

cef::wrap_life_span_handler! {
    struct LumenLifeSpanHandler {
        events: Arc<Mutex<EngineEvents>>,
        space_id: String,
    }

    impl LifeSpanHandler {
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
            let cef_path = CefString::from(path.to_string_lossy().as_ref());
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
    }
}

pub struct BrowserEntry {
    pub browser: Browser,
    pub state: Arc<Mutex<BrowserSharedState>>,
}

pub struct Engine {
    browsers: HashMap<String, BrowserEntry>,
    contexts: HashMap<String, RequestContext>,
    initialized: bool,
    args: Args,
    app: Option<cef::App>,
    visible: Option<String>,
    lifecycle: TabLifecycle,
    events: Arc<Mutex<EngineEvents>>,
    retired_images: Vec<Arc<RenderImage>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            browsers: HashMap::new(),
            contexts: HashMap::new(),
            initialized: false,
            args: Args::new(),
            app: None,
            visible: None,
            lifecycle: TabLifecycle::default(),
            events: Arc::new(Mutex::new(EngineEvents::default())),
            retired_images: Vec::new(),
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
        settings.external_message_pump = 1;
        settings.multi_threaded_message_loop = 0;
        settings.windowless_rendering_enabled = 1;
        settings.locale = CefString::from("en-US");
        settings.user_agent = CefString::from(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36",
        );
        let cache_root = crate::paths::cache_dir();
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

    pub fn pump(&self) {
        if self.initialized {
            cef::do_message_loop_work();
        }
    }

    pub fn take_dirty(&self) -> bool {
        self.browsers.values().fold(false, |dirty, entry| {
            let entry_dirty = entry.state.lock().ok().is_some_and(|mut state| {
                let dirty = state.dirty;
                state.dirty = false;
                dirty
            });
            dirty || entry_dirty
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
    ) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if !self.browsers.contains_key(id) {
            let shared = Arc::new(Mutex::new(BrowserSharedState {
                width: width.max(1),
                height: height.max(1),
                image: None,
                dirty: false,
                title: None,
                url: None,
                is_loading: true,
                favicon: None,
                retired: Vec::new(),
                screen_width: width.max(1),
                screen_height: height.max(1),
                screen_scale: 1.0,
                screen_origin_x: 0,
                screen_origin_y: 0,
            }));
            let render_handler = LumenRenderHandler::new(shared.clone());
            let display_handler = LumenDisplayHandler::new(shared.clone());
            let life_span_handler =
                LumenLifeSpanHandler::new(self.events.clone(), space_id.to_owned());
            let download_handler = LumenDownloadHandler::new(self.events.clone());
            let permission_handler = LumenPermissionHandler::new();
            let mut client = LumenClient::new(
                render_handler,
                display_handler,
                life_span_handler,
                download_handler,
                permission_handler,
            );

            let window_info = WindowInfo::default().set_as_windowless(0);
            let start_url = CefString::from(url);

            if !self.contexts.contains_key(space_id) {
                let cache_dir = crate::paths::data_dir().join("profiles").join(space_id);
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
            browser_settings.background_color = 0xff15161a;
            // CEF's OSR default is 30 fps, which makes scrolling and animations visibly choppy.
            browser_settings.windowless_frame_rate = 60;

            let browser = cef::browser_host_create_browser_sync(
                Some(&window_info),
                Some(&mut client),
                Some(&start_url),
                Some(&browser_settings),
                None,
                request_ctx.as_deref_mut(),
            )
            .ok_or_else(|| "CEF failed to create a windowless browser window".to_string())?;

            self.browsers.insert(
                id.to_owned(),
                BrowserEntry {
                    browser,
                    state: shared,
                },
            );
            self.lifecycle.opened(id, Instant::now());
        } else {
            self.resize_browser(id, width, height);
        }
        Ok(())
    }

    /// Marks only `tab_id` as visible. Hidden OSR browsers stop producing frames,
    /// so background tabs no longer burn CPU or trigger UI re-renders.
    pub fn set_visible(&mut self, tab_id: &str) {
        if self.visible.as_deref() == Some(tab_id) || !self.browsers.contains_key(tab_id) {
            return;
        }
        let now = Instant::now();
        if let Some(previous) = self.visible.as_deref() {
            self.lifecycle.deactivated(previous, now);
        }
        for (id, entry) in &self.browsers {
            if let Some(host) = entry.browser.host() {
                host.was_hidden((id != tab_id) as i32);
            }
        }
        self.visible = Some(tab_id.to_owned());
        self.lifecycle.activated(tab_id, now);
    }

    pub fn hide_all(&mut self) {
        let Some(previous) = self.visible.take() else {
            return;
        };
        let now = Instant::now();
        self.lifecycle.deactivated(&previous, now);
        for entry in self.browsers.values() {
            if let Some(host) = entry.browser.host() {
                host.was_hidden(1);
            }
        }
    }

    pub fn close_browser(&mut self, tab_id: &str) {
        self.close_browser_entry(tab_id);
        self.lifecycle.closed(tab_id);
        if self.visible.as_deref() == Some(tab_id) {
            self.visible = None;
        }
    }

    fn close_browser_entry(&mut self, tab_id: &str) {
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
            .discard_expired(Instant::now(), self.visible.as_deref());
        for tab_id in &expired {
            self.close_browser_entry(tab_id);
        }
        expired
    }

    pub fn tab_state(&self, tab_id: &str) -> Option<TabState> {
        self.lifecycle.state(tab_id)
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
            state.screen_width = screen_width.max(1);
            state.screen_height = screen_height.max(1);
            state.screen_scale = scale.max(1.0);
            state.screen_origin_x = origin_x;
            state.screen_origin_y = origin_y;
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

    pub fn get_image(&self, tab_id: &str) -> Option<Arc<RenderImage>> {
        self.browsers.get(tab_id).and_then(|entry| {
            entry
                .state
                .lock()
                .ok()
                .and_then(|state| state.image.clone())
        })
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

    pub fn get_favicon_image(&self, tab_id: &str) -> Option<Arc<RenderImage>> {
        self.browsers.get(tab_id).and_then(|entry| {
            entry
                .state
                .lock()
                .ok()
                .and_then(|state| state.favicon.clone())
        })
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

    pub fn resize_browser(&self, tab_id: &str, width: i32, height: i32) {
        if let Some(entry) = self.browsers.get(tab_id) {
            let mut state = entry.state.lock().unwrap();
            let w = width.max(1);
            let h = height.max(1);
            if state.width != w || state.height != h {
                state.width = w;
                state.height = h;
                drop(state);
                if let Some(host) = entry.browser.host() {
                    host.was_resized();
                }
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
    ) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent { x, y, modifiers: 0 };
                host.send_mouse_click_event(Some(&event), button, mouse_up as i32, click_count);
            }
        }
    }

    pub fn send_mouse_move(&self, tab_id: &str, x: i32, y: i32, mouse_leave: bool) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent { x, y, modifiers: 0 };
                host.send_mouse_move_event(Some(&event), mouse_leave as i32);
            }
        }
    }

    pub fn send_mouse_wheel(&self, tab_id: &str, x: i32, y: i32, delta_x: i32, delta_y: i32) {
        if let Some(entry) = self.browsers.get(tab_id) {
            if let Some(host) = entry.browser.host() {
                let event = MouseEvent { x, y, modifiers: 0 };
                host.send_mouse_wheel_event(Some(&event), delta_x, delta_y);
            }
        }
    }

    pub fn navigate(&mut self, id: &str, url: &str) {
        if let Some(entry) = self.browsers.get(id) {
            if let Some(frame) = entry.browser.main_frame() {
                frame.load_url(Some(&CefString::from(url)));
            }
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
            .unwrap_or(0);
        let base = cef_key_event(key, &event.keystroke.modifiers, KeyEventType::RAWKEYDOWN, 0);
        host.send_key_event(Some(&base));
        if character != 0 {
            let text = cef_key_event(
                key,
                &event.keystroke.modifiers,
                KeyEventType::CHAR,
                character,
            );
            host.send_key_event(Some(&text));
        }
    }

    pub fn send_key_up(&self, id: &str, event: &KeyUpEvent) {
        if let Some(host) = self.browsers.get(id).and_then(|entry| entry.browser.host()) {
            let key = &event.keystroke.key;
            let key_event = cef_key_event(key, &event.keystroke.modifiers, KeyEventType::KEYUP, 0);
            host.send_key_event(Some(&key_event));
        }
    }
}

fn cef_key_event(
    key: &str,
    modifiers: &gpui_kit::Modifiers,
    event_type: KeyEventType,
    character: u16,
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
    let code = match key.to_ascii_lowercase().as_str() {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0d,
        "escape" => 0x1b,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2d,
        "delete" => 0x2e,
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as i32,
        _ => 0,
    };
    KeyEvent {
        size: std::mem::size_of::<KeyEvent>(),
        type_: event_type,
        modifiers: flags,
        windows_key_code: code,
        native_key_code: native_key_code(key, code),
        is_system_key: 0,
        character,
        unmodified_character: character,
        focus_on_editable_field: 0,
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
        for entry in self.browsers.values() {
            if let Some(host) = entry.browser.host() {
                host.close_browser(1);
            }
        }
        if self.initialized {
            cef::shutdown();
        }
    }
}
