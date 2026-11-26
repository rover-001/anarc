//! Dedicated Wasmtime worker thread, fuel/epoch deadlines, and runtime initialization.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use wasmtime::{
    Config, Engine, Store, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker, ResourceTable},
};
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};

use super::host_bridge::{HostApi, HostState, StoreState};
use super::permissions::{Capability, PermissionGrants, validate_requested_permissions};
use super::{
    CHANNEL_CAPACITY, COMMAND_DEADLINE, COMMAND_FUEL, EVENT_DEADLINE, EVENT_FUEL, LumenPlugin,
    MEMORY_LIMIT_BYTES, exports,
};

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

pub(super) enum Request {
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
    pub(super) requests: SyncSender<Request>,
    pub(super) status: Arc<Mutex<WorkerStatus>>,
    pub(super) thread: Option<JoinHandle<()>>,
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

    pub(super) fn try_enqueue(&self, request: Request) -> Result<(), WorkerFailure> {
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

fn finish_request<T>(
    status: &Mutex<WorkerStatus>,
    reply: mpsc::Sender<Result<T, WorkerFailure>>,
    result: Result<T, WorkerFailure>,
) -> bool {
    if let Err(error) = &result {
        set_status(status, WorkerStatus::Failed(error.clone()));
    }
    let failed = result.is_err();
    let _ = reply.send(result);
    failed
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
        let should_stop = match request {
            Request::Event {
                name,
                payload,
                reply,
            } => {
                let result = invoke_event(&engine, &mut store, &bindings, &name, &payload);
                finish_request(&status, reply, result)
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
                finish_request(&status, reply, result)
            }
            Request::Stop => true,
        };
        if should_stop {
            break;
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

pub(super) fn timed_guest_call<T>(
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
