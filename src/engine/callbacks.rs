use super::*;

const MAX_RECYCLED_FRAME_BYTES: usize = 32 * 1024 * 1024;
static RECYCLED_FRAME_PIXELS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

fn recycle_frame_pixels(pixels: Vec<u8>) {
    let Ok(mut pool) = RECYCLED_FRAME_PIXELS.lock() else {
        return;
    };
    let retained_bytes: usize = pool.iter().map(Vec::capacity).sum();
    if pool.len() < 3 && pixels.capacity() <= MAX_RECYCLED_FRAME_BYTES - retained_bytes {
        pool.push(pixels);
    }
}

fn copy_frame_pixels(source: &[u8]) -> Vec<u8> {
    let recycled = RECYCLED_FRAME_PIXELS.lock().ok().and_then(|mut pool| {
        if let Some(index) = pool.iter().position(|pixels| pixels.len() == source.len()) {
            Some(pool.swap_remove(index))
        } else {
            pool.pop()
        }
    });
    if let Some(mut pixels) = recycled {
        pixels.clear();
        pixels.extend_from_slice(source);
        pixels
    } else {
        source.to_vec()
    }
}

fn should_block_popup(user_gesture: i32, ad_blocker_enabled: bool) -> bool {
    ad_blocker_enabled && user_gesture == 0
}

cef::wrap_load_handler! {
    pub(super) struct LumenLoadHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        https_state: Arc<Mutex<HttpsFirstState>>,
        events: Arc<Mutex<EngineEvents>>,
        tab_id: String,
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

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            let Some(frame) = frame else { return; };
            inject_clipboard_script(frame);
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: i32,
        ) {
            let Some(frame) = frame else { return; };
            if frame.is_main() == 0 { return; }
            if let Some(browser) = browser
                && let Some(host) = browser.host()
                    && let Ok(mut state) = self.shared.lock() {
                            state.last_selected_text = None;
                            if let Some(zoom) = state.target_zoom_level {
                                host.set_zoom_level(zoom.clamp(-5.0, 5.0));
                            }
                            host.invalidate(PaintElementType::VIEW);
                        }
            inject_clipboard_script(frame);
            let url = CefStringUtf16::from(&frame.url()).to_string();
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "page.load_end".to_string(),
                    format!("{}\n{url}", self.tab_id),
                ));
            }
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

fn inject_clipboard_script(frame: &Frame) {
    let script = r#"
        (function() {
            if (window.__anarc_cb_init) return;
            window.__anarc_cb_init = true;
            try {
                const style = document.createElement('style');
                style.id = '__anarc_overscroll_fix';
                style.textContent = 'html, body { overscroll-behavior: none !important; }';
                (document.head || document.documentElement).appendChild(style);
            } catch (_) {}
            try {
                function focusedElementIsEditable() {
                    let el = document.activeElement;
                    while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                        el = el.shadowRoot.activeElement;
                    }
                    return !!el && (
                        el.isContentEditable ||
                        (el.matches && el.matches('input, textarea, select'))
                    );
                }
                function reportEditableFocus() {
                    console.log('[ANARC_CB_EDITABLE]' + (focusedElementIsEditable() ? '1' : '0'));
                }
                window.addEventListener('focusin', reportEditableFocus, true);
                window.addEventListener('focusout', function() {
                    setTimeout(reportEditableFocus, 0);
                }, true);
                reportEditableFocus();
            } catch (_) {}
            try {
                function getSelectedText() {
                    try {
                        let text = window.getSelection ? window.getSelection().toString() : '';
                        if (text) return text;
                        let el = document.activeElement;
                        while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                            el = el.shadowRoot.activeElement;
                        }
                        if (el) {
                            if ('selectionStart' in el && 'selectionEnd' in el && typeof el.value === 'string') {
                                const start = Math.min(el.selectionStart, el.selectionEnd);
                                const end = Math.max(el.selectionStart, el.selectionEnd);
                                if (end > start) return el.value.substring(start, end);
                            }
                            if (el.contentDocument && el.contentDocument.getSelection) {
                                const ifr = el.contentDocument.getSelection().toString();
                                if (ifr) return ifr;
                            }
                        }
                        return '';
                    } catch (_) {
                        return '';
                    }
                }

                if (navigator.clipboard && navigator.clipboard.writeText) {
                    const origWriteText = navigator.clipboard.writeText.bind(navigator.clipboard);
                    navigator.clipboard.writeText = async function(text) {
                        try {
                            if (typeof text === 'string' && text.length > 0) {
                                console.log("[ANARC_CB_COPY]" + encodeURIComponent(text));
                            }
                        } catch (_) {}
                        try {
                            return await origWriteText(text);
                        } catch (err) {
                            return Promise.resolve();
                        }
                    };
                }

                function onCopyHandler(e) {
                    let text = '';
                    if (e && e.clipboardData) {
                        try {
                            text = e.clipboardData.getData('text/plain');
                        } catch (_) {}
                    }
                    if (!text) {
                        text = getSelectedText();
                    }
                    if (text) {
                        console.log("[ANARC_CB_COPY]" + encodeURIComponent(text));
                    }
                    setTimeout(function() {
                        try {
                            let late = getSelectedText();
                            if (late && late !== text) {
                                console.log("[ANARC_CB_COPY]" + encodeURIComponent(late));
                            }
                        } catch (_) {}
                    }, 0);
                }

                window.addEventListener('copy', onCopyHandler, true);
                document.addEventListener('copy', onCopyHandler, true);

                function onMouseUpHandler() {
                    setTimeout(function() {
                        try {
                            let text = getSelectedText();
                            if (text && text.trim().length > 0) {
                                console.log("[ANARC_CB_PRIMARY]" + encodeURIComponent(text));
                            }
                        } catch (_) {}
                    }, 10);
                }

                window.addEventListener('mouseup', onMouseUpHandler, true);
                document.addEventListener('mouseup', onMouseUpHandler, true);
            } catch (_) {}
        })();
    "#;
    frame.execute_java_script(Some(&CefString::from(script)), None, 1);
}

#[derive(Default)]
pub(super) struct HttpsFirstState {
    upgraded_hosts: std::collections::HashSet<String>,
    pub(super) fallback_hosts: std::collections::HashSet<String>,
}

pub(super) fn remember_https_upgrade(state: &mut HttpsFirstState, https_url: &str) {
    if let Some(host) = crate::shield::url_host(https_url) {
        state.upgraded_hosts.insert(host);
    }
}

fn clear_https_upgrade(state: &mut HttpsFirstState, https_url: &str) {
    if let Some(host) = crate::shield::url_host(https_url) {
        state.upgraded_hosts.remove(&host);
    }
}

pub(super) fn fallback_http_after_upgrade_failure(
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
    pub(super) struct LumenResourceRequestHandler {
        shield: Arc<crate::shield::ShieldState>,
        tab_id: String,
        panel_packages: super::panel_scheme::PanelRegistry,
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
            if source.starts_with("lumen-plugin://")
                && !super::panel_scheme::allowed_panel_network_request(
                    &source,
                    &url,
                    &self.panel_packages,
                )
            {
                return ReturnValue::CANCEL;
            }
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
    pub(super) struct LumenRequestHandler {
        shield: Arc<crate::shield::ShieldState>,
        tab_id: String,
        https_state: Arc<Mutex<HttpsFirstState>>,
        events: Arc<Mutex<EngineEvents>>,
        panel_packages: super::panel_scheme::PanelRegistry,
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
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "navigation.before".to_string(),
                    format!("{}\n{url}", self.tab_id),
                ));
            }
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
            Some(LumenResourceRequestHandler::new(
                self.shield.clone(),
                self.tab_id.clone(),
                self.panel_packages.clone(),
            ))
        }
    }
}

cef::wrap_context_menu_handler! {
    pub(super) struct LumenContextMenuHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        events: Arc<Mutex<EngineEvents>>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(params) = params {
                let selection = CefStringUtf16::from(&params.selection_text()).to_string();
                if let Ok(mut state) = self.shared.lock() {
                    state.last_selected_text = if selection.is_empty() {
                        None
                    } else {
                        Some(selection.clone())
                    };
                }
                if !selection.is_empty() {
                    crate::engine::clipboard::write_system_clipboard(&selection);
                    crate::engine::clipboard::write_primary_clipboard(&selection);
                }
            }
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "context_menu.build".to_string(),
                    "page".to_string(),
                ));
            }
            // CEF's OSR menu paints use PET_POPUP, which this client intentionally ignores.
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

cef::wrap_download_image_callback! {
    pub(super) struct LumenFaviconCallback {
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

            // Decode every available scale and keep the largest image so
            // favicons render at the highest resolution the site provides.
            let mut decoded_buffer: Option<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> = None;
            let mut decoded_pixels = 0u64;
            for scale in [2.0f32, 1.0, 0.0] {
                if let Some(png) = favicon_image.as_png(scale, 1, None, None) {
                    let size = png.size();
                    if size > 0 && size <= 512 * 1024 {
                        let mut bytes = vec![0u8; size];
                        let copied = png.data(Some(&mut bytes), 0).min(size);
                        bytes.truncate(copied);
                        if let Ok(decoded) = image::load_from_memory(&bytes) {
                            let rgba = decoded.to_rgba8();
                            let pixels = u64::from(rgba.width()) * u64::from(rgba.height());
                            if pixels > decoded_pixels {
                                decoded_pixels = pixels;
                                decoded_buffer = Some(rgba);
                            }
                        }
                    }
                }

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
                            let pixels = u64::from(img_buffer.width()) * u64::from(img_buffer.height());
                            if pixels > decoded_pixels {
                                decoded_pixels = pixels;
                                decoded_buffer = Some(img_buffer);
                            }
                        }
                    }
                }
            }

            let Some(img_buffer) = decoded_buffer else {
                favicon_debug(format_args!("failed to decode favicon for tab={}", self.tab_id));
                return;
            };

            let mut display_buffer = img_buffer.clone();
            rgba_to_bgra_in_place(&mut display_buffer);
            let frame = image::Frame::new(display_buffer);
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
                    (Some(curr), Some(target)) => same_favicon_host(curr, target),
                    _ => true,
                };

                let host_mismatch = match (&state.favicon_host, &target_host) {
                    (Some(current), Some(target)) => !same_favicon_host(current, target),
                    _ => false,
                };

                // A network result is fresher than the on-disk cache, so it may
                // replace it. Only a page-declared (preferred) icon is protected
                // from being downgraded by the origin fallback.
                if should_store_favicon(
                    matches_current,
                    self.preferred,
                    state.favicon_preferred,
                    host_mismatch,
                ) {
                    state.favicon = Some(render_image);
                    state.favicon_host = target_host;
                    state.favicon_preferred = self.preferred;
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
    pub(super) struct LumenRenderHandler {
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
            let slice = unsafe { std::slice::from_raw_parts(buffer, len) };
            let pixels = copy_frame_pixels(slice);
            if let Some(img_buffer) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                width as u32,
                height as u32,
                pixels,
            ) {
                let frame = image::Frame::new(img_buffer);
                let render_image =
                    Arc::new(RenderImage::with_recycled_pixels(frame, recycle_frame_pixels));
                let mut state = self.shared.lock().unwrap();
                // OSR buffers are physical pixels; pane dimensions are DIPs.
                // Compare consecutive buffers so HiDPI does not resample every paint.
                let frame_size_changed = state.pending_image.as_ref().or(state.image.as_ref()).is_none_or(|image| {
                    let size = image.size(0);
                    u32::from(size.width) != width as u32
                        || u32::from(size.height) != height as u32
                });
                let should_calc_edge = state.edge_color.is_none()
                    || frame_size_changed
                    || state.last_edge_color_instant.is_none_or(|t| {
                        std::time::Instant::now().duration_since(t) >= std::time::Duration::from_millis(1000)
                    });
                if should_calc_edge {
                    state.edge_color = median_edge_color_bgra(slice, width as usize, height as usize);
                    state.last_edge_color_instant = Some(std::time::Instant::now());
                }
                state.pending_image = Some(render_image);
                state.dirty = true;
            }
            let _ = self.paint_wake.try_send(());
        }
    }
}

cef::wrap_display_handler! {
    pub(super) struct LumenDisplayHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        tab_id: String,
        ui_update_wake: async_channel::Sender<()>,
        events: Arc<Mutex<EngineEvents>>,
    }

    impl DisplayHandler {
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            if let Some(msg) = message {
                let text = msg.to_string();
                    if let Some(editable) = text.strip_prefix("[ANARC_CB_EDITABLE]") {
                        if let Ok(mut state) = self.shared.lock() {
                            state.focus_on_editable_field = editable == "1";
                        }
                        return 1;
                    }
                if let Some(encoded) = text.strip_prefix("[ANARC_CB_COPY]") {
                    let decoded = crate::engine::clipboard::url_decode(encoded);
                    crate::engine::clipboard::write_system_clipboard(&decoded);
                    return 1;
                }
                if let Some(encoded) = text.strip_prefix("[ANARC_CB_PRIMARY]") {
                    let decoded = crate::engine::clipboard::url_decode(encoded);
                    crate::engine::clipboard::write_primary_clipboard(&decoded);
                    return 1;
                }
            }
            0
        }

        fn on_fullscreen_mode_change(&self, _browser: Option<&mut Browser>, fullscreen: i32) {
            if let Ok(mut events) = self.events.lock() {
                events.fullscreen.push((self.tab_id.clone(), fullscreen != 0));
            }
            let _ = self.ui_update_wake.try_send(());
        }

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
            if let Ok(mut events) = self.events.lock() {
                events
                    .plugin_events
                    .push(("page.favicon".to_string(), self.tab_id.clone()));
            }
            favicon_debug(format_args!("URL found tab={} url={url}", self.tab_id));
            if url.starts_with("data:image/")
                && let Some(comma) = url.find(',') {
                    let data_part = &url[comma + 1..];
                    if let Some(bytes) = decode_base64(data_part)
                        && let Ok(decoded) = image::load_from_memory(&bytes) {
                            let rgba = decoded.to_rgba8();
                            let target_host = page_host.clone().or_else(|| crate::shield::url_host(&page_url));
                            if let Some(ref host) = target_host {
                                save_favicon_to_disk(host, &rgba);
                            }
                            let mut display_buffer = rgba;
                            rgba_to_bgra_in_place(&mut display_buffer);
                            let frame = image::Frame::new(display_buffer);
                            let render_image = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
                            if let Ok(mut state) = self.shared.lock() {
                                state.favicon = Some(render_image);
                                state.favicon_host = target_host;
                                state.favicon_preferred = true;
                                state.dirty = true;
                                state.root_dirty = true;
                                let _ = self.ui_update_wake.try_send(());
                            }
                            return;
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
                256,
                0,
                Some(&mut callback),
            );
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(title) = title {
                let title = title.to_string();
                let mut state = self.shared.lock().unwrap();
                state.title = Some(title.clone());
                let url = state.url.clone();
                state.dirty = true;
                state.root_dirty = true;
                drop(state);
                if let Some(url) = url {
                    crate::history::update_title(&url, &title);
                }
                if let Ok(mut events) = self.events.lock() {
                    events
                        .plugin_events
                        .push(("page.title".to_string(), self.tab_id.clone()));
                }
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
            if let Ok(mut state) = self.shared.lock()
                && state.cursor != cursor
            {
                state.cursor = cursor;
                state.dirty = true;
                drop(state);
                let _ = self.ui_update_wake.try_send(());
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
                if let Ok(mut events) = self.events.lock() {
                    events
                        .plugin_events
                        .push(("page.address".to_string(), self.tab_id.clone()));
                }
                let new_host = crate::shield::url_host(&page_url);
                if let Ok(mut state) = self.shared.lock() {
                    state.url = Some(page_url.clone());
                    state.current_url = Some(page_url.clone());
                    state.title = None;
                    let host_changed = match (&state.current_host, &new_host) {
                        (Some(curr), Some(new_h)) => !same_favicon_host(curr, new_h),
                        (None, Some(_)) => true,
                        (Some(_), None) => true,
                        (None, None) => false,
                    };
                    if host_changed {
                        state.current_host = new_host.clone();
                        state.favicon = None;
                        state.favicon_host = None;
                        state.favicon_preferred = false;
                        if let Some(ref host) = new_host
                            && let Some(cached) = load_favicon_from_disk(host) {
                                state.favicon = Some(cached);
                                state.favicon_host = Some(host.clone());
                            }
                    } else if state.favicon.is_none()
                        && let Some(ref host) = new_host
                            && let Some(cached) = load_favicon_from_disk(host) {
                                state.favicon = Some(cached);
                                state.favicon_host = Some(host.clone());
                                state.favicon_preferred = false;
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
                        256,
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
    pub(super) struct LumenLifeSpanHandler {
        events: Arc<Mutex<EngineEvents>>,
        space_id: String,
        tab_id: String,
        paint_wake: async_channel::Sender<()>,
        close_state: Arc<(Mutex<usize>, Condvar)>,
        shutdown_requested: Arc<std::sync::atomic::AtomicBool>,
        shield: Arc<crate::shield::ShieldState>,
        https_state: Arc<Mutex<HttpsFirstState>>,
        panel_packages: panel_scheme::PanelRegistry,
        frame_rate: i32,
        initial_zoom_level: f64,
        popup_serial: Arc<std::sync::atomic::AtomicU64>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return; };
            if let Some(host) = browser.host() {
                host.set_windowless_frame_rate(self.frame_rate);
                host.set_zoom_level(self.initial_zoom_level.clamp(-5.0, 5.0));
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

        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            if let Ok(mut events) = self.events.lock() {
                events.close_requests.push(self.tab_id.clone());
                let _ = self.paint_wake.try_send(());
            }
            0
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: i32,
            popup_features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            if should_block_popup(user_gesture, self.shield.enabled()) {
                return 1;
            }
            let url = target_url.map(ToString::to_string).unwrap_or_default();
            let trimmed = url.trim();
            // Some sign-in libraries open a blank window first and navigate it
            // from script afterwards, so a gesture-opened blank popup still has
            // to exist. Everything else blank is noise.
            let blank = trimmed.is_empty() || trimmed == "about:blank";
            let wants_native_window = user_gesture == 1
                && (target_disposition == WindowOpenDisposition::NEW_POPUP
                    || is_auth_flow_url(trimmed));
            if blank && !wants_native_window {
                return 1;
            }
            if !blank
                && self.shield.enabled()
                && self.shield.should_block(trimmed, trimmed, "document", "GET", &self.tab_id)
            {
                return 1;
            }
            // Sign-in popups have to keep a live `window.opener`: identity
            // providers hand the result back through `postMessage` and finish
            // with `window.close()`. Letting CEF own those windows keeps that
            // contract, and the app only has to show the browser it created.
            let popup_url = if blank { "about:blank" } else { trimmed };
            if wants_native_window
                && start_popup_browser(
                    self,
                    popup_url,
                    popup_features,
                    window_info,
                    client,
                    settings,
                )
            {
                return 0;
            }
            if blank {
                return 1;
            }
            if let Ok(mut events) = self.events.lock()
                && !events
                    .popups
                    .iter()
                    .any(|popup| popup.space_id == self.space_id && popup.url == trimmed)
            {
                events.popups.push(PopupRequest {
                    space_id: self.space_id.clone(),
                    url: trimmed.to_owned(),
                    browser_id: None,
                });
                let _ = self.paint_wake.try_send(());
            }
            1
        }
    }
}

/// Lets CEF create the popup itself, so the new browser keeps its opener.
/// Returns false when the platform passed nothing to configure, in which case
/// the caller falls back to an app-created peek window.
fn start_popup_browser(
    handler: &LumenLifeSpanHandler,
    url: &str,
    popup_features: Option<&PopupFeatures>,
    window_info: Option<&mut WindowInfo>,
    client: Option<&mut Option<Client>>,
    settings: Option<&mut BrowserSettings>,
) -> bool {
    let (Some(window_info), Some(client), Some(settings)) = (window_info, client, settings) else {
        return false;
    };
    let width = popup_features
        .filter(|features| features.width_set != 0 && features.width > 0)
        .map_or(800, |features| features.width);
    let height = popup_features
        .filter(|features| features.height_set != 0 && features.height > 0)
        .map_or(700, |features| features.height);
    let serial = handler
        .popup_serial
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tab_id = format!("{}-popup-{serial}", handler.space_id);
    let shared = Arc::new(Mutex::new(new_shared_state(width, height, url, 1.0)));
    let render_handler =
        LumenRenderHandler::new(shared.clone(), handler.paint_wake.clone(), tab_id.clone());
    let display_handler = LumenDisplayHandler::new(
        shared.clone(),
        tab_id.clone(),
        handler.paint_wake.clone(),
        handler.events.clone(),
    );
    let life_span_handler = LumenLifeSpanHandler::new(
        handler.events.clone(),
        handler.space_id.clone(),
        tab_id.clone(),
        handler.paint_wake.clone(),
        handler.close_state.clone(),
        handler.shutdown_requested.clone(),
        handler.shield.clone(),
        handler.https_state.clone(),
        handler.panel_packages.clone(),
        handler.frame_rate,
        handler.initial_zoom_level,
        handler.popup_serial.clone(),
    );
    let download_handler =
        LumenDownloadHandler::new(handler.events.clone(), handler.paint_wake.clone());
    let permission_handler = LumenPermissionHandler::new(handler.events.clone());
    let load_handler = LumenLoadHandler::new(
        shared.clone(),
        handler.https_state.clone(),
        handler.events.clone(),
        tab_id.clone(),
    );
    let context_menu_handler = LumenContextMenuHandler::new(shared.clone(), handler.events.clone());
    let request_handler = LumenRequestHandler::new(
        handler.shield.clone(),
        tab_id.clone(),
        handler.https_state.clone(),
        handler.events.clone(),
        handler.panel_packages.clone(),
    );
    let dialog_handler = LumenDialogHandler::new(handler.paint_wake.clone());
    *client = Some(LumenClient::new(
        render_handler,
        display_handler,
        life_span_handler,
        download_handler,
        permission_handler,
        load_handler,
        context_menu_handler,
        request_handler,
        dialog_handler,
    ));
    *window_info = WindowInfo::default().set_as_windowless(0);
    // Keep popup browsers on CEF's own frame source too; see ensure_browser.
    settings.windowless_frame_rate = handler.frame_rate;
    settings.javascript_access_clipboard = State::ENABLED;
    settings.javascript_dom_paste = State::ENABLED;
    if let Ok(mut count) = handler.close_state.0.lock() {
        *count += 1;
    }
    if let Ok(mut events) = handler.events.lock() {
        events.pending_popup_states.push((tab_id.clone(), shared));
        events.popups.push(PopupRequest {
            space_id: handler.space_id.clone(),
            url: url.to_owned(),
            browser_id: Some(tab_id),
        });
    }
    let _ = handler.paint_wake.try_send(());
    true
}

#[cfg(test)]
mod popup_tests {
    use super::should_block_popup;

    #[test]
    fn enabled_ad_blocker_rejects_automatic_popups_but_preserves_user_requested_ones() {
        assert!(should_block_popup(0, true));
        assert!(!should_block_popup(1, true));
        assert!(!should_block_popup(0, false));
    }

    #[test]
    fn empty_and_blank_popups_are_rejected() {
        for url in ["", "   ", "about:blank", "  about:blank  "] {
            let trimmed = url.trim();
            assert!(trimmed.is_empty() || trimmed == "about:blank");
        }
    }
}

#[cfg(test)]
mod dialog_tests {
    use super::*;

    #[test]
    fn filter_patterns_clean_and_deduplicate_extensions() {
        let filters = vec!["image/*".to_string(), "custom.pdf".to_string()];
        let extensions = vec![
            ".png;.jpg".to_string(),
            "jpeg".to_string(),
            "*.png".to_string(),
        ];
        let patterns = collect_filter_patterns(&filters, &extensions);
        assert_eq!(patterns, vec!["*.jpeg", "*.jpg", "*.pdf", "*.png"]);
    }

    #[test]
    fn build_zenity_args_handles_open_modes_and_filters() {
        let open_args = build_zenity_args(
            FileDialogMode::OPEN,
            Some("Upload file"),
            Some("/tmp/test.txt"),
            &[],
            &["txt".to_string()],
        );
        assert!(open_args.contains(&"--file-selection".to_string()));
        assert!(open_args.contains(&"--title=Upload file".to_string()));
        assert!(open_args.contains(&"--filename=/tmp/test.txt".to_string()));
        assert!(open_args.contains(&"--file-filter=Supported files | *.txt".to_string()));
        assert!(open_args.contains(&"--file-filter=All files | *".to_string()));
        assert!(!open_args.contains(&"--multiple".to_string()));
        assert!(!open_args.contains(&"--directory".to_string()));
    }

    #[test]
    fn build_zenity_args_handles_open_multiple_and_folder() {
        let multi_args = build_zenity_args(FileDialogMode::OPEN_MULTIPLE, None, None, &[], &[]);
        assert!(multi_args.contains(&"--multiple".to_string()));
        assert!(multi_args.contains(&"--separator=\n".to_string()));
        assert!(multi_args.contains(&"--title=Select Files".to_string()));

        let folder_args = build_zenity_args(FileDialogMode::OPEN_FOLDER, None, None, &[], &[]);
        assert!(folder_args.contains(&"--directory".to_string()));
        assert!(folder_args.contains(&"--title=Select Folder".to_string()));

        let save_args = build_zenity_args(FileDialogMode::SAVE, None, None, &[], &[]);
        assert!(save_args.contains(&"--save".to_string()));
        assert!(save_args.contains(&"--confirm-overwrite".to_string()));
        assert!(save_args.contains(&"--title=Save File".to_string()));
    }

    #[test]
    fn extract_cef_string_list_on_none_returns_empty() {
        let result = extract_cef_string_list(None);
        assert!(result.is_empty());
    }
}

cef::wrap_download_handler! {
    pub(super) struct LumenDownloadHandler {
        events: Arc<Mutex<EngineEvents>>,
        ui_update_wake: async_channel::Sender<()>,
    }

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> i32 {
            1
        }

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
                events.plugin_events.push((
                    "download.before".to_string(),
                    format!("{}\n{}", record.path.display(), record.url),
                ));
                events.downloads.push(record.clone());
            }
            drop(events);
            callback.cont(Some(&cef_path), 0);
            let _ = self.ui_update_wake.try_send(());
            eprintln!("Lumen download started: {} -> {}", record.url, record.path.display());
            1
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
            if let Ok(mut events) = self.events.lock()
                && let Some(record) = events.downloads.iter_mut().find(|record| record.id == item.id()) {
                    record.received_bytes = received;
                    record.total_bytes = total;
                    record.complete = complete;
                    record.canceled = canceled;
                    let _ = self.ui_update_wake.try_send(());
                    if complete || canceled {
                        eprintln!("Lumen download {}: {} ({}/{})", if complete { "complete" } else { "failed" }, record.path.display(), received, total);
                    } else {
                        eprintln!("Lumen download progress: {} ({}/{})", record.path.display(), received, total);
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
    pub(super) struct LumenPermissionHandler {
        events: Arc<Mutex<EngineEvents>>,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _origin: Option<&CefString>,
            _permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "permission.request".to_string(),
                    format!("{}\nmedia", _origin.map(ToString::to_string).unwrap_or_default()),
                ));
            }
            if let Some(callback) = callback { callback.cont(0); }
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            _origin: Option<&CefString>,
            permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "permission.request".to_string(),
                    format!("{}\npermission", _origin.map(ToString::to_string).unwrap_or_default()),
                ));
            }
            if let Some(callback) = callback {
                const CEF_PERMISSION_TYPE_CLIPBOARD: u32 = 16;
                let result = if (permissions & CEF_PERMISSION_TYPE_CLIPBOARD) != 0 {
                    PermissionRequestResult::ACCEPT
                } else {
                    PermissionRequestResult::DENY
                };
                callback.cont(result);
            }
            1
        }
    }
}

pub(crate) fn collect_filter_patterns(filters: &[String], extensions: &[String]) -> Vec<String> {
    let mut patterns = Vec::new();
    for ext_group in extensions {
        for ext in ext_group.split(';') {
            let clean = ext.trim().trim_start_matches('*').trim_start_matches('.');
            let actual_ext = clean.rsplit('.').next().unwrap_or(clean);
            if !actual_ext.is_empty() {
                patterns.push(format!("*.{actual_ext}"));
            }
        }
    }
    for filter in filters {
        let clean = filter.trim();
        if !clean.contains('/') {
            for ext in clean.split(';') {
                let clean_ext = ext.trim().trim_start_matches('*').trim_start_matches('.');
                let actual_ext = clean_ext.rsplit('.').next().unwrap_or(clean_ext);
                if !actual_ext.is_empty() {
                    patterns.push(format!("*.{actual_ext}"));
                }
            }
        }
    }
    patterns.sort();
    patterns.dedup();
    patterns
}

pub(crate) fn build_zenity_args(
    mode: FileDialogMode,
    title: Option<&str>,
    default_path: Option<&str>,
    filters: &[String],
    extensions: &[String],
) -> Vec<String> {
    let mut args = vec!["--file-selection".to_string()];

    if mode == FileDialogMode::OPEN_MULTIPLE {
        args.push("--multiple".to_string());
        args.push("--separator=\n".to_string());
    } else if mode == FileDialogMode::OPEN_FOLDER {
        args.push("--directory".to_string());
    } else if mode == FileDialogMode::SAVE {
        args.push("--save".to_string());
        args.push("--confirm-overwrite".to_string());
    }

    let dialog_title = title.unwrap_or(if mode == FileDialogMode::SAVE {
        "Save File"
    } else if mode == FileDialogMode::OPEN_FOLDER {
        "Select Folder"
    } else if mode == FileDialogMode::OPEN_MULTIPLE {
        "Select Files"
    } else {
        "Select File"
    });
    args.push(format!("--title={dialog_title}"));

    if let Some(path) = default_path.filter(|p| !p.is_empty()) {
        args.push(format!("--filename={path}"));
    }

    let patterns = collect_filter_patterns(filters, extensions);
    if !patterns.is_empty() {
        args.push(format!(
            "--file-filter=Supported files | {}",
            patterns.join(" ")
        ));
        args.push("--file-filter=All files | *".to_string());
    }

    args
}

fn run_kdialog(
    mode: FileDialogMode,
    title: Option<&str>,
    default_path: Option<&str>,
    filters: &[String],
    extensions: &[String],
) -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new("kdialog");
    let dialog_title = title.unwrap_or(if mode == FileDialogMode::SAVE {
        "Save File"
    } else if mode == FileDialogMode::OPEN_FOLDER {
        "Select Folder"
    } else if mode == FileDialogMode::OPEN_MULTIPLE {
        "Select Files"
    } else {
        "Select File"
    });
    cmd.arg("--title").arg(dialog_title);

    let patterns = collect_filter_patterns(filters, extensions);
    let filter_arg = if patterns.is_empty() {
        String::new()
    } else {
        patterns.join(" ")
    };

    if mode == FileDialogMode::OPEN_FOLDER {
        cmd.arg("--getexistingdirectory");
        if let Some(path) = default_path.filter(|p| !p.is_empty()) {
            cmd.arg(path);
        }
    } else if mode == FileDialogMode::SAVE {
        cmd.arg("--getsavefilename");
        cmd.arg(default_path.unwrap_or(""));
        if !filter_arg.is_empty() {
            cmd.arg(filter_arg);
        }
    } else if mode == FileDialogMode::OPEN_MULTIPLE {
        cmd.arg("--getopenfilename");
        cmd.arg(default_path.unwrap_or(""));
        if !filter_arg.is_empty() {
            cmd.arg(filter_arg);
        }
        cmd.arg("--multiple").arg("--separate-output");
    } else {
        cmd.arg("--getopenfilename");
        cmd.arg(default_path.unwrap_or(""));
        if !filter_arg.is_empty() {
            cmd.arg(filter_arg);
        }
    }

    cmd.output()
}

pub(crate) fn run_file_picker(
    mode: FileDialogMode,
    title: Option<&str>,
    default_path: Option<&str>,
    filters: &[String],
    extensions: &[String],
) -> Option<Vec<String>> {
    let zenity_args = build_zenity_args(mode, title, default_path, filters, extensions);
    match std::process::Command::new("zenity")
        .args(&zenity_args)
        .output()
    {
        Ok(output) if output.status.success() => {
            let out = String::from_utf8_lossy(&output.stdout);
            let paths: Vec<String> = out
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if !paths.is_empty() {
                return Some(paths);
            }
        }
        Ok(_) => return None,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }

    match run_kdialog(mode, title, default_path, filters, extensions) {
        Ok(output) if output.status.success() => {
            let out = String::from_utf8_lossy(&output.stdout);
            let paths: Vec<String> = out
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if !paths.is_empty() {
                return Some(paths);
            }
        }
        Ok(_) => return None,
        Err(_) => {}
    }

    None
}

pub(crate) fn extract_cef_string_list(mut list: Option<&mut CefStringList>) -> Vec<String> {
    let mut items = Vec::new();
    let Some(list) = list.as_mut() else {
        return items;
    };
    let raw: *mut cef::sys::_cef_string_list_t = (&mut **list).into();
    if raw.is_null() {
        return items;
    }
    let count = unsafe { cef::sys::cef_string_list_size(raw) };
    for i in 0..count {
        let mut s: cef::sys::cef_string_t = unsafe { std::mem::zeroed() };
        if unsafe { cef::sys::cef_string_list_value(raw, i, &mut s) } != 0 {
            let text = if s.str_.is_null() || s.length == 0 {
                String::new()
            } else {
                let units = unsafe { std::slice::from_raw_parts(s.str_, s.length) };
                String::from_utf16_lossy(units)
            };
            unsafe { cef::sys::cef_string_utf16_clear(&mut s) };
            if !text.is_empty() {
                items.push(text);
            }
        }
    }
    items
}

cef::wrap_dialog_handler! {
    pub(super) struct LumenDialogHandler {
        ui_update_wake: async_channel::Sender<()>,
    }

    impl DialogHandler {
        fn on_file_dialog(
            &self,
            _browser: Option<&mut Browser>,
            mode: FileDialogMode,
            title: Option<&CefString>,
            default_file_path: Option<&CefString>,
            accept_filters: Option<&mut CefStringList>,
            accept_extensions: Option<&mut CefStringList>,
            _accept_descriptions: Option<&mut CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback else { return 0; };
            let callback = callback.clone();
            let title_str = title.map(ToString::to_string);
            let default_path = default_file_path.map(ToString::to_string);
            let filters = extract_cef_string_list(accept_filters);
            let extensions = extract_cef_string_list(accept_extensions);
            let wake = self.ui_update_wake.clone();

            std::thread::spawn(move || {
                let selected = run_file_picker(
                    mode,
                    title_str.as_deref(),
                    default_path.as_deref(),
                    &filters,
                    &extensions,
                );
                match selected {
                    Some(paths) if !paths.is_empty() => {
                        let mut list = CefStringList::new();
                        for path in paths {
                            list.append(&path);
                        }
                        callback.cont(Some(&mut list));
                    }
                    _ => {
                        callback.cancel();
                    }
                }
                let _ = wake.try_send(());
            });

            1
        }
    }
}

cef::wrap_client! {
    pub(super) struct LumenClient {
        render_handler: RenderHandler,
        display_handler: DisplayHandler,
        life_span_handler: LifeSpanHandler,
        download_handler: DownloadHandler,
        permission_handler: PermissionHandler,
        load_handler: LoadHandler,
        context_menu_handler: ContextMenuHandler,
        request_handler: RequestHandler,
        dialog_handler: DialogHandler,
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
        fn dialog_handler(&self) -> Option<DialogHandler> { Some(self.dialog_handler.clone()) }
    }
}
