mod engine;
mod lifecycle;
mod model;
mod persist;
mod stats;
mod ui;

use std::time::Instant;

use gpui_kit::{
    Bounds, Context, FontWeight, Render, Window, WindowBounds, WindowOptions,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px, rgb, size,
};
use model::{Space, Tab, TabState};
#[cfg(all(target_os = "linux", not(test)))]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use ui::{sidebar, space_switcher, tab_list};

struct Lumen {
    spaces: Vec<Space>,
    active_space: String,
    sidebar_collapsed: bool,
    started_at: Instant,
    first_frame_logged: bool,
    address: gpui_kit::Entity<InputState>,
    _address_subscription: gpui_kit::Subscription,
    engine: engine::Engine,
    cef_task_started: bool,
}

impl Lumen {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut personal = Space::new("personal", "Personal", 0x9aa6ff);
        let mut welcome = Tab::new("welcome", "Welcome to Lumen", "lumen://welcome");
        welcome.state = TabState::Live;
        personal.active = Some(welcome.id.clone());
        personal.tabs.push(welcome);

        let mut work = Space::new("work", "Work", 0x76d7b2);
        work.tabs.push(Tab::new(
            "work-start",
            "Getting started",
            "https://example.com",
        ));
        work.active = Some("work-start".into());

        let address = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search or enter a website address"));
        let address_subscription = cx.subscribe(&address, |this: &mut Lumen, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.navigate(cx);
                cx.notify();
            }
        });

        Self {
            spaces: vec![personal, work],
            active_space: "personal".into(),
            sidebar_collapsed: false,
            started_at: Instant::now(),
            first_frame_logged: false,
            address,
            _address_subscription: address_subscription,
            engine: engine::Engine::new(),
            cef_task_started: false,
        }
    }

    fn active_space(&self) -> &Space {
        self.spaces
            .iter()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }

    fn activate_space(&mut self, id: &str) {
        self.active_space = id.to_owned();
    }

    fn add_tab(&mut self) {
        let space = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist");
        let id = format!("tab-{}", space.tabs.len() + 1);
        let tab = Tab::new(&id, "New tab", "about:blank");
        space.tabs.push(tab);
        space.active = Some(id);
    }

    fn navigate(&mut self, cx: &Context<Self>) {
        let input = self.address.read(cx).value().to_string();
        let space = self.active_space_mut();
        let Some((id, url)) = submit_address(space, &input) else {
            return;
        };
        self.engine.navigate(&id, &url);
    }

    fn active_space_mut(&mut self) -> &mut Space {
        self.spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist")
    }
}

fn normalize_address(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if input.contains("://") || input.starts_with("about:") {
        input.to_owned()
    } else if input == "localhost" || input.starts_with("localhost:") || input.contains('.') {
        format!("https://{input}")
    } else {
        format!("https://duckduckgo.com/?q={}", input.replace(' ', "+"))
    }
}

fn submit_address(space: &mut Space, input: &str) -> Option<(String, String)> {
    let url = normalize_address(input);
    let url = if url.is_empty() {
        "https://duckduckgo.com".to_owned()
    } else {
        url
    };

    if space.active.is_none() {
        let id = format!("tab-{}", space.tabs.len() + 1);
        space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
        space.active = Some(id);
    }
    let id = space.active.clone()?;
    let tab = space.tabs.iter_mut().find(|tab| tab.id == id)?;
    tab.url = url.clone();
    tab.title = url.clone();
    tab.state = TabState::Live;
    Some((id, url))
}

impl Render for Lumen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            eprintln!(
                "Lumen time-to-first-frame: {:.1} ms",
                self.started_at.elapsed().as_secs_f64() * 1000.0
            );
        }

        #[cfg(not(test))]
        if !self.cef_task_started {
            self.cef_task_started = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                loop {
                    let updated = this.update(cx, |this, cx| {
                        if let Err(error) = this.engine.initialize() {
                            eprintln!("Lumen CEF startup failed: {error}");
                            return false;
                        }
                        this.engine.pump();
                        cx.notify();
                        true
                    });
                    if !matches!(updated, Ok(true)) {
                        break;
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                }
            })
            .detach();
        }

        let active_space_id = self.active_space.clone();
        let active_space = self.active_space();
        let active_tab_id = active_space.active.clone();
        let selected_name = active_space.name.clone();
        let selected_color = active_space.color;
        let tabs: Vec<(String, String)> = active_space
            .tabs
            .iter()
            .map(|tab| (tab.id.clone(), tab.title.clone()))
            .collect();
        let active_tab_title = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref())
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| "New tab".into());
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab = active_space
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == active_tab_id.as_ref());
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab_id_string = active_tab_id.clone().unwrap_or_default();
        #[cfg(all(target_os = "linux", not(test)))]
        let active_tab_url = active_tab.map(|tab| tab.url.clone()).unwrap_or_default();

        #[cfg(all(target_os = "linux", not(test)))]
        if self.engine.is_initialized() && active_tab_url != "lumen://welcome" {
            if let Ok(handle) = _window.window_handle() {
                let parent = match handle.as_raw() {
                    RawWindowHandle::Xlib(xlib) => Some(xlib.window as u64),
                    RawWindowHandle::Xcb(xcb) => Some(u64::from(xcb.window.get())),
                    _ => None,
                };
                if let Some(parent) = parent {
                    let scale = _window.scale_factor();
                    let sidebar = if self.sidebar_collapsed { 64.0 } else { 240.0 };
                    let bounds = _window.bounds();
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    let tab_id = active_tab_id_string.clone();
                    if !tab_id.is_empty() {
                        let rect = [
                            (sidebar * scale) as i32,
                            (52.0 * scale) as i32,
                            ((width - sidebar).max(1.0) * scale) as i32,
                            ((height - 52.0).max(1.0) * scale) as i32,
                        ];
                        if let Err(error) =
                            self.engine
                                .ensure_browser(&tab_id, &active_tab_url, parent, rect)
                        {
                            eprintln!("Lumen CEF browser creation failed: {error}");
                        }
                        self.engine.hide_except(&tab_id);
                    }
                }
            }
        }

        let sidebar_width = if self.sidebar_collapsed {
            px(64.0)
        } else {
            px(240.0)
        };
        let mut sidebar_content = div()
            .id("sidebar")
            .flex()
            .flex_col()
            .w(sidebar_width)
            .h_full()
            .px_3()
            .py_3()
            .gap_2()
            .bg(rgb(0x191a1f));

        if !self.sidebar_collapsed {
            sidebar_content = sidebar_content
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xf1f2f5))
                                .child("Lumen"),
                        )
                        .child(
                            div()
                                .id("collapse-sidebar")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_color(rgb(0x858994))
                                .hover(|style| style.bg(rgb(0x282b31)))
                                .cursor_pointer()
                                .child("‹")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sidebar_collapsed = true;
                                    cx.notify();
                                })),
                        ),
                )
                .child(sidebar::section_label("SPACES"));

            for space in &self.spaces {
                let id = space.id.clone();
                let is_active = space.id == active_space_id;
                let element = space_switcher::row(&space.name, space.color);
                sidebar_content = sidebar_content.child(
                    div()
                        .id(format!("space-{id}"))
                        .w_full()
                        .cursor_pointer()
                        .when(is_active, |this| this.bg(rgb(0x25272d)))
                        .child(element)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_space(&id);
                            cx.notify();
                        })),
                );
            }

            sidebar_content = sidebar_content.child(sidebar::section_label("TABS"));
            for (id, title) in &tabs {
                let tab_id = id.clone();
                let current_space_id = active_space_id.clone();
                let selected = active_tab_id.as_ref() == Some(id);
                sidebar_content = sidebar_content.child(
                    div()
                        .id(format!("tab-{id}"))
                        .w_full()
                        .cursor_pointer()
                        .child(tab_list::tab(title, selected))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(space) = this
                                .spaces
                                .iter_mut()
                                .find(|space| space.id == current_space_id)
                            {
                                space.active = Some(tab_id.clone());
                            }
                            cx.notify();
                        })),
                );
            }

            sidebar_content = sidebar_content.child(div().flex_1().min_h(px(8.0))).child(
                div()
                    .id("new-tab-action")
                    .w_full()
                    .cursor_pointer()
                    .child(sidebar::new_tab_button())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_tab();
                        cx.notify();
                    })),
            );
        } else {
            sidebar_content = sidebar_content.child(
                div()
                    .id("expand-sidebar")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .text_color(rgb(0x858994))
                    .hover(|style| style.bg(rgb(0x282b31)))
                    .cursor_pointer()
                    .child("›")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar_collapsed = false;
                        cx.notify();
                    })),
            );
        }

        div()
            .flex()
            .size_full()
            .bg(rgb(0x111216))
            .text_color(rgb(0xe6e7eb))
            .child(sidebar_content)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(52.0))
                            .flex()
                            .items_center()
                            .px_4()
                            .gap_3()
                            .border_b_1()
                            .border_color(rgb(0x23252b))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x777b85))
                                    .child(format!("{} ·", selected_name)),
                            )
                            .child(Input::new(&self.address).id("address-bar").w_full())
                            .child(
                                div()
                                    .id("navigate")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .bg(rgb(0x25272d))
                                    .text_sm()
                                    .text_color(rgb(0xc6c8cf))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x30333a)))
                                    .child("Go")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.navigate(cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(44.0))
                                    .rounded_xl()
                                    .bg(rgb(selected_color))
                                    .text_color(rgb(0x111216))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("L"),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xe6e7eb))
                                    .child(active_tab_title),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x777b85))
                                    .child("Your browsing space is ready"),
                            ),
                    ),
            )
    }
}

fn main() {
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
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let bounds = Bounds::centered(None, size(px(1200.0), px(760.0)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Lumen::new(window, cx)),
            )
            .expect("open Lumen window");
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_bar_routes_urls_and_searches_to_the_active_tab() {
        let mut space = Space::new("test", "Test", 0x123456);
        let tab = Tab::new("existing", "Welcome", "lumen://welcome");
        space.active = Some(tab.id.clone());
        space.tabs.push(tab);

        let (id, url) = submit_address(&mut space, "youtube").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://duckduckgo.com/?q=youtube");
        assert_eq!(space.tabs[0].url, url);
        assert_eq!(space.tabs[0].title, url);
        assert_eq!(space.tabs[0].state, TabState::Live);
    }

    #[test]
    fn empty_address_opens_search_home_and_creates_a_tab_if_needed() {
        let mut space = Space::new("test", "Test", 0x123456);
        let (id, url) = submit_address(&mut space, "").unwrap();
        assert_eq!(id, "tab-1");
        assert_eq!(url, "https://duckduckgo.com");
        assert_eq!(space.active.as_deref(), Some("tab-1"));
        assert_eq!(space.tabs[0].url, url);
    }
}
