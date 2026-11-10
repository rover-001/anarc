use super::{
    context::{Disposer, PluginCtx, Shared},
    event::{EventMode, EventPayload},
    fiber::{Fiber, FiberState},
    manifest::{Manifest, Plugin, PluginError},
};
use std::{
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

struct Mounted {
    manifest: Manifest,
    plugin: Box<dyn Plugin>,
    fiber: Fiber,
}

pub(crate) struct Kernel {
    shared: Rc<Shared>,
    mounted: HashMap<String, Mounted>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub(crate) fn new() -> Self {
        Self {
            shared: Shared::new(),
            mounted: HashMap::new(),
        }
    }

    pub(crate) fn mount(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let manifest = plugin.manifest().clone();
        if self
            .mounted
            .get(&manifest.id)
            .is_some_and(|mounted| mounted.fiber.state != FiberState::Disposed)
        {
            return Err(PluginError(format!(
                "plugin already mounted: {}",
                manifest.id
            )));
        }
        let id = manifest.id.clone();
        self.mounted.insert(
            id.clone(),
            Mounted {
                manifest,
                plugin,
                fiber: Fiber::pending(),
            },
        );
        self.activate_ready();
        Ok(())
    }

    pub(crate) fn state(&self, id: &str) -> Option<&FiberState> {
        self.mounted.get(id).map(|mounted| &mounted.fiber.state)
    }

    pub(crate) fn unmount(&mut self, id: &str) {
        if !self.mounted.contains_key(id) {
            return;
        }
        self.deactivate_dependents(id);
        if let Some(mounted) = self.mounted.get_mut(id) {
            mounted.fiber.dispose_effects();
            mounted.fiber.state = FiberState::Disposed;
        }
    }

    fn deactivate_dependents(&mut self, provider_id: &str) {
        let removed_services = self
            .mounted
            .get(provider_id)
            .map(|provider| provider.manifest.provides.clone())
            .unwrap_or_default();
        let dependents = self
            .mounted
            .iter()
            .filter(|(other_id, mounted)| {
                other_id.as_str() != provider_id
                    && mounted.fiber.state == FiberState::Active
                    && mounted
                        .manifest
                        .required
                        .iter()
                        .any(|key| removed_services.contains(key))
            })
            .map(|(other_id, _)| other_id.clone())
            .collect::<Vec<_>>();
        for dependent in dependents {
            self.deactivate_dependents(&dependent);
            self.deactivate_to_pending(&dependent);
        }
    }

    pub(crate) fn contributions(&self) -> Vec<super::Contribution> {
        self.shared.contributions.borrow().clone()
    }

    pub(crate) fn emit(
        &self,
        name: &str,
        mode: EventMode,
        payload: EventPayload,
    ) -> Vec<EventPayload> {
        self.shared.events.borrow().emit(name, mode, payload)
    }

    fn deactivate_to_pending(&mut self, id: &str) {
        if let Some(mounted) = self.mounted.get_mut(id) {
            mounted.fiber.dispose_effects();
            mounted.fiber.state = FiberState::Pending;
        }
    }

    fn activate_ready(&mut self) {
        loop {
            let available = self
                .shared
                .services
                .borrow()
                .keys()
                .cloned()
                .collect::<HashSet<_>>();
            let candidate = self
                .mounted
                .iter()
                .filter(|(_, mounted)| {
                    mounted.fiber.state == FiberState::Pending
                        && mounted
                            .manifest
                            .required
                            .iter()
                            .all(|key| available.contains(key))
                })
                .map(|(id, _)| id.clone())
                .min();
            let Some(id) = candidate else { break };
            self.activate(&id);
        }
    }

    fn activate(&mut self, id: &str) {
        let Some(mounted) = self.mounted.get_mut(id) else {
            return;
        };
        mounted.fiber.state = FiberState::Loading;
        let mut disposers: Vec<Disposer> = Vec::new();
        let mut ctx = PluginCtx {
            owner: id.to_string(),
            shared: self.shared.clone(),
            disposers: &mut disposers,
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| mounted.plugin.apply(&mut ctx)));
        mounted.fiber.dispose_effects();
        match outcome {
            Ok(Ok(())) => {
                mounted.fiber.disposers = disposers;
                mounted.fiber.state = FiberState::Active;
            }
            Ok(Err(error)) => {
                dispose_all(disposers);
                mounted.fiber.state = FiberState::Failed(error.0);
            }
            Err(panic) => {
                dispose_all(disposers);
                mounted.fiber.state = FiberState::Failed(panic_message(panic));
            }
        }
    }
}

fn dispose_all(mut disposers: Vec<Disposer>) {
    while let Some(dispose) = disposers.pop() {
        dispose();
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = panic.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else {
        "plugin panicked while applying".to_string()
    }
}
