//! Web page context menu overlay (copy, save link, paste, select all, nav).

use super::dismiss_backdrop;
use super::super::*;

const COPY_SELECTION_FALLBACK_SCRIPT: &str = r#"(function() {
    try {
        function getSel() {
            let text = window.getSelection ? window.getSelection().toString() : '';
            if (text) return text;
            let el = document.activeElement;
            while (el && el.shadowRoot && el.shadowRoot.activeElement) {
                el = el.shadowRoot.activeElement;
            }
            if (el && 'selectionStart' in el && 'selectionEnd' in el && typeof el.value === 'string') {
                const start = Math.min(el.selectionStart, el.selectionEnd);
                const end = Math.max(el.selectionStart, el.selectionEnd);
                if (end > start) return el.value.substring(start, end);
            }
            return '';
        }
        const s = getSel();
        if (s) {
            console.log("[ANARC_CB_COPY]" + encodeURIComponent(s));
        }
    } catch (_) {}
})();"#;

fn web_menu_item(
    id: impl Into<gpui_kit::ElementId>,
    theme: visual::SpaceTheme,
    icon: IconName,
    label: &'static str,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .px_2()
        .py_1p5()
        .rounded_md()
        .text_sm()
        .text_color(rgb(theme.text))
        .hover(move |style| {
            style.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x40,
            ))
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .child(Icon::new(icon))
        .child(label)
}

impl Anarc {
    pub(super) fn render_web_context_menu(
        &mut self,
        theme: visual::SpaceTheme,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let menu = self.web_context_menu.as_ref()?;
        let menu_tab_id = menu.tab_id.clone();
        let menu_x = menu.x;
        let menu_y = menu.y;
        let download_url = menu.download_url.clone();

        let menu_layer = dismiss_backdrop(
            "web-context-dismiss-layer",
            |this, _, _| {
                this.web_context_menu = None;
            },
            cx,
        )
        .on_mouse_down(
            gpui_kit::MouseButton::Right,
            cx.listener(|this, _, window, cx| {
                this.web_context_menu = None;
                if this.app_page.is_none() {
                    this.focus_browser(window, cx);
                }
                cx.notify();
            }),
        );

        let mut panel = div()
            .id("web-context-menu-panel")
            .occlude()
            .absolute()
            .left(px(menu_x))
            .top(px(menu_y))
            .w(px(180.0 * UI_SCALE))
            .bg(rgb(visual::panel_surface(theme.surface)))
            .border_1()
            .border_color(rgb(theme.border))
            .rounded_lg()
            .shadow_lg()
            .p_1()
            .flex()
            .flex_col()
            .gap_0p5();

        let copy_id = menu_tab_id.clone();
        panel = panel.child(
            web_menu_item("ctx-web-copy", theme, IconName::Copy, "Copy").on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(text) = this.engine.get_selected_text(&copy_id) {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.clone()));
                        crate::engine::clipboard::write_system_clipboard(&text);
                    }
                    this.engine.copy(&copy_id);
                    this.engine
                        .execute_script(&copy_id, COPY_SELECTION_FALLBACK_SCRIPT);
                    this.web_context_menu = None;
                    cx.notify();
                },
            )),
        );

        if let Some(url) = download_url {
            let save_id = menu_tab_id.clone();
            panel = panel.child(
                web_menu_item(
                    "ctx-web-save-link",
                    theme,
                    IconName::Download,
                    "Save link as...",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.engine.start_download(&save_id, &url);
                    this.web_context_menu = None;
                    cx.notify();
                })),
            );
        }

        let paste_id = menu_tab_id.clone();
        let select_all_id = menu_tab_id.clone();
        let reload_id = menu_tab_id.clone();
        let back_id = menu_tab_id.clone();
        let fwd_id = menu_tab_id;

        panel = panel
            .child(
                web_menu_item("ctx-web-paste", theme, IconName::Clipboard, "Paste").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.engine.paste(&paste_id);
                        this.web_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                web_menu_item("ctx-web-select-all", theme, IconName::Check, "Select All")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.engine.select_all(&select_all_id);
                        this.web_context_menu = None;
                        cx.notify();
                    })),
            )
            .child(div().h(px(1.0)).mx_1().my_0p5().bg(rgb(theme.border)))
            .child(
                web_menu_item("ctx-web-reload", theme, IconName::RotateCw, "Reload").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.engine.reload(&reload_id, false);
                        this.web_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                web_menu_item("ctx-web-back", theme, IconName::ArrowLeft, "Back").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.engine.go_back(&back_id);
                        this.web_context_menu = None;
                        cx.notify();
                    }),
                ),
            )
            .child(
                web_menu_item("ctx-web-forward", theme, IconName::ArrowRight, "Forward").on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.engine.go_forward(&fwd_id);
                        this.web_context_menu = None;
                        cx.notify();
                    }),
                ),
            );

        Some(menu_layer.child(panel).into_any_element())
    }
}
