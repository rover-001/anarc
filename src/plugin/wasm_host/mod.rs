//! Permission-checked Wasmtime boundary for WebAssembly component plugins.
//!
//! Each mounted component owns one worker thread and one Wasmtime engine. Calls
//! cross to that worker through a bounded channel; WIT imports are checked
//! against grants before a host adapter is called. This module is feature-gated
//! and deliberately not wired into `Kernel` until the plugin manager supplies
//! explicit grants and a main-thread service dispatcher.

mod host_bridge;
mod loader;
mod permissions;
#[cfg(test)]
mod tests;
mod worker;

use std::time::Duration;

use wasmtime::component::bindgen;

bindgen!({
    path: "wit",
    world: "lumen-plugin",
});

pub(super) const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
pub(super) const EVENT_DEADLINE: Duration = Duration::from_millis(5);
pub(super) const COMMAND_DEADLINE: Duration = Duration::from_millis(50);
pub(super) const EVENT_FUEL: u64 = 5_000_000;
pub(super) const COMMAND_FUEL: u64 = 50_000_000;
pub(super) const CHANNEL_CAPACITY: usize = 32;

pub use host_bridge::{
    HistoryRecord, HostApi, HostRequest, HostResponse, SpaceRecord, TabRecord,
};
pub(crate) use loader::register_wasm_plugins;
pub use permissions::{Capability, PermissionGrants};
pub use worker::{WasmWorker, WorkerFailure, WorkerStatus};
