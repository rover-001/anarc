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
        // Atomic replace: a crash mid-write must never leave a truncated config
        // that loads as "no entries" and silently resets every plugin toggle.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &contents)
            .map_err(|error| format!("{}: {error}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|error| format!("{}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_false_round_trips_through_toml() {
        let mut config = PluginConfig::default();
        config.plugins.insert(
            "com.example.plugin".to_owned(),
            PluginEntry {
                enabled: Some(false),
                ..Default::default()
            },
        );
        let encoded = toml::to_string_pretty(&config).unwrap();
        assert!(encoded.contains("enabled = false"), "{encoded}");
        let decoded: PluginConfig = toml::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.plugins["com.example.plugin"].enabled,
            Some(false)
        );
    }
}
