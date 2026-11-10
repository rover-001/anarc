// The plugin API is introduced incrementally; later phases consume these hooks.
#![allow(dead_code)]

mod config;
mod context;
mod core;
mod event;
mod fiber;
mod kernel;
mod manifest;
mod registry;

#[cfg(test)]
mod tests;

pub(crate) use config::PluginConfig;
pub(crate) use context::PluginCtx;
pub(crate) use core::{CorePlugin, PALETTE_COMMANDS, PaletteCommand, SHORTCUT_BINDINGS};
pub(crate) use event::{EventMode, EventPayload};
pub(crate) use kernel::Kernel;
pub(crate) use manifest::{Manifest, Plugin, PluginError};
pub(crate) use registry::{Contribution, Order};

#[cfg(test)]
pub(crate) use context::ServiceKey;
#[cfg(test)]
pub(crate) use event::EventBus;
#[cfg(test)]
pub(crate) use fiber::FiberState;
