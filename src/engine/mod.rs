//! Thin CEF boundary. The first implementation keeps one native child browser
//! per tab and shares the default request context; space isolation comes later.

use std::collections::HashMap;

use cef::args::Args;
use cef::*;

cef::wrap_app! {
    struct LumenCefApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(command_line) = command_line {
                let name = CefString::from("renderer-process-limit");
                let value = CefString::from("4");
                command_line.append_switch_with_value(Some(&name), Some(&value));
                for name in [
                    "enable-low-end-device-mode", "disable-background-networking",
                    "disable-component-update", "disable-sync", "disable-breakpad",
                    "disable-extensions", "no-pings", "no-first-run",
                    "disable-gpu-shader-disk-cache",
                ] {
                    let name = CefString::from(name);
                    command_line.append_switch(Some(&name));
                }
                let name = CefString::from("autoplay-policy");
                let value = CefString::from("user-gesture-required");
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

cef::wrap_client! {
    struct LumenClient;

    impl Client {}
}

pub struct Engine {
    browsers: HashMap<String, Browser>,
    initialized: bool,
    args: Args,
    app: Option<cef::App>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            browsers: HashMap::new(),
            initialized: false,
            args: Args::new(),
            app: None,
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
        settings.windowless_rendering_enabled = 0;
        settings.locale = CefString::from("en-US");
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

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    #[cfg(target_os = "linux")]
    pub fn ensure_browser(
        &mut self,
        id: &str,
        url: &str,
        parent: u64,
        bounds: [i32; 4],
    ) -> Result<(), String> {
        if !self.initialized {
            return Ok(());
        }
        if !self.browsers.contains_key(id) {
            let rect = Rect {
                x: bounds[0],
                y: bounds[1],
                width: bounds[2].max(1),
                height: bounds[3].max(1),
            };
            let window_info = WindowInfo::default().set_as_child(parent as _, &rect);
            let mut client = LumenClient::new();
            let start_url = CefString::from(url);
            let browser = cef::browser_host_create_browser_sync(
                Some(&window_info),
                Some(&mut client),
                Some(&start_url),
                Some(&BrowserSettings::default()),
                None,
                None,
            )
            .ok_or_else(|| "CEF failed to create a native browser window".to_string())?;
            self.browsers.insert(id.to_owned(), browser);
        }
        let browser = self.browsers.get(id).expect("browser just created");
        let Some(host) = browser.host() else {
            return Err("CEF browser has no host".into());
        };
        let handle = host.window_handle() as x11::xlib::Window;
        unsafe {
            let display = cef::get_xdisplay().cast::<x11::xlib::Display>();
            if !display.is_null() {
                x11::xlib::XMoveResizeWindow(
                    display,
                    handle,
                    bounds[0],
                    bounds[1],
                    bounds[2].max(1) as u32,
                    bounds[3].max(1) as u32,
                );
                x11::xlib::XMapWindow(display, handle);
                x11::xlib::XSync(display, 0);
            }
        }
        host.was_hidden(0);
        host.was_resized();
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn hide_except(&mut self, active_id: &str) {
        for (id, browser) in &self.browsers {
            let Some(host) = browser.host() else { continue };
            if id == active_id {
                host.was_hidden(0);
            } else {
                host.was_hidden(1);
                unsafe {
                    let display = cef::get_xdisplay().cast::<x11::xlib::Display>();
                    if !display.is_null() {
                        x11::xlib::XUnmapWindow(display, host.window_handle() as _);
                    }
                }
            }
        }
    }

    pub fn navigate(&mut self, id: &str, url: &str) {
        if let Some(browser) = self.browsers.get(id)
            && let Some(frame) = browser.main_frame()
        {
            frame.load_url(Some(&CefString::from(url)));
        }
    }
}

/// Run CEF's subprocess entry point before GPUI starts, as required for renderer
/// and GPU child processes. `None` means this is the browser process.
pub fn execute_subprocess() -> Option<i32> {
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
        for browser in self.browsers.values() {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
        if self.initialized {
            cef::shutdown();
        }
    }
}
