use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use wasmtime::{
    Config, Engine, Instance, Module, Store, StoreLimitsBuilder, component::ResourceTable,
};
use wasmtime_wasi::WasiCtxBuilder;

use super::host_bridge::{HostState, StoreState};
use super::permissions::validate_requested_permissions;
use super::worker::{Request, timed_guest_call};
use super::*;

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
