use super::super::*;
#[cfg(not(test))]
use gpui_kit::{
    component::{WindowExt, button::Button},
    rems,
};

impl Anarc {
    #[cfg(not(test))]
    pub(in crate::app) fn choose_live_share_space(
        &mut self,
        request: engine::LiveShareSpaceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.app_page.is_some()
            || self.focused_tab_id() != request.tab_id()
            || !request.is_current()
            || window.has_active_dialog(cx)
        {
            return;
        }

        let choices: Vec<_> = self
            .spaces
            .iter()
            .filter(|space| !space.name.starts_with("[Shared]") && !space.id.starts_with("shared-"))
            .map(|space| {
                (
                    space.id.clone(),
                    space.name.clone(),
                    shareable_web_tabs(space).count(),
                )
            })
            .collect();
        let owner = cx.weak_entity();
        let request = std::rc::Rc::new(request);
        window.open_dialog(cx, move |dialog, _, _| {
            let spaces = div()
                .id("live-share-spaces")
                .max_h(rems(18.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .children(choices.iter().map(|(id, name, count)| {
                    let id = id.clone();
                    let owner = owner.clone();
                    let request = request.clone();
                    let noun = if *count == 1 { "tab" } else { "tabs" };
                    Button::new(gpui_kit::SharedString::from(format!("live-share-space-{id}")))
                        .outline()
                        .w_full()
                        .label(format!("{name} · {count} web {noun}"))
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |this, _| {
                                if this.focused_tab_id() == request.tab_id()
                                    && let Some(space) = this.spaces.iter().find(|space| space.id == id)
                                {
                                    request.send_selection(&live_share_selection(space));
                                }
                            });
                            window.close_dialog(cx);
                        })
                }));
            dialog
                .title("Choose a space to share")
                .child(
                    div().flex().flex_col().gap_3().child(
                        "Live Share will share the current web tab titles and URLs from the space you choose.",
                    ).child(spaces),
                )
                .footer(Button::new("cancel-live-share-space").label("Cancel").on_click(
                    |_, window, cx| window.close_dialog(cx),
                ))
        });
    }

    /// Opens an enabled panel as a regular browser tab.
    #[allow(dead_code)]
    pub(crate) fn open_plugin_panel(&mut self, id: &str) -> Result<(), String> {
        let row = self
            .plugins
            .rows()
            .into_iter()
            .find(|row| {
                row.descriptor.id == id && row.descriptor.kind == crate::plugin::PluginKind::Panel
            })
            .ok_or_else(|| format!("unknown panel plugin: {id}"))?;
        if !row.enabled || !self.plugins.is_active(id) {
            return Err(format!("panel is not enabled: {id}"));
        }
        if id == "com.anarc.live-share" {
            super::ensure_live_share_daemon_running();
        }
        let registered = self
            .panel_registry
            .lock()
            .map_err(|_| "panel registry is unavailable".to_owned())?
            .contains_key(id);
        if !registered {
            return Err(format!("panel package is not registered: {id}"));
        }

        let space_id = self.active_space.clone();
        let space = self
            .spaces
            .iter_mut()
            .find(|space| space.id == space_id)
            .ok_or_else(|| "active space is unavailable".to_owned())?;
        let tab_id = next_tab_id(space);
        let title = row.descriptor.name;
        let url = format!("anarc-plugin://{id}/");
        let mut tab = Tab::new(&tab_id, title, &url);
        tab.state = crate::model::TabState::Live;
        space.tabs.push(tab);
        space.active = Some(tab_id.clone());
        self.added_tab = Some((space_id, tab_id.clone(), Instant::now()));
        self.show_suggestions = false;
        self.address_editing = false;
        self.reset_address_for_tab_change = true;
        self.emit_plugin_text("tab.created", tab_id);
        self.save_state();
        Ok(())
    }

    /// Close CEF renderers for every tab owned by the panel before its package
    /// is removed from the scheme registry.
    pub(crate) fn stop_panel_browsers(&mut self, id: &str) {
        let anarc_prefix = format!("anarc-plugin://{id}/");
        let lumen_prefix = format!("lumen-plugin://{id}/");
        for space in &self.spaces {
            for tab in &space.tabs {
                if tab.url.starts_with(&anarc_prefix) || tab.url.starts_with(&lumen_prefix) {
                    self.engine.close_browser(&tab.id);
                }
            }
        }
    }
}

fn shareable_web_tabs(space: &Space) -> impl Iterator<Item = &Tab> {
    space.tabs.iter().filter(|tab| {
        tab.url.split_once("://").is_some_and(|(scheme, rest)| {
            matches!(scheme, "http" | "https")
                && rest
                    .split(['/', '?', '#'])
                    .next()
                    .is_some_and(|authority| !authority.is_empty() && !authority.contains('@'))
        })
    })
}

fn live_share_selection(space: &Space) -> serde_json::Value {
    let tabs: Vec<_> = shareable_web_tabs(space)
        .map(|tab| serde_json::json!({ "id": tab.id, "title": tab.title, "url": tab.url }))
        .collect();
    serde_json::json!({ "id": space.id, "name": space.name, "tabs": tabs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_share_selection_contains_only_web_tabs_from_the_selected_space() {
        let mut space = Space::new("work", "Work", "briefcase", 0);
        space.tabs = vec![
            Tab::new("1", "Notes", "https://example.com/notes"),
            Tab::new("2", "Local file", "file:///private.txt"),
            Tab::new("3", "Live Share", "anarc-plugin://com.anarc.live-share/"),
            Tab::new("4", "Login", "https://name:secret@example.com/"),
        ];
        assert_eq!(
            live_share_selection(&space),
            serde_json::json!({
                "id": "work",
                "name": "Work",
                "tabs": [{ "id": "1", "title": "Notes", "url": "https://example.com/notes" }]
            })
        );
    }
}
