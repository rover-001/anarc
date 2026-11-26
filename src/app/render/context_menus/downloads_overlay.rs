//! Download completion toast, recent-downloads bubble, and drag-out payload.

use super::dismiss_backdrop;
use super::super::*;

/// Compact "x minutes ago" label derived from the downloaded file's mtime.
/// Keeps DownloadRecord's persisted shape unchanged.
fn download_age_label(path: &std::path::Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let elapsed = std::time::SystemTime::now()
        .duration_since(modified)
        .ok()?
        .as_secs();
    let label = if elapsed < 60 {
        "just now"
    } else if elapsed < 3600 {
        return Some(format!("{} min ago", elapsed / 60));
    } else if elapsed < 86_400 {
        return Some(format!("{} hours ago", elapsed / 3600));
    } else if elapsed < 604_800 {
        return Some(format!("{} days ago", elapsed / 86_400));
    } else {
        return Some(format!("{} weeks ago", elapsed / 604_800));
    };
    Some(label.to_owned())
}

/// File path carried from a recent-download row to the active web page.
#[derive(Clone)]
struct DownloadFileDrag {
    path: std::path::PathBuf,
    filename: String,
    text_color: u32,
    surface: u32,
}

/// Small chip rendered under the pointer while a download row is dragged.
struct DownloadFileDragGhost {
    filename: String,
    offset: gpui_kit::Point<gpui_kit::Pixels>,
    text_color: u32,
    surface: u32,
}

impl Render for DownloadFileDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.offset.x)
            .pt(self.offset.y)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .max_w(px(240.0 * UI_SCALE))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .bg(rgb(self.surface))
                    .text_xs()
                    .text_color(rgb(self.text_color))
                    .child(self.filename.clone()),
            )
    }
}

impl Anarc {
    pub(super) fn render_download_toast(
        &mut self,
        theme: visual::SpaceTheme,
        downloads: &[engine::DownloadRecord],
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let latest = downloads.last()?;
        let is_active = !latest.complete && !latest.canceled;
        let show_toast = (is_active || latest.complete)
            && self.dismissed_download_toast_id != Some(latest.id)
            && self.app_page != Some(AppPage::Downloads)
            && !self.downloads_bubble_open;
        if !show_toast {
            return None;
        }
        let filename = latest
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_owned());
        let download_id = latest.id;
        let is_complete = latest.complete;
        let pct = if latest.total_bytes > 0 {
            (latest.received_bytes as f32 / latest.total_bytes as f32).clamp(0.0, 1.0)
        } else if is_complete {
            1.0
        } else {
            0.0
        };
        let status_text = if is_complete {
            "Download complete · Saved to ~/Downloads".to_string()
        } else if latest.total_bytes > 0 {
            format!(
                "Downloading... {} / {}",
                ui::app_pages::format_bytes(latest.received_bytes),
                ui::app_pages::format_bytes(latest.total_bytes)
            )
        } else {
            format!(
                "Downloading... {}",
                ui::app_pages::format_bytes(latest.received_bytes)
            )
        };
        let bar_width = 250.0 * UI_SCALE;
        let fill_width = (bar_width * pct).max(if is_active { 4.0 } else { 0.0 });

        let toast = div()
            .id("download-toast")
            .occlude()
            .absolute()
            .top(px(visual::active_topbar_height() + 4.0 * UI_SCALE))
            .right(px(8.0 * UI_SCALE))
            .w(px(280.0 * UI_SCALE))
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(if is_active {
                theme.accent
            } else {
                theme.border
            }))
            .rounded_xl()
            .p_3()
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.app_page = Some(AppPage::Downloads);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Download).text_color(rgb(
                                if is_complete { 0x76d7b2 } else { theme.accent },
                            )))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(if is_complete {
                                        "Download Complete"
                                    } else {
                                        "Download Started"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("dismiss-download-toast")
                            .px_1()
                            .py_0p5()
                            .rounded_md()
                            .hover(move |s| {
                                s.bg(crate::ui::visual::translucent_surface(
                                    theme.surface_hover,
                                    0x40,
                                ))
                            })
                            .text_color(rgb(visual::panel_muted(theme)))
                            .text_xs()
                            .child("✕")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.dismissed_download_toast_id = Some(download_id);
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(filename),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(if is_complete { 0x76d7b2 } else { theme.muted }))
                    .child(status_text),
            )
            .when(!is_complete, |parent| {
                parent.child(
                    div()
                        .w(px(bar_width))
                        .h(px(4.0 * UI_SCALE))
                        .rounded_full()
                        .bg(rgb(theme.border))
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .bg(rgb(theme.accent))
                                .w(px(fill_width)),
                        ),
                )
            });

        Some(toast.into_any_element())
    }

    pub(super) fn render_downloads_bubble(
        &mut self,
        theme: visual::SpaceTheme,
        downloads: &[engine::DownloadRecord],
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        if !self.downloads_bubble_open || !crate::plugins::downloads_enabled() {
            return None;
        }
        // The dismiss layer covers the page, so it is the drop target
        // for a file dragged out of the bubble (the WebViewport's own
        // file-drop listener is occluded while the bubble is open).
        let dismiss = dismiss_backdrop(
            "downloads-bubble-dismiss",
            |this, _, _| {
                this.downloads_bubble_open = false;
            },
            cx,
        )
        .on_drop(cx.listener(|this, drag: &DownloadFileDrag, window, cx| {
            let sidebar_width = if visual::active_layout_preset()
                .as_ref()
                .and_then(|preset| preset.sidebar_side.as_deref())
                == Some("right")
            {
                0.0
            } else {
                this.effective_sidebar_width()
            };
            let position = window.mouse_position();
            let x = (f32::from(position.x) - sidebar_width - crate::app::CONTENT_BEZEL_X)
                .max(0.0) as i32;
            let y = (f32::from(position.y) - visual::active_topbar_height()).max(0.0) as i32;
            let tab_id = this.focused_tab_id().to_string();
            this.engine
                .send_drag_drop_files(&tab_id, std::slice::from_ref(&drag.path), x, y);
            this.downloads_bubble_open = false;
            cx.stop_propagation();
            cx.notify();
        }));

        let mut list = div().w_full().flex().flex_col().gap_1();
        if downloads.is_empty() {
            list = list.child(
                div()
                    .w_full()
                    .py_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(visual::panel_muted(theme)))
                    .child("No downloads yet"),
            );
        } else {
            for download in downloads.iter().rev().take(5) {
                list = list.child(render_download_bubble_row(theme, download));
            }
        }

        let panel = div()
            .id("downloads-bubble-panel")
            .occlude()
            .absolute()
            .top(px(visual::active_topbar_height() + 4.0 * UI_SCALE))
            .right(px(8.0 * UI_SCALE))
            .w(px(340.0 * UI_SCALE))
            .max_h(px(420.0 * UI_SCALE))
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(theme.border))
            .rounded_xl()
            .p_3()
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_2()
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child("Recent download history"),
                    )
                    .child(
                        div()
                            .id("close-downloads-bubble")
                            .px_1()
                            .py_0p5()
                            .rounded_md()
                            .hover(move |s| {
                                s.bg(crate::ui::visual::translucent_surface(
                                    theme.surface_hover,
                                    0x40,
                                ))
                            })
                            .text_color(rgb(visual::panel_muted(theme)))
                            .text_xs()
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.downloads_bubble_open = false;
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_h(px(320.0 * UI_SCALE))
                    .overflow_hidden()
                    .child(list),
            )
            .child(
                div()
                    .id("downloads-bubble-full-history")
                    .w_full()
                    .px_2()
                    .py_2()
                    .rounded_lg()
                    .hover(move |s| {
                        s.bg(crate::ui::visual::translucent_surface(
                            theme.surface_hover,
                            0x40,
                        ))
                    })
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.accent))
                    .child("Full download history")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.downloads_bubble_open = false;
                        this.app_page = Some(AppPage::Downloads);
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .with_spring(
                "downloads-bubble-entrance",
                ui::motion::popover().enter(),
                |panel, progress| {
                    panel
                        .w(px(340.0 * UI_SCALE * (0.96 + 0.04 * progress)))
                        .opacity(ui::motion::clamp_percentage(progress))
                },
            );

        Some(dismiss.child(panel).into_any_element())
    }
}

fn render_download_bubble_row(
    theme: visual::SpaceTheme,
    download: &engine::DownloadRecord,
) -> impl IntoElement {
    let filename = download
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| download.path.display().to_string());
    let (detail, icon_color) = if download.canceled {
        ("Canceled".to_owned(), 0xff8b8b8b)
    } else if download.complete {
        let when = download_age_label(&download.path).unwrap_or_else(|| "Done".to_owned());
        (
            format!(
                "{} · {}",
                ui::app_pages::format_bytes(download.received_bytes),
                when
            ),
            0x76d7b2,
        )
    } else if download.total_bytes > 0 {
        (
            format!(
                "{} / {} · Downloading",
                ui::app_pages::format_bytes(download.received_bytes),
                ui::app_pages::format_bytes(download.total_bytes)
            ),
            theme.accent,
        )
    } else {
        (
            format!(
                "{} · Downloading",
                ui::app_pages::format_bytes(download.received_bytes)
            ),
            theme.accent,
        )
    };
    let drag_payload = DownloadFileDrag {
        path: download.path.clone(),
        filename: filename.clone(),
        text_color: theme.text,
        surface: visual::panel_surface(theme.surface),
    };
    div()
        .id(format!("downloads-bubble-item-{}", download.id))
        .w_full()
        .px_2()
        .py_2()
        .rounded_lg()
        .flex()
        .items_center()
        .gap_2()
        .on_drag(
            drag_payload,
            |drag: &DownloadFileDrag, offset, _, cx: &mut gpui_kit::App| {
                cx.new(|_| DownloadFileDragGhost {
                    filename: drag.filename.clone(),
                    offset,
                    text_color: drag.text_color,
                    surface: drag.surface,
                })
            },
        )
        .external_drag_payload::<DownloadFileDrag>(|drag, _, _| {
            let is_dir = std::fs::metadata(&drag.path)
                .map(|meta| meta.is_dir())
                .unwrap_or(false);
            Some(gpui_kit::ExternalDragPayload::Files(
                gpui_kit::FileDragPaths::new([(drag.path.clone(), is_dir)]),
            ))
        })
        .child(Icon::new(IconName::Download).text_color(rgb(icon_color)))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(filename),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(visual::panel_muted(theme)))
                        .child(detail),
                ),
        )
}
