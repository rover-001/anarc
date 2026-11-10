// The plugin API is introduced incrementally; later phases consume these hooks.
#![allow(dead_code)]

mod config;
mod context;
mod core;
mod distribution;
mod event;
mod fiber;
mod host;
mod kernel;
mod manager_model;
mod manifest;
mod pack_plugin;
mod packs;
mod panel;
mod panel_plugin;
mod registry;
mod services;
#[cfg(feature = "wasm-plugins")]
mod wasm_host;

#[cfg(test)]
mod tests;

pub(crate) use config::PluginConfig;
pub(crate) use context::{PluginCtx, ServiceKey};
pub(crate) use core::{CorePlugin, PALETTE_COMMANDS, PaletteCommand, SHORTCUT_BINDINGS};
pub(crate) use distribution::{inspect_package, install_package};
pub(crate) use event::{EventMode, EventPayload};
pub(crate) use host::PluginHost;
pub(crate) use kernel::Kernel;
pub(crate) use manager_model::{
    PluginDescriptor, PluginKind, PluginManagerModel, PluginRow, RuntimeStatus, ToggleAction,
};
pub(crate) use manifest::{Manifest, Plugin, PluginError};
#[cfg(not(test))]
pub(crate) use pack_plugin::{MatchedPageAssetKind, matched_page_assets};
pub(crate) use pack_plugin::{active_layout_preset, active_theme_overrides};
pub(crate) use pack_plugin::{register_packs, validate_pack_directory};
pub(crate) use packs::{LayoutPack, ThemePack};
pub(crate) use panel::{
    PanelPackage, PanelRegistryHandle, PanelRequest, PanelResponse, new_panel_registry,
};
pub(crate) use panel_plugin::register_panel_plugins;
pub(crate) use registry::{Contribution, Order};
pub(crate) use services::{
    COMMANDS, CommandsService, ENGINE, EngineService, HISTORY, HistoryService, KEYMAP,
    KeymapService, NAVIGATION, NavigationService, SETTINGS, SPACES, SettingsService, SpacesService,
    TABS, THEME, TabsService, ThemeService,
};
#[cfg(feature = "wasm-plugins")]
pub(crate) use wasm_host::register_wasm_plugins;

#[cfg(test)]
pub(crate) use event::EventBus;
#[cfg(test)]
pub(crate) use fiber::FiberState;
