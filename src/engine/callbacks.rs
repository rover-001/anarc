use super::*;

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
                    && let Ok(state) = self.shared.lock()
                        && let Some(zoom) = state.target_zoom_level {
                            host.set_zoom_level(zoom.clamp(-5.0, 5.0));
                        }
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
        events: Arc<Mutex<EngineEvents>>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
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
    pub(super) struct LumenDisplayHandler {
        shared: Arc<Mutex<BrowserSharedState>>,
        tab_id: String,
        ui_update_wake: async_channel::Sender<()>,
        events: Arc<Mutex<EngineEvents>>,
    }

    impl DisplayHandler {
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
                if let Some(url) = state.url.as_deref() {
                    crate::history::update_title(url, &title);
                }
                state.dirty = true;
                state.root_dirty = true;
                drop(state);
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
        frame_rate: i32,
        initial_zoom_level: f64,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return; };
            if let Some(host) = browser.host() {
                host.set_windowless_frame_rate(self.frame_rate);
                host.set_zoom_level(self.initial_zoom_level.clamp(-5.0, 5.0));
                if std::env::var_os("ANARC_STATS").is_some_and(|value| value == "1") {
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
            if should_block_popup(_user_gesture, self.shield.enabled()) {
                return 1;
            }
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

#[cfg(test)]
mod popup_tests {
    use super::should_block_popup;

    #[test]
    fn enabled_ad_blocker_rejects_automatic_popups_but_preserves_user_requested_ones() {
        assert!(should_block_popup(0, true));
        assert!(!should_block_popup(1, true));
        assert!(!should_block_popup(0, false));
    }
}

cef::wrap_download_handler! {
    pub(super) struct LumenDownloadHandler {
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
                events.plugin_events.push((
                    "download.before".to_string(),
                    format!("{}\n{}", record.path.display(), record.url),
                ));
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
            if let Ok(mut events) = self.events.lock()
                && let Some(record) = events.downloads.iter_mut().find(|record| record.id == item.id()) {
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
            _permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            if let Ok(mut events) = self.events.lock() {
                events.plugin_events.push((
                    "permission.request".to_string(),
                    format!("{}\npermission", _origin.map(ToString::to_string).unwrap_or_default()),
                ));
            }
            if let Some(callback) = callback { callback.cont(PermissionRequestResult::DENY); }
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
