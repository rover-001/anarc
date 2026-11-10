use super::super::*;

impl Lumen {
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
        let url = format!("lumen-plugin://{id}/");
        space.tabs.push(Tab::new(&tab_id, title, &url));
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
        let prefix = format!("lumen-plugin://{id}/");
        for space in &self.spaces {
            for tab in &space.tabs {
                if tab.url.starts_with(&prefix) {
                    self.engine.close_browser(&tab.id);
                }
            }
        }
    }
}
