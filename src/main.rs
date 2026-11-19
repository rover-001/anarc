mod app;
mod engine;
mod history;
mod import;
mod lifecycle;
mod model;
mod navigation;
mod paths;
mod persist;
mod plugin;
mod plugins;
mod reader;
mod settings;
mod shield;
mod ui;

use gpui_kit::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};

fn main() {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("ANARC_PTRACE").is_some() {
            unsafe extern "C" {
                fn prctl(option: i32, ...) -> i32;
            }
            unsafe {
                prctl(0x5961_6d61, -1i64, 0i64, 0i64, 0i64);
            }
        }
    }
    if handle_import_cli() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        // Prevent glibc from thrashing mmap/munmap syscalls and page faults on 8.3MB frame buffers
        unsafe extern "C" {
            fn mallopt(param: std::os::raw::c_int, value: std::os::raw::c_int) -> std::os::raw::c_int;
        }
        unsafe {
            mallopt(1 /* M_MMAP_THRESHOLD */, 64 * 1024 * 1024);
            mallopt(-1 /* M_TRIM_THRESHOLD */, 64 * 1024 * 1024);
        }
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_some() {
        // GPUI prefers Wayland when both display variables are set. CEF's
        // windowed Linux child browser needs an X11/XWayland parent instead.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
    }
    if let Some(code) = engine::execute_subprocess() {
        std::process::exit(code);
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(ui::motion::reduced_motion());
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            gpui_kit::component::Theme::update(cx, |theme| {
                theme.caret = gpui_kit::rgb(0xffffff).into();
            });
            let bounds = Bounds::centered(None, size(px(1200.0), px(760.0)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui_kit::TitlebarOptions {
                        title: Some("Anarc".into()),
                        ..Default::default()
                    }),
                    app_id: Some("anarc".into()),
                    // Pages keep animating while the window is in the
                    // background, and the default inactive cap (~30 fps) made
                    // them visibly stutter next to the focused window. Video
                    // and canvas content are the reason windows stay open
                    // beside other apps, so the same frame cadence is kept.
                    inactive_frame_interval: None,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    window.set_window_title("Anarc");
                    cx.new(|cx| app::Lumen::new(window, cx))
                },
            )
            .expect("open Anarc window");
        });
}

fn handle_import_cli() -> bool {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(index) = args.iter().position(|arg| arg == "--import") else {
        return false;
    };
    let Some(browser) = args.get(index + 1) else {
        eprintln!("usage: anarc --import chrome|chromium|brave|edge|all");
        std::process::exit(2);
    };
    if !plugins::import_enabled_from_config() {
        eprintln!("The import plugin is disabled in plugins.toml");
        std::process::exit(2);
    }
    match import::import(browser) {
        Ok(summary) => {
            println!(
                "Import complete: {} profiles, {} bookmarks, {} visits",
                summary.profiles, summary.bookmarks, summary.visits
            );
            println!(
                "Bookmarks: {}",
                paths::data_dir().join("bookmarks.json").display()
            );
            println!(
                "History: {}",
                paths::data_dir().join("history.sqlite").display()
            );
            for warning in summary.warnings {
                eprintln!("Import warning: {warning}");
            }
        }
        Err(error) => {
            eprintln!("Import failed: {error}");
            std::process::exit(2);
        }
    }
    true
}
