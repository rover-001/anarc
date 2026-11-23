//! Plugins settings view. The app owns plugin lifecycle and persistence; this
//! module only renders rows and reports user actions to that host.

use std::{cell::Cell, collections::HashSet, rc::Rc};

use gpui_kit::{AnimationExt as _, App, ClickEvent, IntoElement, Window, div, prelude::*, px, rgb};

use crate::{
    plugin::{PluginKind, PluginRow, RuntimeStatus},
    ui::{
        motion,
        visual::{SpaceTheme, UI_SCALE},
    },
};

pub(crate) type ToggleCallback = Rc<dyn Fn(String, bool, &ClickEvent, &mut Window, &mut App)>;
pub(crate) type ActionCallback = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
pub(crate) type PluginActionCallback = Rc<dyn Fn(String, &ClickEvent, &mut Window, &mut App)>;
pub(crate) type DetailsCallback = Rc<dyn Fn(String, &ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallSource {
    Folder,
    Package,
}

pub(crate) type InstallCallback = Rc<dyn Fn(InstallSource, &ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PluginDetails {
    /// Text summary of contributed settings, or `None` when the plugin has no
    /// configurable settings or the host has not loaded its schema.
    pub(crate) configuration: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginReview {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: InstallSource,
    pub(crate) requested_permissions: Vec<String>,
    pub(crate) signature_valid: bool,
    pub(crate) signature_trusted: bool,
    pub(crate) signer_fingerprint: Option<String>,
}

/// Permission review shown before an external plugin package is installed.
/// Installing leaves the plugin disabled; enabling remains a separate action.
pub(crate) fn render_permission_review(
    review: &PluginReview,
    theme: SpaceTheme,
    on_cancel: ActionCallback,
    on_install: ActionCallback,
) -> gpui_kit::AnyElement {
    let permissions = if review.requested_permissions.is_empty() {
        vec!["No additional permissions requested".to_owned()]
    } else {
        review.requested_permissions.clone()
    };
    let mut permission_list = div().flex().flex_col().gap_1();
    for permission in permissions {
        permission_list = permission_list.child(
            div()
                .text_xs()
                .text_color(rgb(theme.text))
                .child(format!("• {permission}")),
        );
    }

    let signer = review
        .signer_fingerprint
        .as_ref()
        .map(|fingerprint| format!("Signer key: {fingerprint}"))
        .unwrap_or_else(|| "No signer key is available".to_owned());
    let (signature_status, install_label, can_install) = match review.source {
        InstallSource::Folder => (
            "Local folder is unsigned; review its contents before installing.",
            "Install disabled plugin",
            true,
        ),
        InstallSource::Package if !review.signature_valid => (
            "Package signature is missing or invalid.",
            "Valid signature required",
            false,
        ),
        InstallSource::Package if review.signature_trusted => (
            "Signature verified with a trusted signer.",
            "Install disabled plugin",
            true,
        ),
        InstallSource::Package => (
            "Signature is valid but this signer is not yet trusted. Installing trusts this signer for this package.",
            "Trust signer and install",
            true,
        ),
    };

    div()
        .w_full()
        .max_w(px(520.0))
        .mx_auto()
        .p_5()
        .rounded_xl()
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
        .border_1()
        .border_color(rgb(theme.border))
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child("Review plugin permissions"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.text))
                        .child(format!("{} · v{}", review.name, review.version)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(review.id.clone()),
                ),
        )
        .child(
            div()
                .w_full()
                .p_3()
                .rounded_xl()
                .bg(rgb(theme.base))
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child("Requested access"),
                )
                .child(permission_list),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.text))
                        .child(signature_status),
                )
                .child(div().text_xs().text_color(rgb(theme.muted)).child(signer))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("The plugin will remain disabled until you enable it."),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .gap_2()
                .child(action_button("Cancel", theme, on_cancel))
                .child(if can_install {
                    action_button(install_label, theme, on_install).into_any_element()
                } else {
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(install_label)
                        .into_any_element()
                }),
        )
        .into_any_element()
}

/// Renders the approved Plugins settings layout using existing theme tokens.
///
/// `on_toggle` receives the plugin id and requested enabled state. The host
/// must apply the returned action to the plugin kernel and persist config.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    rows: &[PluginRow],
    details: &[(&str, PluginDetails)],
    feedback: Option<&str>,
    theme: SpaceTheme,
    on_toggle: ToggleCallback,
    on_remove: PluginActionCallback,
    on_install: InstallCallback,
    on_open_plugin_folder: ActionCallback,
    on_open: PluginActionCallback,
    expanded: &HashSet<String>,
    on_toggle_details: DetailsCallback,
) -> gpui_kit::AnyElement {
    // Stable, state-independent order: alphabetical by display name. Disabling a
    // plugin must never move its row. Live Share no longer gets a top slot.
    let mut sorted_rows = rows.to_vec();
    sorted_rows.sort_by(|a, b| {
        a.descriptor
            .name
            .to_lowercase()
            .cmp(&b.descriptor.name.to_lowercase())
            .then_with(|| a.descriptor.id.cmp(&b.descriptor.id))
    });
    let mut list = div().w_full().flex().flex_col().gap_2();
    for row in &sorted_rows {
        let detail = details
            .iter()
            .find(|(id, _)| *id == row.descriptor.id)
            .map(|(_, details)| details.clone())
            .unwrap_or_default();
        list = list.child(plugin_row(
            row,
            detail,
            theme,
            on_toggle.clone(),
            on_remove.clone(),
            on_open.clone(),
            expanded.contains(&row.descriptor.id),
            on_toggle_details.clone(),
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child("Manage installed plugins and built-in features"),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(action_button("Install from folder…", theme, {
                            let on_install = on_install.clone();
                            Rc::new(move |event, window, app| {
                                on_install(InstallSource::Folder, event, window, app);
                            })
                        }))
                        .child(action_button("Install package…", theme, {
                            let on_install = on_install.clone();
                            Rc::new(move |event, window, app| {
                                on_install(InstallSource::Package, event, window, app);
                            })
                        })),
                ),
        )
        .children(feedback.map(|message| {
            div()
                .w_full()
                .text_xs()
                .text_color(rgb(theme.text))
                .child(message.to_owned())
        }))
        .child(list)
        .child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .pt_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .child(action_button(
                    "Open plugin folder",
                    theme,
                    on_open_plugin_folder,
                )),
        )
        .into_any_element()
}

fn plugin_row(
    row: &PluginRow,
    details: PluginDetails,
    theme: SpaceTheme,
    on_toggle: ToggleCallback,
    on_remove: PluginActionCallback,
    on_open: PluginActionCallback,
    expanded: bool,
    on_toggle_details: DetailsCallback,
) -> impl IntoElement {
    let id = row.descriptor.id.clone();
    let plugin_id = id.clone();
    let requested_state = !row.enabled;
    let can_toggle = row.descriptor.can_toggle;
    // Child controls run their click handler before the card's bubbling handler.
    // This flag keeps clicks on the switch / Open / Remove from also expanding.
    let handled_by_control = Rc::new(Cell::new(false));
    let control = if can_toggle {
        let callback = on_toggle.clone();
        let handled = handled_by_control.clone();
        let toggle_id = format!("plugin-toggle-{id}");
        let debug_id = format!("plugin-toggle-{id}");
        div()
            .id(toggle_id)
            .debug_selector(move || debug_id.clone())
            .child(toggle_switch(
                format!("plugin-switch-{id}"),
                row.enabled,
                theme,
                move |event, window, app| {
                    handled.set(true);
                    callback(id.clone(), requested_state, event, window, app)
                },
            ))
            .into_any_element()
    } else {
        div()
            .px_3()
            .py_1p5()
            .rounded_md()
            .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
            .text_xs()
            .font_weight(gpui_kit::FontWeight::MEDIUM)
            .text_color(rgb(theme.muted))
            .flex_shrink_0()
            .child("Required")
            .into_any_element()
    };

    let permission_summary = if row.descriptor.permissions.is_empty() {
        "No permissions requested".to_owned()
    } else {
        format!("Permissions: {}", row.descriptor.permissions.join(", "))
    };
    let configuration = details
        .configuration
        .unwrap_or_else(|| "No configurable settings".to_owned());
    let details_callback_id = plugin_id.clone();
    let handled_by_card = handled_by_control.clone();

    div()
        .w_full()
        .id(format!("plugin-card-click-{plugin_id}"))
        .debug_selector(|| format!("plugin-card-{plugin_id}"))
        .cursor_pointer()
        .hover(|style| {
            style.bg(crate::ui::visual::translucent_surface(
                theme.surface_hover,
                0x48,
            ))
        })
        .on_click(move |event, window, app| {
            if handled_by_card.replace(false) {
                return;
            }
            on_toggle_details(details_callback_id.clone(), event, window, app);
        })
        .p_4()
        .rounded_xl()
        .bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
        .border_1()
        .border_color(rgb(theme.border))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.text))
                                .child(if row.descriptor.id == "lumen.core" {
                                    "Anarc Core".to_string()
                                } else {
                                    row.descriptor.name.clone()
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child(kind_label(row.descriptor.kind)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child(format!("v{}", row.descriptor.version)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_shrink_0()
                        .debug_selector(|| format!("plugin-actions-{plugin_id}"))
                        .child(if row.descriptor.kind == crate::plugin::PluginKind::Panel && row.enabled {
                            let callback = on_open.clone();
                            let handled = handled_by_control.clone();
                            let plugin_id = plugin_id.clone();
                            div()
                                .id(format!("plugin-open-{plugin_id}"))
                                .debug_selector(|| format!("plugin-open-{plugin_id}"))
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
                                .text_xs()
                                .font_weight(gpui_kit::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .cursor_pointer()
                                .flex_shrink_0()
                                .active(|style| style.opacity(0.88))
                                .hover(move |style| style.bg(rgb(theme.accent)).text_color(rgb(theme.base)))
                                .child("Open")
                                .on_click(move |event, window, app| {
                                    handled.set(true);
                                    callback(plugin_id.clone(), event, window, app);
                                })
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(if can_remove(row) {
                            let callback = on_remove.clone();
                            let handled = handled_by_control.clone();
                            div()
                                .id(format!("plugin-remove-{plugin_id}"))
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
                                .text_xs()
                                .font_weight(gpui_kit::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .cursor_pointer()
                                .flex_shrink_0()
                                .active(|style| style.opacity(0.88))
                                .hover(|style| style.bg(rgb(0xef4444)).text_color(rgb(0xffffff)))
                                .child("Remove")
                                .on_click(move |event, window, app| {
                                    handled.set(true);
                                    callback(plugin_id.clone(), event, window, app);
                                })
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(control),
                ),
        )
        .when(expanded, |card| {
            card.child(
                div()
                    .w_full()
                    .pt_2()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(rgb(theme.muted))
                            .child(row.descriptor.description.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child(format!("Status: {}", runtime_label(&row.runtime))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child(permission_summary),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child(format!("Configuration: {configuration}")),
                    )
                    .child(if let Some(error) = details.error.clone() {
                        div()
                            .text_xs()
                            .text_color(rgb(theme.text))
                            .child(format!("Error: {error}"))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }),
            )
        })
}

/// Compact on/off switch matching the toggle used by the Extensions menu.
/// The thumb and track share a spring so a flip reads as a state change
/// instead of an instant jump, and rapid re-toggles keep their momentum.
pub(crate) fn toggle_switch(
    id: String,
    checked: bool,
    theme: SpaceTheme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui_kit::AnyElement {
    let spring_id = format!("toggle-spring-{id}");
    div()
        .id(id)
        .flex_shrink_0()
        .w(px(34.0 * UI_SCALE))
        .h(px(20.0 * UI_SCALE))
        .rounded_full()
        .flex()
        .items_center()
        .justify_start()
        .px(px(2.0 * UI_SCALE))
        .cursor_pointer()
        .hover(|style| style.opacity(0.92))
        .active(|style| style.opacity(0.88))
        .on_click(on_click)
        .with_spring(
            spring_id,
            motion::toggle().to(if checked { 1.0 } else { 0.0 }),
            move |track, value| {
                let color_progress = motion::clamp_percentage(value);
                // Let the thumb keep the spring's small overshoot; clamp only
                // the colour channels so they always land inside the palette.
                let offset_progress = if value.is_finite() {
                    value.clamp(0.0, 1.1)
                } else {
                    0.0
                };
                track
                    .bg(rgb(mix_color(theme.border, theme.accent, color_progress)))
                    .child(
                        div()
                            .size(px(16.0 * UI_SCALE))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .ml(px(14.0 * UI_SCALE * offset_progress)),
                    )
            },
        )
        .into_any_element()
}

fn mix_color(from: u32, to: u32, progress: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((from >> shift) & 0xff_u32) as f32;
        let b = ((to >> shift) & 0xff_u32) as f32;
        (a + (b - a) * progress).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn action_button(
    label: &'static str,
    theme: SpaceTheme,
    callback: ActionCallback,
) -> impl IntoElement {
    div()
        .id(format!("plugin-manager-action-{label}"))
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(crate::ui::visual::translucent_surface(theme.surface_hover, 0x40))
        .text_xs()
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_color(rgb(theme.text))
        .cursor_pointer()
        .flex_shrink_0()
        .active(|style| style.opacity(0.88))
        .hover(|style| style.bg(rgb(theme.accent)).text_color(rgb(theme.base)))
        .child(label)
        .on_click(move |event, window, app| callback(event, window, app))
}

fn kind_label(kind: PluginKind) -> &'static str {
    match kind {
        PluginKind::Core => "Core",
        PluginKind::Builtin => "Built-in",
        PluginKind::Pack => "Pack",
        PluginKind::Wasm => "WASM",
        PluginKind::Panel => "Panel",
    }
}

fn can_remove(row: &PluginRow) -> bool {
    matches!(
        row.descriptor.kind,
        PluginKind::Pack | PluginKind::Wasm | PluginKind::Panel
    )
}

fn runtime_label(status: &RuntimeStatus) -> String {
    match status {
        RuntimeStatus::Pending => "Pending".to_owned(),
        RuntimeStatus::Loading => "Loading".to_owned(),
        RuntimeStatus::Active => "Active".to_owned(),
        RuntimeStatus::Disposed => "Disabled".to_owned(),
        RuntimeStatus::Failed(reason) => format!("Failed: {reason}"),
    }
}
