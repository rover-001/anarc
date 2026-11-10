use super::ServiceKey;
use crate::{model::Space, settings::BrowserPreferences};
use std::collections::HashMap;

pub(crate) const HISTORY: ServiceKey<HistoryService> = ServiceKey::new("history");
pub(crate) const SETTINGS: ServiceKey<SettingsService> = ServiceKey::new("settings");
pub(crate) const NAVIGATION: ServiceKey<NavigationService> = ServiceKey::new("navigation");
pub(crate) const TABS: ServiceKey<TabsService> = ServiceKey::new("tabs");
pub(crate) const SPACES: ServiceKey<SpacesService> = ServiceKey::new("spaces");
pub(crate) const ENGINE: ServiceKey<EngineService> = ServiceKey::new("engine");
pub(crate) const COMMANDS: ServiceKey<CommandsService> = ServiceKey::new("commands");
pub(crate) const KEYMAP: ServiceKey<KeymapService> = ServiceKey::new("keymap");
pub(crate) const THEME: ServiceKey<ThemeService> = ServiceKey::new("theme");

#[derive(Clone, Copy)]
pub(crate) struct HistoryService;

impl HistoryService {
    pub(crate) fn search(&self, query: &str, limit: usize) -> Vec<crate::history::HistoryEntry> {
        crate::history::search(query, limit)
    }

    pub(crate) fn clear(&self) {
        crate::history::clear_browsing_history();
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SettingsService;

impl SettingsService {
    pub(crate) fn load_preferences(&self) -> BrowserPreferences {
        crate::settings::load_preferences()
    }

    pub(crate) fn save_preferences(&self, preferences: &BrowserPreferences) {
        crate::settings::save_preferences(preferences);
    }

    pub(crate) fn load_zooms(&self) -> HashMap<String, f64> {
        crate::settings::load_zooms()
    }

    pub(crate) fn save_zooms(&self, zooms: &HashMap<String, f64>) {
        crate::settings::save_zooms(zooms);
    }

    pub(crate) fn url_host(&self, url: &str) -> String {
        crate::settings::url_host(url)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct NavigationService;

impl NavigationService {
    pub(crate) fn submit_address(
        &self,
        space: &mut Space,
        input: &str,
        engine: crate::settings::SearchEngine,
    ) -> Option<(String, String, bool)> {
        crate::navigation::submit_address_with_engine(space, input, engine)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TabsService;

impl TabsService {
    pub(crate) fn add(&self, app: &mut crate::app::Lumen) {
        app.add_tab();
    }

    pub(crate) fn close_active(&self, app: &mut crate::app::Lumen) {
        app.close_active_tab();
    }

    pub(crate) fn reopen_closed(&self, app: &mut crate::app::Lumen) {
        app.reopen_closed_tab();
    }

    pub(crate) fn cycle(&self, app: &mut crate::app::Lumen, reverse: bool) {
        app.cycle_tab(reverse);
    }

    pub(crate) fn activate_number(&self, app: &mut crate::app::Lumen, number: usize) {
        app.activate_tab_number(number);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SpacesService;

impl SpacesService {
    pub(crate) fn activate(&self, app: &mut crate::app::Lumen, id: &str) {
        app.activate_space(id);
    }

    pub(crate) fn active_id<'a>(&self, app: &'a crate::app::Lumen) -> &'a str {
        &app.active_space().id
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EngineService;

impl EngineService {
    pub(crate) fn execute_script(&self, app: &crate::app::Lumen, tab_id: &str, script: &str) {
        app.engine.execute_script(tab_id, script);
    }

    pub(crate) fn reload(&self, app: &crate::app::Lumen, tab_id: &str, ignore_cache: bool) {
        app.engine.reload(tab_id, ignore_cache);
    }

    pub(crate) fn find(
        &self,
        app: &crate::app::Lumen,
        tab_id: &str,
        query: &str,
        forward: bool,
        next: bool,
    ) {
        app.engine.find(tab_id, query, forward, next);
    }

    pub(crate) fn set_zoom_level(&self, app: &crate::app::Lumen, tab_id: &str, level: f64) {
        app.engine.set_zoom_level(tab_id, level);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CommandsService;

impl CommandsService {
    pub(crate) fn items(
        &self,
        app: &crate::app::Lumen,
        query: &str,
    ) -> Vec<crate::ui::command_palette::PaletteItem> {
        app.command_items(query)
    }

    pub(crate) fn execute(
        &self,
        app: &mut crate::app::Lumen,
        item: crate::ui::command_palette::PaletteItem,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<crate::app::Lumen>,
    ) {
        app.execute_palette_item(item, window, cx);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct KeymapService;

impl KeymapService {
    pub(crate) fn is_app_shortcut(&self, key: &str, control: bool, shift: bool) -> bool {
        crate::app::Lumen::is_app_shortcut(key, control, shift)
    }

    pub(crate) fn dispatch(
        &self,
        app: &mut crate::app::Lumen,
        key: &str,
        control: bool,
        shift: bool,
    ) -> bool {
        app.handle_shortcut(key, control, shift)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ThemeService;

impl ThemeService {
    pub(crate) fn current(&self, app: &crate::app::Lumen) -> crate::ui::visual::SpaceTheme {
        crate::ui::visual::theme_for_space(app.theme_picker_space())
    }
}
