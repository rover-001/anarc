use super::super::*;

impl Lumen {
    pub(in crate::app) fn command_items(&self, query: &str) -> Vec<PaletteItem> {
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
        let history = history::search(query, 20);
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
        command_palette::rank_items(query, &tabs, &history, &actions)
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
    pub(in crate::app) fn execute_palette_item(
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
                        space.active = Some(tab_id);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if valid {
                    self.activate_space_immediate(&space_id, cx);
                }
            }
            PaletteTarget::History(url) => self.navigate_to(&url, window, cx),
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
