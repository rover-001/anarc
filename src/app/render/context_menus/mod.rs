//! Context menu, toolbar popover, and download toast/bubble overlays.

mod downloads_overlay;
mod plugins_menu;
mod space_folder_menu;
mod tab_menu;
mod web_menu;

use super::*;

pub(super) fn dismiss_backdrop(
    id: impl Into<gpui_kit::ElementId>,
    on_dismiss: impl Fn(&mut Anarc, &mut Window, &mut Context<Anarc>) + 'static,
    cx: &mut Context<Anarc>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .occlude()
        .absolute()
        .inset_0()
        .on_mouse_down(
            gpui_kit::MouseButton::Left,
            cx.listener(move |this, _, window, cx| {
                on_dismiss(this, window, cx);
                if this.app_page.is_none() {
                    this.focus_browser(window, cx);
                }
                cx.notify();
            }),
        )
}

pub(super) fn context_menu_item(
    id: impl Into<gpui_kit::ElementId>,
    theme: visual::SpaceTheme,
    icon: IconName,
    label: impl Into<gpui_kit::SharedString>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .px_2()
        .py_2()
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
        .child(label.into())
}

impl Anarc {
    pub(in crate::app) fn render_context_menus(
        &mut self,
        theme: visual::SpaceTheme,
        tabs: &[(
            String,
            String,
            Option<std::sync::Arc<gpui_kit::RenderImage>>,
        )],
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        let mut overlays = Vec::new();
        if let Some(el) = self.render_plugins_menu(theme, cx) {
            overlays.push(el);
        }
        let downloads = self.engine.downloads();
        if let Some(el) = self.render_download_toast(theme, &downloads, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_downloads_bubble(theme, &downloads, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_sidebar_context_menu(theme, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_theme_editor_overlay(theme, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_space_context_menu(theme, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_tab_context_menu(theme, tabs, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_folder_context_menu(theme, cx) {
            overlays.push(el);
        }
        if let Some(el) = self.render_web_context_menu(theme, cx) {
            overlays.push(el);
        }
        overlays
    }
}
