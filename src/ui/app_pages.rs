use std::collections::HashSet;

use gpui_kit::{
    Context, FontWeight, StatefulInteractiveElement, assets::IconName, component::Icon,
    component::scroll::ScrollableElement, div, prelude::*, px, rgb,
};

use crate::app::SettingsSection;
use crate::app::{AppPage, Lumen};
use crate::engine::DownloadRecord;
use crate::settings::{SearchEngine, SidebarMode};
use crate::ui::visual::{self, SpaceTheme, UI_SCALE};

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
    sidebar_mode: SidebarMode,
    active_space_color: u32,
    plugin_rows: Vec<crate::plugin::PluginRow>,
    pending_plugin_review: Option<crate::ui::plugin_manager::PluginReview>,
    plugin_action_error: Option<String>,
    expanded_plugins: HashSet<String>,
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
            sidebar_mode,
            active_space_color,
            plugin_rows,
            pending_plugin_review,
            plugin_action_error,
            expanded_plugins,
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
                        .gap_2p5()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(visual::anarc_logo().map_or_else(
                            || Icon::new(icon).into_any_element(),
                            |logo| {
                                div()
                                    .size(px(22.0 * UI_SCALE))
                                    .flex_shrink_0()
                                    .child(gpui_kit::img(logo).size_full())
                                    .into_any_element()
                            },
                        ))
                        .child(if page == AppPage::Settings {
                            "Anarc Settings"
                        } else {
                            title
                        }),
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
                        .cursor_pointer()
                        .active(|style| style.opacity(0.88))
                        .hover(move |style| {
                            style
                                .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
                                .text_color(rgb(theme.text))
                        })
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
    sidebar_mode: SidebarMode,
    _active_space_color: u32,
    plugin_rows: Vec<crate::plugin::PluginRow>,
    pending_plugin_review: Option<crate::ui::plugin_manager::PluginReview>,
    plugin_action_error: Option<String>,
    expanded_plugins: HashSet<String>,
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
            "Shortcuts",
            SettingsSection::Shortcuts,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Shortcuts);
                cx.notify();
            }),
        ))
        .child(settings_tab(
            "Plugins",
            SettingsSection::Plugins,
            selected_section,
            theme,
            cx.listener(|this, _, _, cx| {
                this.set_settings_section(SettingsSection::Plugins);
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
            let search_choices = div()
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
            let shield_control = if has_web_site {
                crate::ui::plugin_manager::toggle_switch(
                    "settings-site-protection".to_owned(),
                    site_protection_enabled,
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_shield_for_active_tab();
                        cx.notify();
                    }),
                )
            } else {
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child("Open a site to change")
                    .into_any_element()
            };
            let protection_text = if !has_web_site {
                "Open a site to change"
            } else if site_protection_enabled {
                "On for this site"
            } else {
                "Off for this site"
            };
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(setting_card(
                    "Search",
                    "Default engine",
                    "Used for address-bar searches.",
                    search_choices,
                    theme,
                ))
                .child(setting_card(
                    "Privacy",
                    "Tracking protection",
                    protection_text,
                    shield_control,
                    theme,
                ))
                .child(setting_card(
                    "Browsing data",
                    "History & suggestions",
                    "Clear visited pages and saved searches.",
                    clear_button,
                    theme,
                ))
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
                "Space accent",
                "Accent color for this space.",
                color_controls,
                theme,
            );
            let zoom_card = setting_card(
                "Page zoom",
                if has_web_site {
                    "Current website"
                } else {
                    "New websites"
                },
                "Adjust in 10% steps.",
                zoom_controls,
                theme,
            );
            let sidebar_choices = div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(action_button(
                    if sidebar_mode == SidebarMode::Hidden {
                        "✓ Hide"
                    } else {
                        "Hide"
                    },
                    theme,
                    cx.listener(|this, _, window, cx| {
                        this.set_sidebar_mode(SidebarMode::Hidden, window, cx);
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    if sidebar_mode == SidebarMode::Icons {
                        "✓ Icons"
                    } else {
                        "Icons"
                    },
                    theme,
                    cx.listener(|this, _, window, cx| {
                        this.set_sidebar_mode(SidebarMode::Icons, window, cx);
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    if sidebar_mode == SidebarMode::Hover {
                        "✓ Reveal on hover"
                    } else {
                        "Reveal on hover"
                    },
                    theme,
                    cx.listener(|this, _, window, cx| {
                        this.set_sidebar_mode(SidebarMode::Hover, window, cx);
                        cx.notify();
                    }),
                ));
            let sidebar_card = setting_card(
                "Sidebar",
                "Collapsed appearance",
                "Hidden, tab icons, or reveal on hover.",
                sidebar_choices,
                theme,
            );
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(theme_card)
                .child(zoom_card)
                .child(sidebar_card)
                .into_any_element()
        }
        SettingsSection::Shortcuts => div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section(
                    "Tabs & spaces",
                    vec![
                        ("Ctrl + T", "New tab"),
                        ("Ctrl + W", "Close tab or this panel"),
                        ("Ctrl + Shift + T", "Reopen closed tab"),
                        ("Ctrl + Tab", "Next tab"),
                        ("Ctrl + Shift + Tab", "Previous tab"),
                        ("Ctrl + 1 … 9", "Jump to tab (9 = last)"),
                        ("Ctrl + Shift + S", "Split active tab"),
                        ("Ctrl + Shift + W", "Exit split view"),
                        ("Ctrl + Alt + ← / →", "Focus split pane"),
                    ],
                    theme,
                ),
            )
            .child(
                section(
                    "Navigation & address bar",
                    vec![
                        ("Ctrl + L", "Focus address bar"),
                        ("Enter", "Open typed URL or suggestion"),
                        ("↑ / ↓", "Move address suggestion"),
                        ("Esc", "Dismiss suggestions"),
                        ("Alt + ← / →", "Back / forward"),
                        ("Ctrl + R", "Reload"),
                        ("Ctrl + Shift + R", "Hard reload"),
                        ("Ctrl + V", "Paste into address bar"),
                    ],
                    theme,
                ),
            )
            .child(
                section(
                    "Page & find",
                    vec![
                        ("Ctrl + F", "Find in page"),
                        ("Enter / Shift + Enter", "Find next / previous"),
                        ("Esc", "Close find bar"),
                        ("Ctrl + C / X / V", "Copy / cut / paste selection"),
                        ("Ctrl + A", "Select all"),
                        ("Esc", "Stop page loading"),
                    ],
                    theme,
                ),
            )
            .child(
                section(
                    "Tools & appearance",
                    vec![
                        ("Ctrl + K", "Command palette"),
                        ("Ctrl + S", "Toggle sidebar"),
                        ("Ctrl + Shift + B", "Toggle site shield"),
                        ("Ctrl + Alt + R", "Reader mode"),
                        ("Ctrl + = / − / 0", "Zoom in / out / reset"),
                    ],
                    theme,
                ),
            )
            .child(
                section(
                    "Dialogs & overlays",
                    vec![
                        ("Esc", "Close palette, peek, or menu"),
                        ("↑ / ↓", "Move palette selection"),
                        ("Enter", "Run selected command"),
                    ],
                    theme,
                ),
            )
            .child(
                section(
                    "Plugins",
                    vec![("Ctrl + J", "Next tab (Vim keymap)")],
                    theme,
                ),
            )
            .into_any_element(),
        SettingsSection::Plugins => plugin_manager_content(
            theme,
            plugin_rows,
            pending_plugin_review,
            plugin_action_error,
            expanded_plugins,
            cx,
        ),
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

fn plugin_manager_content(
    theme: SpaceTheme,
    plugin_rows: Vec<crate::plugin::PluginRow>,
    pending_review: Option<crate::ui::plugin_manager::PluginReview>,
    feedback: Option<String>,
    expanded: HashSet<String>,
    cx: &mut Context<Lumen>,
) -> gpui_kit::AnyElement {
    use std::rc::Rc;

    let entity = cx.entity();
    if let Some(review) = pending_review {
        let cancel_entity = entity.clone();
        let on_cancel: crate::ui::plugin_manager::ActionCallback = Rc::new(move |_, _, app| {
            app.update_entity(&cancel_entity, |this, cx| {
                let result = this.confirm_plugin_install(false);
                this.set_plugin_action_error(result.err());
                cx.notify();
            });
        });
        let install_entity = entity.clone();
        let on_install: crate::ui::plugin_manager::ActionCallback = Rc::new(move |_, _, app| {
            app.update_entity(&install_entity, |this, cx| {
                let result = this.confirm_plugin_install(true);
                this.set_plugin_action_error(result.err());
                cx.notify();
            });
        });
        return crate::ui::plugin_manager::render_permission_review(
            &review, theme, on_cancel, on_install,
        );
    }
    let toggle_entity = entity.clone();
    let on_toggle: crate::ui::plugin_manager::ToggleCallback =
        Rc::new(move |id, enabled, _, _, app| {
            app.update_entity(&toggle_entity, |this, cx| {
                let result = this.set_plugin_enabled(&id, enabled);
                this.set_plugin_action_error(
                    result
                        .err()
                        .map(|error| format!("Failed to update plugin {id}: {error}")),
                );
                cx.notify();
            });
        });
    let plugin_dir = crate::paths::data_dir().join("plugins");
    let install_entity = entity.clone();
    let on_install: crate::ui::plugin_manager::InstallCallback =
        Rc::new(move |source, _, _, app| {
            let entity = install_entity.clone();
            app.update_entity(&entity, move |_this, cx| {
                cx.spawn(async move |this, cx| {
                    let picked = cx
                        .background_executor()
                        .spawn(async move { pick_plugin_path(source) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        match picked {
                            Ok(Some(path)) => {
                                let result = this.install_plugin_path(&path, source);
                                this.set_plugin_action_error(result.err().map(|error| {
                                    format!("Failed to inspect plugin {}: {error}", path.display())
                                }));
                            }
                            Ok(None) => {}
                            Err(error) => this.set_plugin_action_error(Some(format!(
                                "Plugin picker failed: {error}"
                            ))),
                        }
                        cx.notify();
                    });
                })
                .detach();
            });
        });
    let remove_entity = entity.clone();
    let on_remove: crate::ui::plugin_manager::PluginActionCallback =
        Rc::new(move |id, _, _, app| {
            let entity = remove_entity.clone();
            app.update_entity(&entity, move |this, cx| {
                let result = this.remove_plugin(&id);
                this.set_plugin_action_error(
                    result
                        .err()
                        .map(|error| format!("Failed to remove plugin {id}: {error}")),
                );
                cx.notify();
            });
        });
    let open_dir = plugin_dir;
    let open_dir_entity = entity.clone();
    let on_open_plugin_folder: crate::ui::plugin_manager::ActionCallback =
        Rc::new(move |_, _, app| {
            let result = std::fs::create_dir_all(&open_dir)
                .map_err(|error| {
                    format!(
                        "Failed to create plugin directory {}: {error}",
                        open_dir.display()
                    )
                })
                .and_then(|()| open_local_directory(&open_dir));
            let entity = open_dir_entity.clone();
            app.update_entity(&entity, move |this, cx| {
                this.set_plugin_action_error(result.err());
                cx.notify();
            });
        });

    let open_panel_entity = entity.clone();
    let on_open_panel: crate::ui::plugin_manager::PluginActionCallback =
        Rc::new(move |id, _, window, app| {
            let entity = open_panel_entity.clone();
            app.update_entity(&entity, move |this, cx| {
                match this.open_plugin_panel(&id) {
                    Ok(()) => {
                        this.set_plugin_action_error(None);
                        this.focus_browser(window, cx);
                    }
                    Err(error) => this.set_plugin_action_error(Some(error)),
                }
                cx.notify();
            });
        });

    let details_entity = entity.clone();
    let on_toggle_details: crate::ui::plugin_manager::DetailsCallback =
        Rc::new(move |id, _, _, app| {
            let entity = details_entity.clone();
            app.update_entity(&entity, move |this, cx| {
                this.toggle_plugin_details(id);
                cx.notify();
            });
        });
    crate::ui::plugin_manager::render(
        &plugin_rows,
        &[],
        feedback.as_deref(),
        theme,
        on_toggle,
        on_remove,
        on_install,
        on_open_plugin_folder,
        on_open_panel,
        &expanded,
        on_toggle_details,
    )
}

fn pick_plugin_path(
    source: crate::ui::plugin_manager::InstallSource,
) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let title = match source {
            crate::ui::plugin_manager::InstallSource::Folder => "Select plugin folder",
            crate::ui::plugin_manager::InstallSource::Package => "Select plugin package",
        };
        let zenity_args = match source {
            crate::ui::plugin_manager::InstallSource::Folder => {
                vec!["--file-selection", "--directory", "--title", title]
            }
            crate::ui::plugin_manager::InstallSource::Package => vec![
                "--file-selection",
                "--title",
                title,
                "--file-filter=Anarc plugins | *.anarcplugin *.lumenplugin",
            ],
        };
        match Command::new("zenity").args(zenity_args).output() {
            Ok(output) => return picker_output(output),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not start zenity: {error}")),
        }

        let (kdialog_mode, filter) = match source {
            crate::ui::plugin_manager::InstallSource::Folder => ("--getexistingdirectory", ""),
            crate::ui::plugin_manager::InstallSource::Package => {
                ("--getopenfilename", "*.anarcplugin *.lumenplugin")
            }
        };
        let mut command = Command::new("kdialog");
        command.arg(kdialog_mode);
        if !filter.is_empty() {
            command.arg("").arg(filter);
        }
        command.arg("--title").arg(title);
        match command.output() {
            Ok(output) => return picker_output(output),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not start kdialog: {error}")),
        }

        Err("No file chooser is installed. Install zenity or kdialog to select plugins.".into())
    }

    #[cfg(target_os = "windows")]
    {
        let is_folder = matches!(source, crate::ui::plugin_manager::InstallSource::Folder);
        let title_str = if is_folder {
            "Select plugin folder"
        } else {
            "Select plugin package"
        };
        let script = if is_folder {
            format!(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $f = New-Object System.Windows.Forms.FolderBrowserDialog; \
                 $f.Description = '{title_str}'; \
                 if ($f.ShowDialog() -eq 'OK') {{ [Console]::WriteLine($f.SelectedPath) }}"
            )
        } else {
            format!(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $f = New-Object System.Windows.Forms.OpenFileDialog; \
                 $f.Title = '{title_str}'; \
                 $f.Filter = 'Anarc plugins (*.anarcplugin;*.lumenplugin)|*.anarcplugin;*.lumenplugin|All files (*.*)|*.*'; \
                 if ($f.ShowDialog() -eq 'OK') {{ [Console]::WriteLine($f.FileName) }}"
            )
        };
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let output = cmd
            .output()
            .map_err(|e| format!("could not start powershell: {e}"))?;
        if output.status.success() {
            let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !selected.is_empty() {
                return Ok(Some(selected.into()));
            }
        }
        return Ok(None);
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = source;
        Err("Plugin installation picker is currently available on Linux and Windows only.".into())
    }
}

#[cfg(target_os = "linux")]
fn picker_output(output: std::process::Output) -> Result<Option<std::path::PathBuf>, String> {
    use std::os::unix::process::ExitStatusExt;
    if output.status.success() {
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if selected.is_empty() {
            return Ok(None);
        }
        return Ok(Some(selected.into()));
    }
    if output.status.code() == Some(1) || output.status.signal() == Some(1) {
        return Ok(None);
    }
    Err(format!("file chooser exited with {}", output.status))
}

fn open_local_directory(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    result.map(|_| ()).map_err(|error| {
        format!(
            "Failed to open plugin directory {}: {error}",
            path.display()
        )
    })
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
        .debug_selector(|| format!("settings-tab-{label}"))
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(if selected {
            rgb(theme.accent)
        } else {
            crate::ui::visual::translucent_surface(theme.surface, 0x30)
        })
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if selected { theme.base } else { theme.text }))
        .cursor_pointer()
        .active(|style| style.opacity(0.88))
        .hover(move |style| {
            if selected {
                style
            } else {
                style.bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
            }
        })
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
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
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
        .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
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
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
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
                    .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
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

pub(crate) fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes >= 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.0} KB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}
