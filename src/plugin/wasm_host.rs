//! Permission-checked Wasmtime boundary for WebAssembly component plugins.
//!
//! Each mounted component owns one worker thread and one Wasmtime engine. Calls
//! cross to that worker through a bounded channel; WIT imports are checked
//! against grants before a host adapter is called. This module is feature-gated
//! and deliberately not wired into `Kernel` until the plugin manager supplies
//! explicit grants and a main-thread service dispatcher.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use super::{Manifest, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind};

use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker, ResourceTable, bindgen},
};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

bindgen!({
    path: "wit",
    world: "lumen-plugin",
});

const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const EVENT_DEADLINE: Duration = Duration::from_millis(5);
const COMMAND_DEADLINE: Duration = Duration::from_millis(50);
const EVENT_FUEL: u64 = 5_000_000;
const COMMAND_FUEL: u64 = 50_000_000;
const CHANNEL_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    TabsRead,
    TabsWrite,
    SpacesRead,
    SpacesWrite,
    HistoryRead,
    HistoryWrite,
    Storage,
    Notify,
    Commands,
    Keymap,
    UiSlots,
}

/// Explicit grants produced by the install/permission flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionGrants {
    capabilities: BTreeSet<Capability>,
    network_hosts: Vec<String>,
}

impl PermissionGrants {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
            network_hosts: Vec::new(),
        }
    }

    pub fn with_network_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
        self.network_hosts = hosts.into_iter().collect();
        self
    }

    pub fn parse(capabilities: &[String], network_hosts: &[String]) -> Result<Self, String> {
        let mut grants = Self::default().with_network_hosts(network_hosts.iter().cloned());
        for raw in capabilities {
            match raw.as_str() {
                "tabs=read" => {
                    grants.capabilities.insert(Capability::TabsRead);
                }
                "tabs=read-write" => {
                    grants.capabilities.insert(Capability::TabsRead);
                    grants.capabilities.insert(Capability::TabsWrite);
                }
                "spaces=read" => {
                    grants.capabilities.insert(Capability::SpacesRead);
                }
                "spaces=read-write" => {
                    grants.capabilities.insert(Capability::SpacesRead);
                    grants.capabilities.insert(Capability::SpacesWrite);
                }
                "history=read" => {
                    grants.capabilities.insert(Capability::HistoryRead);
                }
                "history=read-write" => {
                    grants.capabilities.insert(Capability::HistoryRead);
                    grants.capabilities.insert(Capability::HistoryWrite);
                }
                "storage=true" => {
                    grants.capabilities.insert(Capability::Storage);
                }
                "notify=true" => {
                    grants.capabilities.insert(Capability::Notify);
                }
                "commands=true" => {
                    grants.capabilities.insert(Capability::Commands);
                }
                "keymap=true" => {
                    grants.capabilities.insert(Capability::Keymap);
                }
                "ui-slots=true" => {
                    grants.capabilities.insert(Capability::UiSlots);
                }
                _ => return Err(format!("unsupported permission declaration: {raw}")),
            }
        }
        Ok(grants)
    }

    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    fn require(&self, capability: Capability) -> Result<(), String> {
        if self.allows(capability) {
            Ok(())
        } else {
            Err(format!("permission denied: {capability:?}"))
        }
    }

    fn allows_network_url(&self, raw_url: &str) -> bool {
        let Ok(url) = url::Url::parse(raw_url) else {
            return false;
        };
        if url.scheme() != "https" {
            return false;
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return false;
        };
        self.network_hosts.iter().any(|pattern| {
            let pattern = pattern.trim().to_ascii_lowercase();
            if let Some(suffix) = pattern.strip_prefix("*.") {
                host.strip_suffix(&format!(".{suffix}"))
                    .is_some_and(|prefix| !prefix.is_empty())
            } else {
                host == pattern
            }
        })
    }

    fn grants_network_pattern(&self, requested: &str) -> bool {
        let requested = requested
            .strip_prefix("https://")
            .unwrap_or(requested)
            .trim_end_matches('/');
        let requested = requested.to_ascii_lowercase();
        self.network_hosts.iter().any(|granted| {
            let granted = granted
                .strip_prefix("https://")
                .unwrap_or(granted)
                .trim_end_matches('/')
                .to_ascii_lowercase();
            if let Some(requested_suffix) = requested.strip_prefix("*.") {
                granted == requested
                    || granted.strip_prefix("*.").is_some_and(|granted_suffix| {
                        requested_suffix == granted_suffix
                            || requested_suffix.ends_with(&format!(".{granted_suffix}"))
                    })
            } else if let Some(granted_suffix) = granted.strip_prefix("*.") {
                requested == granted_suffix || requested.ends_with(&format!(".{granted_suffix}"))
            } else {
                requested == granted
            }
        })
    }
}

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
struct HostState {
    grants: PermissionGrants,
    api: Arc<dyn HostApi>,
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

struct StoreState {
    wasi: WasiCtx,
    table: ResourceTable,
    host: HostState,
    limits: StoreLimits,
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

fn validate_requested_permissions(
    requested: &[String],
    approved: &PermissionGrants,
) -> Result<(), String> {
    let mut capabilities = Vec::new();
    let mut network_hosts = Vec::new();
    for permission in requested {
        if let Some(host) = permission.strip_prefix("network=") {
            network_hosts.push(host.to_owned());
        } else {
            capabilities.push(permission.clone());
        }
    }
    let requested = PermissionGrants::parse(&capabilities, &network_hosts)?;
    for capability in requested.capabilities {
        approved.require(capability)?;
    }
    for host in requested.network_hosts {
        if !approved.grants_network_pattern(&host) {
            return Err(format!(
                "network permission denied for host pattern: {host}"
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerFailure {
    Initialization(String),
    Trap(String),
    Timeout { operation: &'static str },
    FuelExhausted { operation: &'static str },
    QueueFull,
    WorkerStopped,
    GuestError(String),
}

impl std::fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WorkerFailure {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerStatus {
    Starting,
    Active,
    Failed(WorkerFailure),
    Stopped,
}

enum Request {
    Event {
        name: String,
        payload: String,
        reply: mpsc::Sender<Result<Option<String>, WorkerFailure>>,
    },
    Command {
        id: String,
        reply: mpsc::Sender<Result<(), WorkerFailure>>,
    },
    Stop,
}

pub struct WasmWorker {
    requests: SyncSender<Request>,
    status: Arc<Mutex<WorkerStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl WasmWorker {
    pub fn spawn(
        component_path: impl AsRef<Path>,
        plugin_data_dir: impl AsRef<Path>,
        config: String,
        grants: PermissionGrants,
        api: Arc<dyn HostApi>,
    ) -> Result<Self, WorkerFailure> {
        let component_path = component_path.as_ref().to_owned();
        let plugin_data_dir = plugin_data_dir.as_ref().to_owned();
        std::fs::create_dir_all(&plugin_data_dir)
            .map_err(|error| WorkerFailure::Initialization(error.to_string()))?;
        let (requests, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
        let status = Arc::new(Mutex::new(WorkerStatus::Starting));
        let worker_status = Arc::clone(&status);
        let thread = thread::Builder::new()
            .name("lumen-wasm-plugin".into())
            .spawn(move || {
                worker_main(
                    component_path,
                    plugin_data_dir,
                    config,
                    grants,
                    api,
                    receiver,
                    worker_status,
                );
            })
            .map_err(|error| WorkerFailure::Initialization(error.to_string()))?;
        Ok(Self {
            requests,
            status,
            thread: Some(thread),
        })
    }

    pub fn status(&self) -> WorkerStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(WorkerStatus::Stopped)
    }

    pub fn on_event(
        &self,
        name: impl Into<String>,
        payload: impl Into<String>,
    ) -> Result<Option<String>, WorkerFailure> {
        let (reply, receiver) = mpsc::channel();
        self.try_enqueue(Request::Event {
            name: name.into(),
            payload: payload.into(),
            reply,
        })?;
        receiver.recv().map_err(|_| WorkerFailure::WorkerStopped)?
    }

    pub fn run_command(&self, id: impl Into<String>) -> Result<(), WorkerFailure> {
        let (reply, receiver) = mpsc::channel();
        self.try_enqueue(Request::Command {
            id: id.into(),
            reply,
        })?;
        receiver.recv().map_err(|_| WorkerFailure::WorkerStopped)?
    }

    fn try_enqueue(&self, request: Request) -> Result<(), WorkerFailure> {
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => WorkerFailure::QueueFull,
                mpsc::TrySendError::Disconnected(_) => WorkerFailure::WorkerStopped,
            })
    }

    pub fn stop(mut self) {
        let _ = self.requests.try_send(Request::Stop);
        // Dropping a JoinHandle detaches it; never block a UI caller while a
        // plugin is finishing its bounded current invocation.
        let _ = self.thread.take();
    }
}

impl Drop for WasmWorker {
    fn drop(&mut self) {
        let _ = self.requests.try_send(Request::Stop);
    }
}

fn worker_main(
    component_path: PathBuf,
    plugin_data_dir: PathBuf,
    config: String,
    grants: PermissionGrants,
    api: Arc<dyn HostApi>,
    receiver: Receiver<Request>,
    status: Arc<Mutex<WorkerStatus>>,
) {
    let (engine, mut store, bindings) =
        match initialize_runtime(&component_path, &plugin_data_dir, grants, api) {
            Ok(runtime) => runtime,
            Err(error) => {
                set_status(
                    &status,
                    WorkerStatus::Failed(WorkerFailure::Initialization(error.to_string())),
                );
                return;
            }
        };
    set_status(&status, WorkerStatus::Active);
    if let Err(error) = invoke_command(
        &engine,
        &mut store,
        &bindings,
        "__lumen_apply",
        &config,
        COMMAND_DEADLINE,
        COMMAND_FUEL,
    ) {
        set_status(&status, WorkerStatus::Failed(error));
        return;
    }

    while let Ok(request) = receiver.recv() {
        match request {
            Request::Event {
                name,
                payload,
                reply,
            } => {
                let result = invoke_event(&engine, &mut store, &bindings, &name, &payload);
                if let Err(error) = &result {
                    set_status(&status, WorkerStatus::Failed(error.clone()));
                }
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    break;
                }
            }
            Request::Command { id, reply } => {
                let result = invoke_command(
                    &engine,
                    &mut store,
                    &bindings,
                    &id,
                    "",
                    COMMAND_DEADLINE,
                    COMMAND_FUEL,
                );
                if let Err(error) = &result {
                    set_status(&status, WorkerStatus::Failed(error.clone()));
                }
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    break;
                }
            }
            Request::Stop => break,
        }
    }
    if matches!(status.lock().ok().as_deref(), Some(WorkerStatus::Active)) {
        set_status(&status, WorkerStatus::Stopped);
    }
}

type PluginBindings = LumenPlugin;

fn initialize_runtime(
    component_path: &Path,
    plugin_data_dir: &Path,
    grants: PermissionGrants,
    api: Arc<dyn HostApi>,
) -> Result<(Engine, Store<StoreState>, PluginBindings), wasmtime::Error> {
    let mut config = Config::new();
    config
        .consume_fuel(true)
        .epoch_interruption(true)
        .max_wasm_stack(1024 * 1024);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, component_path)?;
    let mut linker = Linker::new(&engine);
    LumenPlugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

    let mut wasi = WasiCtxBuilder::new();
    if grants.allows(Capability::Storage) {
        wasi.preopened_dir(plugin_data_dir, ".", FsPerms::ReadWrite)?;
    }
    let approved_grants = grants.clone();
    let state = StoreState {
        wasi: wasi.build(),
        table: ResourceTable::new(),
        host: HostState { grants, api },
        limits: StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT_BYTES)
            .memories(1)
            .build(),
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);
    let bindings = timed_guest_call(
        &engine,
        &mut store,
        "initialization",
        COMMAND_DEADLINE,
        COMMAND_FUEL,
        |store| {
            LumenPlugin::instantiate(store, &component, &linker)
                .map_err(|error| format!("{error:#}"))
        },
    )
    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
    let manifest = timed_guest_call(
        &engine,
        &mut store,
        "manifest",
        COMMAND_DEADLINE,
        COMMAND_FUEL,
        |store| {
            bindings
                .lumen_plugin_guest()
                .call_get_manifest(store)
                .map_err(|error| format!("{error:#}"))
        },
    )
    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
    validate_requested_permissions(&manifest.requested_permissions, &approved_grants)
        .map_err(wasmtime::Error::msg)?;
    Ok((engine, store, bindings))
}

fn invoke_command(
    engine: &Engine,
    store: &mut Store<StoreState>,
    bindings: &PluginBindings,
    command: &str,
    config: &str,
    timeout: Duration,
    fuel: u64,
) -> Result<(), WorkerFailure> {
    timed_guest_call(engine, store, "command", timeout, fuel, |store| {
        if command == "__lumen_apply" {
            bindings
                .lumen_plugin_guest()
                .call_apply(store, config)
                .map_err(|error| format!("{error:#}"))?
                .map_err(|error| format!("plugin apply failed: {error}"))?;
        } else {
            bindings
                .lumen_plugin_guest()
                .call_run_command(store, command)
                .map_err(|error| format!("{error:#}"))?
                .map_err(|error| format!("plugin command failed: {error}"))?;
        }
        Ok(())
    })
}

fn invoke_event(
    engine: &Engine,
    store: &mut Store<StoreState>,
    bindings: &PluginBindings,
    name: &str,
    payload: &str,
) -> Result<Option<String>, WorkerFailure> {
    timed_guest_call(
        engine,
        store,
        "event",
        EVENT_DEADLINE,
        EVENT_FUEL,
        |store| {
            bindings
                .lumen_plugin_guest()
                .call_on_event(
                    store,
                    &exports::lumen::plugin::guest::Event {
                        name: name.to_owned(),
                        payload: payload.to_owned(),
                    },
                )
                .map_err(|error| format!("{error:#}"))?
                .map_err(|error| format!("plugin event failed: {error}"))
        },
    )
}

fn timed_guest_call<T>(
    engine: &Engine,
    store: &mut Store<StoreState>,
    operation: &'static str,
    timeout: Duration,
    fuel: u64,
    call: impl FnOnce(&mut Store<StoreState>) -> Result<T, String>,
) -> Result<T, WorkerFailure> {
    store
        .set_fuel(fuel)
        .map_err(|error| WorkerFailure::Trap(error.to_string()))?;
    store.set_epoch_deadline(1);
    let (cancel_tx, cancel_rx) = mpsc::channel::<()>();
    let timer_engine = engine.clone();
    let timer = thread::Builder::new()
        .name("lumen-wasm-deadline".into())
        .spawn(move || {
            if let Err(RecvTimeoutError::Timeout) = cancel_rx.recv_timeout(timeout) {
                timer_engine.increment_epoch();
            }
        })
        .map_err(|error| WorkerFailure::Trap(format!("failed to start deadline timer: {error}")))?;
    let outcome = call(store);
    let _ = cancel_tx.send(());
    let _ = timer.join();
    match outcome {
        Ok(value) => Ok(value),
        Err(error) if error.starts_with("plugin ") => Err(WorkerFailure::GuestError(error)),
        Err(error) if error.to_ascii_lowercase().contains("fuel") => {
            Err(WorkerFailure::FuelExhausted { operation })
        }
        Err(error)
            if error.to_ascii_lowercase().contains("epoch")
                || error.to_ascii_lowercase().contains("interrupt") =>
        {
            Err(WorkerFailure::Timeout { operation })
        }
        Err(error) => Err(WorkerFailure::Trap(error)),
    }
}

fn set_status(status: &Mutex<WorkerStatus>, value: WorkerStatus) {
    if let Ok(mut status) = status.lock() {
        *status = value;
    }
}

#[derive(Clone)]
struct WasmPackage {
    id: String,
    name: String,
    version: String,
    description: String,
    directory: PathBuf,
    component: PathBuf,
    permission_summary: Vec<String>,
}

/// Register installed WebAssembly packages as disabled manager entries.
/// Grants intentionally start empty; an approval flow must provide explicit
/// grants before packages that request capabilities can activate.
pub(crate) fn register_wasm_plugins(host: &mut PluginHost, root: &Path) -> Result<(), String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", root.display())),
    };
    let mut entries = entries
        .map(|entry| entry.map_err(|error| format!("{}: {error}", root.display())))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if !entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let directory = entry.path();
        let manifest_path = directory.join("lumen-plugin.toml");
        let Ok(source) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&source) else {
            continue;
        };
        let Some(plugin) = value.get("plugin").and_then(toml::Value::as_table) else {
            continue;
        };
        if plugin.get("kind").and_then(toml::Value::as_str) != Some("wasm") {
            continue;
        }

        match parse_wasm_package(&directory, &value) {
            Ok(package) => {
                let descriptor = PluginDescriptor {
                    id: package.id.clone(),
                    name: package.name.clone(),
                    version: package.version.clone(),
                    kind: PluginKind::Wasm,
                    description: package.description.clone(),
                    permissions: package.permission_summary.clone(),
                    enabled_by_default: false,
                    can_toggle: true,
                };
                host.register(descriptor, move || {
                    Box::new(WasmPlugin::new(package.clone()))
                });
            }
            Err(error) => {
                let id = plugin
                    .get("id")
                    .and_then(toml::Value::as_str)
                    .filter(|id| safe_plugin_id(id))
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("invalid.wasm.{}", path_slug(&directory)));
                let descriptor = PluginDescriptor {
                    id: id.clone(),
                    name: plugin
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("Invalid WebAssembly plugin")
                        .to_owned(),
                    version: plugin
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                    kind: PluginKind::Wasm,
                    description: format!("Invalid WebAssembly package: {error}"),
                    permissions: Vec::new(),
                    enabled_by_default: false,
                    can_toggle: true,
                };
                let invalid = InvalidWasmPlugin {
                    manifest: Manifest {
                        id,
                        ..Manifest::default()
                    },
                    error,
                };
                host.register(descriptor, move || Box::new(invalid.clone()));
            }
        }
    }
    Ok(())
}

fn parse_wasm_package(directory: &Path, value: &toml::Value) -> Result<WasmPackage, String> {
    let plugin = value
        .get("plugin")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "missing [plugin] table".to_owned())?;
    let field = |name: &str| {
        plugin
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("missing plugin.{name}"))
    };
    let id = field("id")?;
    if !safe_plugin_id(&id) {
        return Err(format!("invalid plugin ID {id:?}"));
    }
    if field("api")? != "1" {
        return Err("unsupported plugin API version".into());
    }
    let entry = field("entry")?;
    let relative = Path::new(&entry);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("plugin entry must be a relative path inside its package".into());
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve package directory: {error}"))?;
    let component = directory.join(relative);
    let canonical_component = component
        .canonicalize()
        .map_err(|error| format!("cannot resolve component entry: {error}"))?;
    if !canonical_component.starts_with(&directory) || !canonical_component.is_file() {
        return Err("component entry must be a regular file inside its package".into());
    }
    let permission_summary = value
        .get("permissions")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(name, value)| format!("{name} = {value}"))
                .collect()
        })
        .unwrap_or_default();
    Ok(WasmPackage {
        id,
        name: field("name")?,
        version: field("version")?,
        description: plugin
            .get("description")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        directory,
        component: canonical_component,
        permission_summary,
    })
}

fn safe_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
}

fn path_slug(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let slug = name
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "unknown".into()
    } else {
        slug
    }
}

#[derive(Clone)]
struct InvalidWasmPlugin {
    manifest: Manifest,
    error: String,
}

impl Plugin for InvalidWasmPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, _: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        Err(PluginError(self.error.clone()))
    }
}

struct WasmPlugin {
    package: WasmPackage,
    worker: Arc<Mutex<Option<WasmWorker>>>,
    manifest: Manifest,
}

impl WasmPlugin {
    fn new(package: WasmPackage) -> Self {
        Self {
            manifest: Manifest {
                id: package.id.clone(),
                ..Manifest::default()
            },
            package,
            worker: Arc::new(Mutex::new(None)),
        }
    }
}

/// The current manager supplies no permission approval UI or main-thread
/// service bridge. This adapter therefore grants nothing and rejects every
/// host request. Integrators may provide a bounded dispatcher only after the
/// user's explicit grants have been persisted and reviewed.
struct DeniedHostApi;

impl HostApi for DeniedHostApi {
    fn dispatch(&self, _request: HostRequest) -> Result<HostResponse, String> {
        Err("host service adapter is not connected; request denied".into())
    }
}

impl Plugin for WasmPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let worker = WasmWorker::spawn(
            &self.package.component,
            self.package.directory.join(".lumen-data"),
            String::new(),
            PermissionGrants::default(),
            Arc::new(DeniedHostApi),
        )
        .map_err(|error| PluginError(error.to_string()))?;
        let slot = Arc::clone(&self.worker);
        if let Ok(mut current) = slot.lock() {
            *current = Some(worker);
        }
        ctx.effect(move || {
            if let Ok(mut current) = slot.lock()
                && let Some(worker) = current.take()
            {
                worker.stop();
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wasmtime::{Instance, Module};

    #[derive(Default)]
    struct CountingApi(AtomicUsize);

    impl HostApi for CountingApi {
        fn dispatch(&self, request: HostRequest) -> Result<HostResponse, String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            match request {
                HostRequest::ListTabs => Ok(HostResponse::Tabs(Vec::new())),
                HostRequest::ListSpaces => Ok(HostResponse::Spaces(Vec::new())),
                HostRequest::SearchHistory { .. } => Ok(HostResponse::History(Vec::new())),
                HostRequest::StorageGet { .. } => Ok(HostResponse::Bytes(None)),
                _ => Ok(HostResponse::Unit),
            }
        }
    }

    fn test_store(engine: &Engine, grants: PermissionGrants) -> Store<StoreState> {
        let state = StoreState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            host: HostState {
                grants,
                api: Arc::new(CountingApi::default()),
            },
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT_BYTES)
                .memories(1)
                .build(),
        };
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.limits);
        store
    }

    fn test_engine() -> Engine {
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .epoch_interruption(true)
            .max_wasm_stack(1024 * 1024);
        Engine::new(&config).expect("Wasmtime engine initializes")
    }

    #[test]
    fn denied_wit_calls_never_reach_the_host_adapter() {
        let api = Arc::new(CountingApi::default());
        let mut state = StoreState {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            host: HostState {
                grants: PermissionGrants::default(),
                api: api.clone(),
            },
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT_BYTES)
                .build(),
        };

        let denied = lumen::plugin::host::Host::list_tabs(&mut state)
            .expect_err("tabs read must be denied by default");
        assert!(denied.contains("permission denied"));
        assert_eq!(api.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn network_patterns_match_exact_and_wildcard_entries_independently() {
        let grants = PermissionGrants::default().with_network_hosts([
            "exact.example.test".to_owned(),
            "*.allowed.example.test".to_owned(),
        ]);
        assert!(grants.allows_network_url("https://exact.example.test/path"));
        assert!(!grants.allows_network_url("https://sub.exact.example.test/path"));
        assert!(grants.allows_network_url("https://sub.allowed.example.test/path"));
        assert!(!grants.allows_network_url("http://sub.allowed.example.test/path"));
        assert!(!grants.allows_network_url("https://allowed.example.test/path"));
    }

    #[test]
    fn denied_manifest_permissions_fail_before_activation() {
        let requested = vec!["tabs=read-write".to_owned()];
        let error = validate_requested_permissions(&requested, &PermissionGrants::default())
            .expect_err("unapproved guest manifest request must fail");
        assert!(error.contains("permission denied"));
    }

    #[test]
    fn bounded_request_queue_returns_without_waiting_when_full() {
        let (requests, _receiver) = mpsc::sync_channel(0);
        let worker = WasmWorker {
            requests,
            status: Arc::new(Mutex::new(WorkerStatus::Starting)),
            thread: None,
        };
        assert_eq!(
            worker.try_enqueue(Request::Stop),
            Err(WorkerFailure::QueueFull)
        );
    }

    #[test]
    fn guest_timeout_and_trap_are_isolated_from_a_new_worker_engine() {
        let engine = test_engine();
        let mut store = test_store(&engine, PermissionGrants::default());
        let spin = Module::new(&engine, r#"(module (func (export "run") (loop br 0)))"#)
            .expect("loop module compiles");
        let instance = Instance::new(&mut store, &spin, &[]).expect("loop module instantiates");
        let spin_fn = instance
            .get_typed_func::<(), ()>(&mut store, "run")
            .expect("loop export exists");
        let timed_out = timed_guest_call(
            &engine,
            &mut store,
            "event",
            Duration::from_millis(5),
            u64::MAX,
            |store| {
                spin_fn
                    .call(store, ())
                    .map_err(|error| format!("{error:#}"))
            },
        );
        assert_eq!(
            timed_out,
            Err(WorkerFailure::Timeout { operation: "event" })
        );

        let healthy_engine = test_engine();
        let mut healthy_store = test_store(&healthy_engine, PermissionGrants::default());
        let trap = Module::new(
            &healthy_engine,
            r#"(module (func (export "run") unreachable))"#,
        )
        .expect("trap module compiles");
        let trap_instance =
            Instance::new(&mut healthy_store, &trap, &[]).expect("trap module instantiates");
        let trap_fn = trap_instance
            .get_typed_func::<(), ()>(&mut healthy_store, "run")
            .expect("trap export exists");
        let trapped = timed_guest_call(
            &healthy_engine,
            &mut healthy_store,
            "command",
            COMMAND_DEADLINE,
            COMMAND_FUEL,
            |store| trap_fn.call(store, ()).map_err(|error| error.to_string()),
        );
        assert!(matches!(trapped, Err(WorkerFailure::Trap(_))));

        let fresh_engine = test_engine();
        let mut fresh_store = test_store(&fresh_engine, PermissionGrants::default());
        let okay = Module::new(&fresh_engine, r#"(module (func (export "run")))"#)
            .expect("healthy module compiles");
        let okay_instance =
            Instance::new(&mut fresh_store, &okay, &[]).expect("healthy module instantiates");
        let okay_fn = okay_instance
            .get_typed_func::<(), ()>(&mut fresh_store, "run")
            .expect("healthy export exists");
        assert_eq!(
            timed_guest_call(
                &fresh_engine,
                &mut fresh_store,
                "command",
                COMMAND_DEADLINE,
                COMMAND_FUEL,
                |store| okay_fn.call(store, ()).map_err(|error| error.to_string()),
            ),
            Ok(())
        );
    }

    #[test]
    fn store_limits_reject_memory_growth_past_sixty_four_mib() {
        let engine = test_engine();
        let mut store = test_store(&engine, PermissionGrants::default());
        store.set_fuel(COMMAND_FUEL).unwrap();
        store.set_epoch_deadline(100);
        let module = Module::new(
            &engine,
            r#"(module (memory (export "memory") 1) (func (export "grow") (result i32) (memory.grow (i32.const 1024))))"#,
        )
        .expect("memory module compiles");
        let instance = Instance::new(&mut store, &module, &[]).expect("memory module instantiates");
        let grow = instance
            .get_typed_func::<(), i32>(&mut store, "grow")
            .expect("grow export exists");
        assert_eq!(grow.call(&mut store, ()).unwrap(), -1);
    }

    #[test]
    fn wasm_host_wit_bindings_compile() {
        let _ = std::any::type_name::<LumenPlugin>();
    }
}
