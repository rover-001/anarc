use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Persistent, user-editable plugin selection and per-plugin configuration.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct PluginConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) profile: Option<String>,
    #[serde(default)]
    pub(crate) plugins: BTreeMap<String, PluginEntry>,
    #[serde(flatten)]
    pub(crate) extra: BTreeMap<String, toml::Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct PluginEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub(crate) config: toml::Table,
    #[serde(flatten)]
    pub(crate) extra: BTreeMap<String, toml::Value>,
}

impl PluginConfig {
    pub(crate) fn load() -> Result<Self, String> {
        let path = crate::paths::plugin_config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                toml::from_str(&contents).map_err(|error| format!("{}: {error}", path.display()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("{}: {error}", path.display())),
        }
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let path = crate::paths::plugin_config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))
    }
}
