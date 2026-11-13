use super::*;

impl Engine {
    pub(crate) fn set_panel_registry(&mut self, registry: crate::plugin::PanelRegistryHandle) {
        self.panel_packages = registry;
    }

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
            windowless_frame_rate: crate::engine::detect_system_refresh_rate()
                .unwrap_or(60)
                .clamp(60, 240),
            close_state: Arc::new((Mutex::new(0), Condvar::new())),
            shutdown_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shield: crate::shield::ShieldState::start(),
            https_state: Arc::new(Mutex::new(HttpsFirstState::default())),
            panel_packages: crate::plugin::new_panel_registry(),
            popup_serial: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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

        let cache_root = crate::paths::cef_root_dir();
        std::fs::create_dir_all(&cache_root)
            .map_err(|error| format!("cannot create CEF cache directory: {error}"))?;

        let current_exe =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("anarc"));
        let exe_dir = current_exe
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Locate CEF runtime resources (resources.pak, locales/)
        // Priority:
        // 1. Next to the current executable (target/debug, target/release, or unified installation)
        // 2. Directory in LD_LIBRARY_PATH (e.g. $PREFIX/anarc-cef)
        // 3. Fallback to executable directory
        let cef_resources_dir = if exe_dir.join("resources.pak").exists() {
            exe_dir.to_path_buf()
        } else if let Ok(ld_path) = std::env::var("LD_LIBRARY_PATH") {
            ld_path
                .split(':')
                .map(std::path::PathBuf::from)
                .find(|p| p.join("resources.pak").exists())
                .unwrap_or_else(|| exe_dir.to_path_buf())
        } else {
            exe_dir.to_path_buf()
        };
        let locales_dir = cef_resources_dir.join("locales");

        let settings = Settings {
            no_sandbox: 1,
            external_message_pump: 0,
            multi_threaded_message_loop: 1,
            windowless_rendering_enabled: 1,
            browser_subprocess_path: CefString::from(current_exe.to_string_lossy().as_ref()),
            resources_dir_path: CefString::from(cef_resources_dir.to_string_lossy().as_ref()),
            locales_dir_path: CefString::from(locales_dir.to_string_lossy().as_ref()),
            locale: CefString::from("en-US"),
            user_agent: CefString::from(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36",
            ),
            root_cache_path: CefString::from(cache_root.to_string_lossy().as_ref()),
            ..Default::default()
        };
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
        let (created, popup_states) = self
            .events
            .lock()
            .ok()
            .map(|mut events| {
                (
                    std::mem::take(&mut events.created),
                    std::mem::take(&mut events.pending_popup_states),
                )
            })
            .unwrap_or_default();
        // Windows the page opened itself are already on their way; registering
        // their state first keeps `ensure_browser` from starting a duplicate.
        for (id, state) in popup_states {
            self.pending_browsers.insert(id, state);
        }
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
                if let Ok(guard) = state.lock()
                    && let Some(zoom) = guard.target_zoom_level
                    && let Some(host) = browser.host()
                {
                    host.set_zoom_level(zoom.clamp(-5.0, 5.0));
                    host.invalidate(cef::PaintElementType::VIEW);
                }
                let (pending_url, initial_url) = state
                    .lock()
                    .ok()
                    .map(|guard| (guard.current_url.clone(), guard.initial_url.clone()))
                    .unwrap_or_default();
                if let Some(ref url) = pending_url
                    && url != "about:blank"
                    && url != "lumen://welcome"
                    && url != &initial_url
                    && let Some(frame) = browser.main_frame()
                {
                    frame.load_url(Some(&CefString::from(url.as_str())));
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

    pub(crate) fn register_panel_package(
        &mut self,
        package: crate::plugin::PanelPackage,
    ) -> Result<(), String> {
        let mut packages = self
            .panel_packages
            .lock()
            .map_err(|_| "panel package registry is unavailable".to_owned())?;
        if packages.contains_key(&package.id) {
            return Err(format!("panel is already registered: {}", package.id));
        }
        packages.insert(package.id.clone(), package);
        Ok(())
    }

    pub(crate) fn unregister_panel_package(&mut self, id: &str) -> bool {
        self.panel_packages
            .lock()
            .ok()
            .is_some_and(|mut packages| packages.remove(id).is_some())
    }
}
