use super::{
    CorePlugin, Kernel, Plugin, PluginConfig, PluginDescriptor, PluginKind, PluginManagerModel,
    RuntimeStatus, ToggleAction,
};
use std::{collections::BTreeMap, rc::Rc};

type PluginFactory = Rc<dyn Fn() -> Box<dyn Plugin>>;

/// Main-thread owner for plugin configuration, metadata, and runtime fibers.
pub(crate) struct PluginHost {
    kernel: Kernel,
    config: PluginConfig,
    manager: PluginManagerModel,
    factories: BTreeMap<String, PluginFactory>,
    safe_mode: bool,
}

impl PluginHost {
    pub(crate) fn new(config: PluginConfig) -> Self {
        Self {
            kernel: Kernel::new(),
            config,
            manager: PluginManagerModel::default(),
            factories: BTreeMap::new(),
            safe_mode: false,
        }
    }

    pub(crate) fn set_safe_mode(&mut self, enabled: bool) {
        self.safe_mode = enabled;
    }

    pub(crate) fn is_safe_mode(&self) -> bool {
        self.safe_mode
    }

    pub(crate) fn mount_core(&mut self) -> Result<(), String> {
        let core = CorePlugin::new();
        let manifest = core.manifest().clone();
        self.kernel.mount(Box::new(core)).map_err(|error| error.0)?;
        self.manager.register(
            PluginDescriptor {
                id: manifest.id,
                name: "Anarc Core".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                kind: PluginKind::Core,
                description: "Core browser services and command registrations".into(),
                permissions: Vec::new(),
                enabled_by_default: true,
                can_toggle: false,
            },
            RuntimeStatus::Active,
        );
        Ok(())
    }

    pub(crate) fn register(
        &mut self,
        descriptor: PluginDescriptor,
        factory: impl Fn() -> Box<dyn Plugin> + 'static,
    ) {
        self.factories
            .insert(descriptor.id.clone(), Rc::new(factory));
        self.manager.register(descriptor, RuntimeStatus::Pending);
    }

    /// Mount built-ins enabled by the current config. Missing entries preserve
    /// each descriptor's default, keeping an old profile behavior-compatible.
    pub(crate) fn mount_enabled(&mut self) -> Result<(), String> {
        if self.safe_mode {
            return Ok(());
        }
        let enabled = self
            .manager
            .rows(&self.config)
            .into_iter()
            .filter(|row| row.descriptor.can_toggle && row.enabled)
            .map(|row| row.descriptor.id)
            .collect::<Vec<_>>();
        for id in enabled {
            if matches!(
                self.kernel.state(&id),
                Some(super::fiber::FiberState::Active | super::fiber::FiberState::Loading)
            ) {
                continue;
            }
            let plugin = self.create_plugin(&id)?;
            self.kernel.mount(plugin).map_err(|error| error.0)?;
            self.manager
                .set_runtime_status(&id, self.runtime_status(&id));
        }
        Ok(())
    }

    pub(crate) fn unmount_external(&mut self) -> Result<(), String> {
        if self.safe_mode {
            return Err("plugin changes are disabled in safe mode".into());
        }
        let ids = self
            .manager
            .rows(&self.config)
            .into_iter()
            .filter(|row| {
                row.descriptor.can_toggle
                    && matches!(
                        row.descriptor.kind,
                        super::PluginKind::Pack
                            | super::PluginKind::Panel
                            | super::PluginKind::Wasm
                    )
            })
            .map(|row| row.descriptor.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.kernel.unmount(&id);
            self.manager
                .set_runtime_status(&id, RuntimeStatus::Disposed);
        }
        Ok(())
    }

    pub(crate) fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        if self.safe_mode {
            return Err("plugin changes are disabled in safe mode".into());
        }
        let before = self.config.clone();
        let action = self
            .manager
            .request_toggle(&mut self.config, id, enabled)
            .map_err(|error| format!("{error:?}"))?;
        let runtime_result = match action {
            ToggleAction::Unchanged => Ok(()),
            ToggleAction::Enable => self
                .create_plugin(id)
                .and_then(|plugin| self.kernel.mount(plugin).map_err(|error| error.0)),
            ToggleAction::Disable => {
                self.kernel.unmount(id);
                Ok(())
            }
        };
        if let Err(error) = runtime_result {
            self.config = before;
            return Err(error);
        }

        if let Err(error) = self.config.save() {
            match action {
                ToggleAction::Enable => self.kernel.unmount(id),
                ToggleAction::Disable => {
                    if let Ok(plugin) = self.create_plugin(id) {
                        let _ = self.kernel.mount(plugin);
                    }
                }
                ToggleAction::Unchanged => {}
            }
            self.config = before;
            self.manager.set_runtime_status(id, self.runtime_status(id));
            return Err(error);
        }
        let status = match action {
            ToggleAction::Disable => RuntimeStatus::Disposed,
            ToggleAction::Enable | ToggleAction::Unchanged => self.runtime_status(id),
        };
        self.manager.set_runtime_status(id, status);
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: &str) -> Result<(), String> {
        if self.safe_mode {
            return Err("plugin changes are disabled in safe mode".into());
        }
        let row = self
            .rows()
            .into_iter()
            .find(|row| row.descriptor.id == id)
            .ok_or_else(|| format!("unknown plugin: {id}"))?;
        if !row.descriptor.can_toggle
            || matches!(
                row.descriptor.kind,
                super::PluginKind::Builtin | super::PluginKind::Core
            )
        {
            return Err(format!("plugin cannot be removed: {id}"));
        }

        let config_before = self.config.clone();
        let factory = self.factories.remove(id);
        let descriptor = self
            .manager
            .unregister(id)
            .ok_or_else(|| format!("unknown plugin: {id}"))?;
        self.config.plugins.remove(id);
        if let Err(error) = self.config.save() {
            self.config = config_before;
            self.manager.register(descriptor, row.runtime.clone());
            if let Some(factory) = factory {
                self.factories.insert(id.to_owned(), factory);
            }
            return Err(error);
        }
        self.kernel.unmount(id);
        Ok(())
    }

    pub(crate) fn rows(&self) -> Vec<super::PluginRow> {
        let mut rows = self.manager.rows(&self.config);
        if self.safe_mode {
            for row in &mut rows {
                row.descriptor.can_toggle = false;
            }
        }
        rows
    }

    pub(crate) fn config(&self) -> &PluginConfig {
        &self.config
    }

    pub(crate) fn is_active(&self, id: &str) -> bool {
        self.kernel.is_active(id)
    }

    fn runtime_status(&self, id: &str) -> RuntimeStatus {
        match self.kernel.state(id) {
            Some(super::fiber::FiberState::Pending) => RuntimeStatus::Pending,
            Some(super::fiber::FiberState::Loading) => RuntimeStatus::Loading,
            Some(super::fiber::FiberState::Active) => RuntimeStatus::Active,
            Some(super::fiber::FiberState::Disposed) => RuntimeStatus::Disposed,
            Some(super::fiber::FiberState::Failed(reason)) => RuntimeStatus::Failed(reason.clone()),
            None => RuntimeStatus::Pending,
        }
    }

    fn create_plugin(&self, id: &str) -> Result<Box<dyn Plugin>, String> {
        let factory = self
            .factories
            .get(id)
            .ok_or_else(|| format!("no plugin factory registered for {id}"))?;
        let plugin = factory();
        if plugin.manifest().id != id {
            return Err(format!(
                "plugin factory for {id} returned manifest {}",
                plugin.manifest().id
            ));
        }
        Ok(plugin)
    }

    pub(crate) fn contributions(&self) -> Vec<super::Contribution> {
        self.kernel.contributions()
    }

    pub(crate) fn service<S: std::any::Any>(&self, key: super::ServiceKey<S>) -> Option<Rc<S>> {
        self.kernel.service(key)
    }

    pub(crate) fn emit(
        &self,
        name: &str,
        mode: super::EventMode,
        payload: super::EventPayload,
    ) -> Vec<super::EventPayload> {
        self.kernel.emit(name, mode, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginError;

    struct FeaturePlugin {
        manifest: super::super::Manifest,
    }

    impl Plugin for FeaturePlugin {
        fn manifest(&self) -> &super::super::Manifest {
            &self.manifest
        }

        fn apply(&mut self, _ctx: &mut super::super::PluginCtx<'_>) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn feature() -> Box<dyn Plugin> {
        Box::new(FeaturePlugin {
            manifest: super::super::Manifest {
                id: "lumen.test-feature".into(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: Vec::new(),
            },
        })
    }

    #[test]
    fn safe_mode_mounts_core_and_blocks_optional_plugins_and_config_changes() {
        let mut host = PluginHost::new(PluginConfig::default());
        host.set_safe_mode(true);
        host.mount_core().unwrap();
        host.register(
            PluginDescriptor {
                id: "lumen.test-feature".into(),
                name: "Feature".into(),
                version: "1.0.0".into(),
                kind: PluginKind::Builtin,
                description: String::new(),
                permissions: Vec::new(),
                enabled_by_default: true,
                can_toggle: true,
            },
            feature,
        );
        host.mount_enabled().unwrap();

        assert!(host.is_active("lumen.core"));
        assert!(!host.is_active("lumen.test-feature"));
        assert!(!host.rows()[1].descriptor.can_toggle);
        assert!(host.set_enabled("lumen.test-feature", false).is_err());
        assert!(host.config().plugins.is_empty());
    }
}
