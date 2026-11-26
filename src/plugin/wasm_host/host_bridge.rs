//! Host API types, WIT `Host` trait bridge, and Wasmtime `StoreState`.

use std::sync::Arc;

use wasmtime::StoreLimits;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use super::lumen;
use super::permissions::{Capability, PermissionGrants};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRecord {
    pub id: String,
    pub title: String,
    pub url: String,
    pub active: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceRecord {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRecord {
    pub url: String,
    pub title: String,
    pub visits: u64,
    pub last_visited_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostRequest {
    ListTabs,
    OpenTab {
        url: String,
    },
    CloseTab {
        tab_id: String,
    },
    ActivateTab {
        tab_id: String,
    },
    MoveTab {
        tab_id: String,
        index: u32,
    },
    ListSpaces,
    SwitchSpace {
        space_id: String,
    },
    Navigate {
        tab_id: String,
        url: String,
    },
    Reload {
        tab_id: String,
    },
    SearchHistory {
        query: String,
        limit: u32,
    },
    ClearHistory,
    StorageGet {
        key: String,
    },
    StorageSet {
        key: String,
        value: Vec<u8>,
    },
    StorageDelete {
        key: String,
    },
    Notify {
        message: String,
    },
    ContributeCommand {
        id: String,
        title: String,
        description: Option<String>,
    },
    ContributeKeybinding {
        command_id: String,
        key: String,
        when: Option<String>,
    },
    ContributeSlotItem {
        slot: String,
        id: String,
        label: String,
        command_id: String,
    },
    HttpRequest {
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostResponse {
    Unit,
    Id(String),
    Tabs(Vec<TabRecord>),
    Spaces(Vec<SpaceRecord>),
    History(Vec<HistoryRecord>),
    Bytes(Option<Vec<u8>>),
    Http {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
}

pub trait HostApi: Send + Sync + 'static {
    /// Dispatch is called synchronously on the plugin worker. Implementations
    /// must be nonblocking or use a bounded response wait; guest epoch
    /// interruption cannot preempt arbitrary Rust host code.
    fn dispatch(&self, request: HostRequest) -> Result<HostResponse, String>;
}

#[derive(Clone)]
pub(super) struct HostState {
    pub(super) grants: PermissionGrants,
    pub(super) api: Arc<dyn HostApi>,
}

impl HostState {
    fn dispatch(
        &self,
        capability: Capability,
        request: HostRequest,
    ) -> Result<HostResponse, String> {
        self.grants.require(capability)?;
        self.api.dispatch(request)
    }
}

pub(super) struct StoreState {
    pub(super) wasi: WasiCtx,
    pub(super) table: ResourceTable,
    pub(super) host: HostState,
    pub(super) limits: StoreLimits,
}

impl WasiView for StoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl StoreState {
    fn dispatch(
        &self,
        capability: Capability,
        request: HostRequest,
    ) -> Result<HostResponse, String> {
        self.host.dispatch(capability, request)
    }
}

impl lumen::plugin::host::Host for StoreState {
    fn list_tabs(&mut self) -> Result<Vec<lumen::plugin::host::Tab>, String> {
        match self.dispatch(Capability::TabsRead, HostRequest::ListTabs)? {
            HostResponse::Tabs(tabs) => Ok(tabs.into_iter().map(Into::into).collect()),
            _ => Err("host adapter returned an invalid response for list-tabs".into()),
        }
    }

    fn open_tab(&mut self, url: String) -> Result<String, String> {
        match self.dispatch(Capability::TabsWrite, HostRequest::OpenTab { url })? {
            HostResponse::Id(id) => Ok(id),
            _ => Err("host adapter returned an invalid response for open-tab".into()),
        }
    }

    fn close_tab(&mut self, tab_id: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::TabsWrite, HostRequest::CloseTab { tab_id })?)
    }

    fn activate_tab(&mut self, tab_id: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::TabsWrite, HostRequest::ActivateTab { tab_id })?)
    }

    fn move_tab(&mut self, tab_id: String, index: u32) -> Result<(), String> {
        unit_response(self.dispatch(
            Capability::TabsWrite,
            HostRequest::MoveTab { tab_id, index },
        )?)
    }

    fn list_spaces(&mut self) -> Result<Vec<lumen::plugin::host::Space>, String> {
        match self.dispatch(Capability::SpacesRead, HostRequest::ListSpaces)? {
            HostResponse::Spaces(spaces) => Ok(spaces.into_iter().map(Into::into).collect()),
            _ => Err("host adapter returned an invalid response for list-spaces".into()),
        }
    }

    fn switch_space(&mut self, space_id: String) -> Result<(), String> {
        unit_response(self.dispatch(
            Capability::SpacesWrite,
            HostRequest::SwitchSpace { space_id },
        )?)
    }

    fn navigate(&mut self, tab_id: String, url: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::TabsWrite, HostRequest::Navigate { tab_id, url })?)
    }

    fn reload(&mut self, tab_id: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::TabsWrite, HostRequest::Reload { tab_id })?)
    }

    fn search_history(
        &mut self,
        query: String,
        limit: u32,
    ) -> Result<Vec<lumen::plugin::host::HistoryEntry>, String> {
        match self.dispatch(
            Capability::HistoryRead,
            HostRequest::SearchHistory { query, limit },
        )? {
            HostResponse::History(entries) => Ok(entries.into_iter().map(Into::into).collect()),
            _ => Err("host adapter returned an invalid response for search-history".into()),
        }
    }

    fn clear_history(&mut self) -> Result<(), String> {
        unit_response(self.dispatch(Capability::HistoryWrite, HostRequest::ClearHistory)?)
    }

    fn storage_get(&mut self, key: String) -> Result<Option<Vec<u8>>, String> {
        match self.dispatch(Capability::Storage, HostRequest::StorageGet { key })? {
            HostResponse::Bytes(value) => Ok(value),
            _ => Err("host adapter returned an invalid response for storage-get".into()),
        }
    }

    fn storage_set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        unit_response(self.dispatch(Capability::Storage, HostRequest::StorageSet { key, value })?)
    }

    fn storage_delete(&mut self, key: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::Storage, HostRequest::StorageDelete { key })?)
    }

    fn notify(&mut self, message: String) -> Result<(), String> {
        unit_response(self.dispatch(Capability::Notify, HostRequest::Notify { message })?)
    }

    fn contribute_command(
        &mut self,
        command: lumen::plugin::host::CommandContribution,
    ) -> Result<(), String> {
        unit_response(self.dispatch(
            Capability::Commands,
            HostRequest::ContributeCommand {
                id: command.id,
                title: command.title,
                description: command.description,
            },
        )?)
    }

    fn contribute_keybinding(
        &mut self,
        binding: lumen::plugin::host::KeybindingContribution,
    ) -> Result<(), String> {
        unit_response(self.dispatch(
            Capability::Keymap,
            HostRequest::ContributeKeybinding {
                command_id: binding.command_id,
                key: binding.key,
                when: binding.when,
            },
        )?)
    }

    fn contribute_slot_item(&mut self, item: lumen::plugin::host::SlotItem) -> Result<(), String> {
        unit_response(self.dispatch(
            Capability::UiSlots,
            HostRequest::ContributeSlotItem {
                slot: item.slot,
                id: item.id,
                label: item.label,
                command_id: item.command_id,
            },
        )?)
    }

    fn http_request(
        &mut self,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<lumen::plugin::host::HttpResponse, String> {
        if !self.host.grants.allows_network_url(&url) {
            return Err("network permission denied for URL host or scheme".into());
        }
        match self.host.api.dispatch(HostRequest::HttpRequest {
            method,
            url,
            headers,
            body,
        })? {
            HostResponse::Http {
                status,
                headers,
                body,
            } => Ok(lumen::plugin::host::HttpResponse {
                status,
                headers,
                body,
            }),
            _ => Err("host adapter returned an invalid response for http-request".into()),
        }
    }
}

fn unit_response(response: HostResponse) -> Result<(), String> {
    match response {
        HostResponse::Unit => Ok(()),
        _ => Err("host adapter returned an invalid unit response".into()),
    }
}

impl From<TabRecord> for lumen::plugin::host::Tab {
    fn from(tab: TabRecord) -> Self {
        Self {
            id: tab.id,
            title: tab.title,
            url: tab.url,
            active: tab.active,
            pinned: tab.pinned,
        }
    }
}

impl From<SpaceRecord> for lumen::plugin::host::Space {
    fn from(space: SpaceRecord) -> Self {
        Self {
            id: space.id,
            name: space.name,
        }
    }
}

impl From<HistoryRecord> for lumen::plugin::host::HistoryEntry {
    fn from(entry: HistoryRecord) -> Self {
        Self {
            url: entry.url,
            title: entry.title,
            visits: entry.visits,
            last_visited_unix_ms: entry.last_visited_unix_ms,
        }
    }
}
