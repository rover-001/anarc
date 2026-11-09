mod engine;
mod lifecycle;
mod model;
mod persist;
mod stats;
mod ui;

use std::time::Instant;

use gpui_kit::{
    Bounds, Context, FontWeight, Render, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};
use model::{Space, Tab, TabState};
use ui::{sidebar, space_switcher, tab_list};

struct Lumen {
    spaces: Vec<Space>,
    active_space: String,
    sidebar_collapsed: bool,
    started_at: Instant,
    first_frame_logged: bool,
}

impl Lumen {
    fn new() -> Self {
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

        Self {
            spaces: vec![personal, work],
            active_space: "personal".into(),
            sidebar_collapsed: false,
            started_at: Instant::now(),
            first_frame_logged: false,
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
                            .px_6()
                            .border_b_1()
                            .border_color(rgb(0x23252b))
                            .text_sm()
                            .text_color(rgb(0x9498a3))
                            .child(format!("{}  ·  {}", selected_name, active_tab_title)),
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
                |_, cx| cx.new(|_| Lumen::new()),
            )
            .expect("open Lumen window");
        });
}
