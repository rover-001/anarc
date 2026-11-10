use super::super::*;

impl Lumen {
    pub(crate) fn command_items(&self, query: &str) -> Vec<PaletteItem> {
        let tabs: Vec<_> = self
            .spaces
            .iter()
            .flat_map(|space| {
                space.tabs.iter().map(move |tab| PaletteTab {
                    space_id: &space.id,
                    space_name: &space.name,
                    tab_id: &tab.id,
                    title: &tab.title,
                    url: &tab.url,
                })
            })
            .collect();
        let history = self
            .plugins
            .service(crate::plugin::HISTORY)
            .expect("core history service is mounted")
            .search(query, 20);
        let registered = self
            .plugins
            .contributions()
            .into_iter()
            .filter(|item| item.key == "palette.action")
            .map(|item| item.value)
            .collect::<std::collections::HashSet<_>>();
        let actions = crate::plugin::PALETTE_COMMANDS
            .iter()
            .copied()
            .filter(|command| registered.contains(command.id))
            .collect::<Vec<_>>();
        let plugins = self.plugins.rows();
        command_palette::rank_items(query, &tabs, &history, &actions, &plugins)
    }
    pub(in crate::app) fn open_command_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_suggestions = false;
        self.show_find = false;
        self.show_command_palette = true;
        self.selected_command = 0;
        self.browser_focused = false;
        self.engine.set_focus(self.active_tab_id(), false);
        self.command_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
    }
    pub(in crate::app) fn close_command_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette = false;
        self.focus_browser(window, cx);
    }
    pub(crate) fn execute_palette_item(
        &mut self,
        item: PaletteItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette = false;
        match item.target {
            PaletteTarget::Tab { space_id, tab_id } => {
                let valid = if let Some(space) =
                    self.spaces.iter_mut().find(|space| space.id == space_id)
                {
                    if space.tabs.iter().any(|tab| tab.id == tab_id) {
                        space.active = Some(tab_id.clone());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if valid {
                    self.emit_plugin_text("tab.activated", tab_id);
                    self.activate_space_immediate(&space_id, cx);
                }
            }
            PaletteTarget::History(url) => self.navigate_to(&url, window, cx),
            PaletteTarget::PluginToggle { id, enabled } => {
                if let Err(error) = self.set_plugin_enabled(&id, enabled) {
                    eprintln!("Failed to update plugin {id}: {error}");
                }
            }
            PaletteTarget::Action(action) => match action {
                PaletteAction::NewTab => self.add_tab(),
                PaletteAction::CloseTab => self.close_active_tab(),
                PaletteAction::ReopenClosedTab => self.reopen_closed_tab(),
                PaletteAction::Reload => self.reload_active(false),
                PaletteAction::FindInPage => {
                    self.show_find = true;
                    self.find_input
                        .update(cx, |input, cx| input.focus(window, cx));
                    self.engine.set_focus(self.active_tab_id(), false);
                }
                PaletteAction::ZoomIn => self.change_zoom(0.1),
                PaletteAction::ZoomOut => self.change_zoom(-0.1),
                PaletteAction::ZoomReset => self.change_zoom(0.0),
                PaletteAction::NewSpace => self.open_create_space_modal(window, cx),
                PaletteAction::EnablePlugin | PaletteAction::DisablePlugin => {
                    self.app_page = Some(AppPage::Settings);
                    self.settings_section = SettingsSection::Plugins;
                    self.plugins_menu_open = false;
                }
                PaletteAction::ReloadPlugins => {
                    if let Err(error) = self.reload_plugins() {
                        eprintln!("Failed to reload plugins: {error}");
                    }
                }
                PaletteAction::OpenPluginFolder => {
                    if let Err(error) = self.open_plugin_folder() {
                        eprintln!("Failed to open plugin folder: {error}");
                    }
                }
            },
        }
        cx.notify();
    }
    pub(in crate::app) fn move_command_selection(&mut self, down: bool, count: usize) {
        if count == 0 {
            self.selected_command = 0;
            return;
        }
        self.selected_command = if down {
            (self.selected_command + 1) % count
        } else {
            (self.selected_command + count - 1) % count
        };
    }
}
