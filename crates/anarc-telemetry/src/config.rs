//! Local consent + install-id state. This file never leaves the machine and
//! holds no page-derived data. Exact timestamps are not stored: only a day
//! number used to rotate the install ID every 30 days.

use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

pub const INSTALL_ID_ROTATION_DAYS: u64 = 30;

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConsentState {
    #[default]
    Unset,
    Granted,
    Denied,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstallIdRecord {
    pub id: Uuid,
    pub generated_day: u64,
}

impl InstallIdRecord {
    pub fn new(now_day: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            generated_day: now_day,
        }
    }

    pub fn is_due_for_rotation(&self, now_day: u64) -> bool {
        now_day.saturating_sub(self.generated_day) >= INSTALL_ID_ROTATION_DAYS
    }

    /// Rotation regenerates a fresh random v4 id; it never derives one from the
    /// old id, clock or machine identifiers.
    pub fn rotate(&mut self, now_day: u64) {
        *self = Self::new(now_day);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub consent: ConsentState,
    pub install: Option<InstallIdRecord>,
}

impl TelemetryConfig {
    /// Missing or unreadable files mean "no consent, new random id" — never a
    /// shared or derived identifier.
    pub fn load(path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&contents).unwrap_or_default()
    }

    /// Loads configuration, creating or rotating the install id as needed.
    /// Write failures are ignored: telemetry must never affect the host app.
    pub fn load_or_create(path: &Path, now_day: u64) -> Self {
        let mut config = Self::load(path);
        let rotate = config
            .install
            .as_ref()
            .is_none_or(|record| record.is_due_for_rotation(now_day));
        if rotate {
            config.install = Some(InstallIdRecord::new(now_day));
            let _ = config.save(path);
        }
        config
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &contents).map_err(|error| format!("{}: {error}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn install_id(&self) -> Option<Uuid> {
        self.install.as_ref().map(|record| record.id)
    }

    pub fn set_consent(&mut self, consent: ConsentState) {
        self.consent = consent;
    }

    /// Settings > Privacy: "Reset install ID" (Phase 4).
    pub fn reset_install_id(&mut self, now_day: u64) -> Uuid {
        let record = InstallIdRecord::new(now_day);
        let id = record.id;
        self.install = Some(record);
        id
    }
}

/// Whole UTC days since the Unix epoch. Stored locally only; never sent.
pub fn now_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "anarc-telemetry-config-{name}-{}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn default_is_unset_without_an_id() {
        let config = TelemetryConfig::default();
        assert_eq!(config.consent, ConsentState::Unset);
        assert!(config.install_id().is_none());
    }

    #[test]
    fn install_id_is_a_random_v4_uuid_and_persists() {
        let path = temp_config_path("persist");
        let _ = std::fs::remove_file(&path);
        let created = TelemetryConfig::load_or_create(&path, 10_000);
        let id = created.install_id().expect("id created");
        assert_eq!(id.get_version_num(), 4);
        assert_eq!(
            created.install.as_ref().expect("record").generated_day,
            10_000
        );
        let loaded = TelemetryConfig::load(&path);
        assert_eq!(loaded.install_id(), Some(id));
        assert_eq!(loaded.consent, ConsentState::Unset);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rotation_regenerates_instead_of_deriving() {
        let mut record = InstallIdRecord::new(1_000);
        let before = record.id;
        assert!(!record.is_due_for_rotation(1_000 + 29));
        assert!(record.is_due_for_rotation(1_000 + 30));
        record.rotate(1_030);
        assert_ne!(record.id, before);
        assert_eq!(record.id.get_version_num(), 4);
        assert_eq!(record.generated_day, 1_030);
    }

    #[test]
    fn reset_install_id_is_a_fresh_uuid() {
        let path = temp_config_path("reset");
        let _ = std::fs::remove_file(&path);
        let mut config = TelemetryConfig::load_or_create(&path, 10_000);
        let before = config.install_id().expect("id created");
        let after = config.reset_install_id(10_001);
        assert_ne!(after, before);
        assert_eq!(after.get_version_num(), 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_config_falls_back_to_unset() {
        let path = temp_config_path("corrupt");
        std::fs::write(&path, "{ \"consent\": \"yes\", \"install\": 12 }").expect("write");
        let config = TelemetryConfig::load(&path);
        assert_eq!(config.consent, ConsentState::Unset);
        assert!(config.install_id().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn consent_round_trips_through_json() {
        let path = temp_config_path("consent");
        let _ = std::fs::remove_file(&path);
        let mut config = TelemetryConfig::load_or_create(&path, 42);
        config.set_consent(ConsentState::Granted);
        config.save(&path).expect("save");
        assert_eq!(TelemetryConfig::load(&path).consent, ConsentState::Granted);
        let _ = std::fs::remove_file(&path);
    }
}
