//! Data model for the Plugins settings page.
//!
//! This module intentionally plans config changes without mounting or
//! unmounting anything. The app host must persist the returned config and then
//! apply `ToggleAction` through `Kernel::mount`/`Kernel::unmount`.

use super::config::PluginConfig;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PluginKind {
    Core,
    Builtin,
    Pack,
    Wasm,
    Panel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeStatus {
    Pending,
    Loading,
    Active,
    Disposed,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginDescriptor {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) kind: PluginKind,
    pub(crate) description: String,
    pub(crate) permissions: Vec<String>,
    pub(crate) enabled_by_default: bool,
    pub(crate) can_toggle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginRow {
    pub(crate) descriptor: PluginDescriptor,
    pub(crate) enabled: bool,
    pub(crate) runtime: RuntimeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToggleAction {
    Unchanged,
    Enable,
    Disable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ManagerError {
    UnknownPlugin(String),
    CannotToggleCore(String),
}

#[derive(Default)]
pub(crate) struct PluginManagerModel {
    descriptors: BTreeMap<String, PluginDescriptor>,
    runtime: BTreeMap<String, RuntimeStatus>,
}

impl PluginManagerModel {
    pub(crate) fn register(&mut self, descriptor: PluginDescriptor, runtime: RuntimeStatus) {
        self.runtime.insert(descriptor.id.clone(), runtime);
        self.descriptors.insert(descriptor.id.clone(), descriptor);
    }

    pub(crate) fn rows(&self, config: &PluginConfig) -> Vec<PluginRow> {
        self.descriptors
            .values()
            .map(|descriptor| PluginRow {
                enabled: config
                    .plugins
                    .get(&descriptor.id)
                    .and_then(|entry| entry.enabled)
                    .unwrap_or(descriptor.enabled_by_default),
                runtime: self
                    .runtime
                    .get(&descriptor.id)
                    .cloned()
                    .unwrap_or(RuntimeStatus::Pending),
                descriptor: descriptor.clone(),
            })
            .collect()
    }

    /// Records the user's requested enabled state in `plugins.toml`'s model.
    /// The caller must save `config` and then apply the returned action to the
    /// kernel; this function does not claim the runtime changed.
    pub(crate) fn request_toggle(
        &self,
        config: &mut PluginConfig,
        id: &str,
        enabled: bool,
    ) -> Result<ToggleAction, ManagerError> {
        let descriptor = self
            .descriptors
            .get(id)
            .ok_or_else(|| ManagerError::UnknownPlugin(id.to_owned()))?;
        if !descriptor.can_toggle {
            return Err(ManagerError::CannotToggleCore(id.to_owned()));
        }

        let was_enabled = config
            .plugins
            .get(id)
            .and_then(|entry| entry.enabled)
            .unwrap_or(descriptor.enabled_by_default);
        config.plugins.entry(id.to_owned()).or_default().enabled = Some(enabled);

        Ok(match (was_enabled, enabled) {
            (true, false) => ToggleAction::Disable,
            (false, true) => ToggleAction::Enable,
            _ => ToggleAction::Unchanged,
        })
    }

    pub(crate) fn set_runtime_status(&mut self, id: &str, status: RuntimeStatus) -> bool {
        if !self.descriptors.contains_key(id) {
            return false;
        }
        self.runtime.insert(id.to_owned(), status);
        true
    }

    pub(crate) fn unregister(&mut self, id: &str) -> Option<PluginDescriptor> {
        self.runtime.remove(id);
        self.descriptors.remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(id: &str, kind: PluginKind, enabled_by_default: bool) -> PluginDescriptor {
        PluginDescriptor {
            id: id.to_owned(),
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            kind,
            description: String::new(),
            permissions: Vec::new(),
            enabled_by_default,
            can_toggle: kind != PluginKind::Core,
        }
    }

    #[test]
    fn absent_config_uses_descriptor_defaults_and_exposes_runtime_errors() {
        let mut model = PluginManagerModel::default();
        model.register(
            descriptor("lumen.reader", PluginKind::Builtin, true),
            RuntimeStatus::Active,
        );
        model.register(
            descriptor("dev.broken", PluginKind::Pack, false),
            RuntimeStatus::Failed("invalid manifest".to_owned()),
        );

        let rows = model.rows(&PluginConfig::default());
        let reader = rows
            .iter()
            .find(|row| row.descriptor.id == "lumen.reader")
            .unwrap();
        let broken = rows
            .iter()
            .find(|row| row.descriptor.id == "dev.broken")
            .unwrap();
        assert!(reader.enabled);
        assert_eq!(reader.runtime, RuntimeStatus::Active);
        assert!(!broken.enabled);
        assert_eq!(
            broken.runtime,
            RuntimeStatus::Failed("invalid manifest".to_owned())
        );
    }

    #[test]
    fn disable_and_reenable_update_persisted_config_and_return_runtime_actions() {
        let mut model = PluginManagerModel::default();
        model.register(
            descriptor("lumen.reader", PluginKind::Builtin, true),
            RuntimeStatus::Active,
        );
        let mut config = PluginConfig::default();

        assert_eq!(
            model.request_toggle(&mut config, "lumen.reader", false),
            Ok(ToggleAction::Disable)
        );
        assert_eq!(config.plugins["lumen.reader"].enabled, Some(false));
        assert_eq!(model.rows(&config)[0].runtime, RuntimeStatus::Active);

        assert_eq!(
            model.request_toggle(&mut config, "lumen.reader", true),
            Ok(ToggleAction::Enable)
        );
        assert_eq!(config.plugins["lumen.reader"].enabled, Some(true));
    }

    #[test]
    fn core_and_unknown_plugins_cannot_be_toggled() {
        let mut model = PluginManagerModel::default();
        model.register(
            descriptor("lumen.core", PluginKind::Core, true),
            RuntimeStatus::Active,
        );
        let mut config = PluginConfig::default();

        assert_eq!(
            model.request_toggle(&mut config, "lumen.core", false),
            Err(ManagerError::CannotToggleCore("lumen.core".to_owned()))
        );
        assert_eq!(
            model.request_toggle(&mut config, "missing", false),
            Err(ManagerError::UnknownPlugin("missing".to_owned()))
        );
        assert!(config.plugins.is_empty());
    }
}
