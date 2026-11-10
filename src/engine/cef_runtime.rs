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
            windowless_frame_rate: 60,
            close_state: Arc::new((Mutex::new(0), Condvar::new())),
            shutdown_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shield: crate::shield::ShieldState::start(),
            https_state: Arc::new(Mutex::new(HttpsFirstState::default())),
            panel_packages: crate::plugin::new_panel_registry(),
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
        let settings = Settings {
            no_sandbox: 1,
            external_message_pump: 0,
            multi_threaded_message_loop: 1,
            windowless_rendering_enabled: 1,
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
                if let Ok(guard) = state.lock()
                    && let Some(zoom) = guard.target_zoom_level
                    && let Some(host) = browser.host()
                {
                    host.set_zoom_level(zoom.clamp(-5.0, 5.0));
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
