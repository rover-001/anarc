use gpui_kit::{
    Context, FontWeight, StatefulInteractiveElement, assets::IconName, component::Icon,
    component::scroll::ScrollableElement, div, prelude::*, px, rgb,
};

use crate::app::SettingsSection;
use crate::app::{AppPage, Lumen};
use crate::engine::DownloadRecord;
use crate::settings::SearchEngine;
use crate::ui::visual::{SpaceTheme, UI_SCALE};

#[allow(clippy::too_many_arguments)] // Settings receives the current page state and its controls.
pub fn render(
    page: AppPage,
    theme: SpaceTheme,
    downloads: Vec<DownloadRecord>,
    page_zoom: f64,
    site_protection_enabled: bool,
    has_web_site: bool,
    settings_section: SettingsSection,
    search_engine: SearchEngine,
    active_space_color: u32,
    cx: &mut Context<Lumen>,
) -> gpui_kit::AnyElement {
    let (title, icon) = match page {
        AppPage::Settings => ("Settings", IconName::Settings),
        AppPage::Downloads => ("Downloads", IconName::Download),
    };
    let content = match page {
        AppPage::Settings => settings_content(
            theme,
            page_zoom,
            site_protection_enabled,
            has_web_site,
            settings_section,
            search_engine,
            active_space_color,
            downloads.clone(),
            cx,
        ),
        AppPage::Downloads => downloads_content(downloads, theme),
    };

    div()
        .id("app-page")
        .size_full()
        .flex()
        .flex_col()
        .text_color(rgb(theme.text))
        .child(
            div()
                .w_full()
                .h(px(56.0 * UI_SCALE))
                .px_5()
                .border_b_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Icon::new(icon))
                        .child(title),
                )
                .child(
                    div()
                        .id("close-app-page")
                        .size(px(30.0 * UI_SCALE))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(theme.muted))
                        .hover(move |style| {
                            style
                                .bg(rgb(theme.surface_hover))
                                .text_color(rgb(theme.text))
                        })
                        .cursor_pointer()
                        .child(Icon::new(IconName::X))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_app_page();
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex_1()
                .w_full()
                .overflow_y_scrollbar()
                .child(content),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)] // Keep the settings page data and GPUI action context explicit.
fn settings_content(
    theme: SpaceTheme,
    page_zoom: f64,
    site_protection_enabled: bool,
    has_web_site: bool,
    selected_section: SettingsSection,
    search_engine: SearchEngine,
    _active_space_color: u32,
    downloads: Vec<DownloadRecord>,
    cx: &mut Context<Lumen>,
) -> gpui_kit::AnyElement {
    let tabs = div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(settings_tab(
            "General",
            SettingsSection::General,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::General);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Appearance",
            SettingsSection::Appearance,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Appearance);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Search",
            SettingsSection::Search,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Search);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Privacy",
            SettingsSection::Privacy,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Privacy);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Downloads",
            SettingsSection::Downloads,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Downloads);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Shortcuts",
            SettingsSection::Shortcuts,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Shortcuts);
                cx.notify();
            }),
        ));

    let content = match selected_section {
        SettingsSection::General => {
            let clear_button = action_button(
                "Clear history",
                theme,
                cx.listener(|this, _, _, cx| {
                    this.clear_history();
                    cx.notify();
                }),
            );
            setting_card(
                "Browsing data",
                "History and search suggestions",
                "Remove visited pages and saved address-bar searches.",
                clear_button,
                theme,
            )
            .into_any_element()
        }
        SettingsSection::Appearance => {
            let zoom_controls = div()
                .flex()
                .items_center()
                .gap_2()
                .child(action_button(
                    "−",
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.change_zoom(-0.1);
                        cx.notify();
                    }),
                ))
                .child(
                    div()
                        .w(px(64.0))
                        .text_sm()
                        .text_center()
                        .child(format!("{:.0}%", page_zoom * 100.0)),
                )
                .child(action_button(
                    "+",
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.change_zoom(0.1);
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    "Reset",
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.change_zoom(0.0);
                        cx.notify();
                    }),
                ));
            let color_controls = action_button(
                "Edit theme…",
                theme,
                cx.listener(|this, event: &gpui_kit::ClickEvent, window, cx| {
                    this.open_theme_editor(event.position(), false, window, cx);
                }),
            );
            let theme_card = setting_card(
                "Theme",
                "Space accent color",
                "Choose the accent used by this space's browser chrome.",
                color_controls,
                theme,
            );
            let zoom_card = setting_card(
                "Page zoom",
                if has_web_site {
                    "Current website"
                } else {
                    "Default zoom for new websites"
                },
                if has_web_site {
                    "Use + or − to adjust this website in 10% steps."
                } else {
                    "This default applies to new websites; + or − changes it in 10% steps."
                },
                zoom_controls,
                theme,
            );
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(theme_card)
                .child(zoom_card)
                .into_any_element()
        }
        SettingsSection::Search => {
            let choices = div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(action_button(
                    if search_engine == SearchEngine::Google {
                        "✓ Google"
                    } else {
                        "Google"
                    },
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.set_search_engine(SearchEngine::Google);
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    if search_engine == SearchEngine::Bing {
                        "✓ Bing"
                    } else {
                        "Bing"
                    },
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.set_search_engine(SearchEngine::Bing);
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    if search_engine == SearchEngine::DuckDuckGo {
                        "✓ DuckDuckGo"
                    } else {
                        "DuckDuckGo"
                    },
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.set_search_engine(SearchEngine::DuckDuckGo);
                        cx.notify();
                    }),
                ));
            setting_card(
                "Address bar",
                "Search engine",
                &format!("Current: {}", search_engine.label()),
                choices,
                theme,
            )
            .into_any_element()
        }
        SettingsSection::Privacy => {
            let shield_text = if !has_web_site {
                "Open a website to change its protection setting".to_owned()
            } else if site_protection_enabled {
                "Protection is on for this website".to_owned()
            } else {
                "Protection is off for this website".to_owned()
            };
            let shield_control = if has_web_site {
                action_button(
                    if site_protection_enabled {
                        "Turn off"
                    } else {
                        "Turn on"
                    },
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_shield_for_active_tab();
                        cx.notify();
                    }),
                )
                .into_any_element()
            } else {
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child("Unavailable")
                    .into_any_element()
            };
            setting_card(
                "Site protection",
                "Tracking protection",
                &shield_text,
                shield_control,
                theme,
            )
            .into_any_element()
        }
        SettingsSection::Downloads => downloads_content(downloads, theme),
        SettingsSection::Shortcuts => section(
            "Keyboard shortcuts",
            vec![
                ("Ctrl + L", "Focus the address bar"),
                ("Ctrl + T", "Open a tab"),
                ("Ctrl + W", "Close the active tab or this settings window"),
                ("Ctrl + K", "Open the command palette"),
                ("Ctrl + Shift + S", "Split the active tab with another tab"),
                ("Ctrl + + / − / 0", "Change or reset page zoom"),
            ],
            theme,
        )
        .into_any_element(),
    };

    div()
        .w_full()
        .max_w(px(900.0))
        .mx_auto()
        .p_5()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .text_sm()
                .text_color(rgb(theme.muted))
                .child("Browser preferences"),
        )
        .child(tabs)
        .child(content)
        .into_any_element()
}

fn settings_tab(
    label: &'static str,
    section: SettingsSection,
    selected_section: SettingsSection,
    theme: SpaceTheme,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    let selected = section == selected_section;
    div()
        .id(format!("settings-tab-{label}"))
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(if selected {
            theme.accent
        } else {
            theme.surface
        }))
        .text_xs()
        .text_color(rgb(if selected { theme.base } else { theme.text }))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.surface_hover)))
        .child(label)
        .on_click(on_click)
}

fn setting_card(
    section_title: &'static str,
    name: &'static str,
    description: &str,
    control: impl IntoElement,
    theme: SpaceTheme,
) -> impl IntoElement {
    div()
        .w_full()
        .p_4()
        .rounded_xl()
        .bg(rgb(theme.surface))
        .border_1()
        .border_color(rgb(theme.border))
        .flex()
        .flex_col()
        .child(
            div()
                .pb_2()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(section_title),
        )
        .child(
            div()
                .w_full()
                .py_3()
                .border_t_1()
                .border_color(rgb(theme.border))
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child(name))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child(description.to_owned()),
                        ),
                )
                .child(control),
        )
}

fn action_button(
    label: &'static str,
    theme: SpaceTheme,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    div()
        .id(format!("settings-action-{label}"))
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(theme.surface_hover))
        .text_xs()
        .text_color(rgb(theme.text))
        .hover(|style| style.bg(rgb(theme.border_hover)))
        .cursor_pointer()
        .child(label)
        .on_click(on_click)
}

fn section(
    title: &'static str,
    rows: Vec<(&'static str, &'static str)>,
    theme: SpaceTheme,
) -> impl IntoElement {
    let mut body = div().flex().flex_col();
    for (name, description) in rows {
        body = body.child(
            div()
                .w_full()
                .py_3()
                .border_t_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .child(div().text_sm().text_color(rgb(theme.text)).child(name))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(description),
                ),
        );
    }
    div()
        .w_full()
        .p_4()
        .rounded_xl()
        .bg(rgb(theme.surface))
        .border_1()
        .border_color(rgb(theme.border))
        .flex()
        .flex_col()
        .child(
            div()
                .pb_2()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(body)
}

fn downloads_content(downloads: Vec<DownloadRecord>, theme: SpaceTheme) -> gpui_kit::AnyElement {
    let body = if downloads.is_empty() {
        div()
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .text_color(rgb(theme.muted))
            .child(Icon::new(IconName::Download).size(px(28.0 * UI_SCALE)))
            .child(div().text_sm().child("No downloads yet"))
            .into_any_element()
    } else {
        let mut list = div().w_full().flex().flex_col().gap_2();
        for download in downloads.into_iter().rev() {
            let filename = download
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| download.path.display().to_string());
            let status = if download.canceled {
                "Canceled".to_owned()
            } else if download.complete {
                "Complete".to_owned()
            } else if download.total_bytes > 0 {
                format!(
                    "{}% · {} / {}",
                    (download.received_bytes * 100 / download.total_bytes).clamp(0, 100),
                    format_bytes(download.received_bytes),
                    format_bytes(download.total_bytes),
                )
            } else {
                format!("{} downloaded", format_bytes(download.received_bytes))
            };
            list = list.child(
                div()
                    .id(format!("download-{}", download.id))
                    .w_full()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(IconName::Download).text_color(rgb(theme.muted)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.text))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(filename),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(download.url),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if download.canceled {
                                0xff8b8b8b
                            } else if download.complete {
                                0x76d7b2
                            } else {
                                theme.accent
                            }))
                            .child(status),
                    ),
            );
        }
        list.into_any_element()
    };

    div().w_full().h_full().p_5().child(body).into_any_element()
}

fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes >= 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.0} KB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}
