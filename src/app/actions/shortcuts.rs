use super::super::*;

impl Lumen {
    pub(crate) fn cycle_tab(&mut self, reverse: bool) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        if space.tabs.len() < 2 {
            return;
        }
        let current = space
            .active
            .as_ref()
            .and_then(|id| space.tabs.iter().position(|tab| &tab.id == id))
            .unwrap_or(0);
        let next = if reverse {
            (current + space.tabs.len() - 1) % space.tabs.len()
        } else {
            (current + 1) % space.tabs.len()
        };
        let active_id = space.tabs[next].id.clone();
        space.active = Some(active_id.clone());
        let _ = space;
        self.close_peek();
        self.close_app_page();
        self.plugins_menu_open = false;
        self.emit_plugin_text("tab.activated", active_id);
        self.save_state();
    }
    pub(crate) fn activate_tab_number(&mut self, number: usize) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let index = if number == 9 {
            space.tabs.len().checked_sub(1)
        } else {
            number.checked_sub(1)
        };
        if let Some(tab) = index.and_then(|index| space.tabs.get(index)) {
            let active_id = tab.id.clone();
            space.active = Some(active_id.clone());
            let _ = space;
            self.close_peek();
            self.close_app_page();
            self.plugins_menu_open = false;
            self.emit_plugin_text("tab.activated", active_id);
            self.save_state();
        }
    }
    pub(crate) fn is_app_shortcut(key: &str, control: bool, shift: bool) -> bool {
        if !control {
            return false;
        }
        if !shift && key == "s" {
            return true;
        }
        if !shift && key.len() == 1 && matches!(key.as_bytes()[0], b'1'..=b'9') {
            return true;
        }
        let shortcut = format!("ctrl+{}{}", if shift { "shift+" } else { "" }, key);
        crate::plugin::SHORTCUT_BINDINGS
            .iter()
            .any(|binding| binding.key == shortcut)
    }

    fn registered_shortcut_action(&self, key: &str, control: bool, shift: bool) -> Option<String> {
        if !control {
            return None;
        }
        if !shift && key.len() == 1 && matches!(key.as_bytes()[0], b'1'..=b'9') {
            return Some("activate-tab-number".to_string());
        }
        let shortcut = format!("ctrl+{}{}", if shift { "shift+" } else { "" }, key);
        self.plugins
            .contributions()
            .into_iter()
            .filter(|item| item.key == "keymap.binding")
            .find_map(|item| {
                let (registered_key, action) = item.value.rsplit_once('=')?;
                (registered_key == shortcut).then(|| action.to_string())
            })
    }
    pub(crate) fn handle_shortcut(&mut self, key: &str, control: bool, shift: bool) -> bool {
        let Some(action) = self.registered_shortcut_action(key, control, shift) else {
            return false;
        };
        match action.as_str() {
            "toggle-shield" => self.toggle_shield_for_active_tab(),
            "reopen-closed-tab" => self.reopen_closed_tab(),
            "new-tab" => self.add_tab(),
            "close-tab" => self.close_active_tab(),
            "next-tab" => self.cycle_tab(false),
            "previous-tab" => self.cycle_tab(true),
            "activate-tab-number" => self.activate_tab_number(key.parse().unwrap_or(1)),
            "reload" => {
                self.close_app_page();
                self.reload_active(false);
            }
            "hard-reload" => {
                self.close_app_page();
                self.reload_active(true);
            }
            "zoom-in" => self.change_zoom(0.1),
            "zoom-out" => self.change_zoom(-0.1),
            "zoom-reset" => self.change_zoom(0.0),
            _ => return false,
        }
        true
    }
}
