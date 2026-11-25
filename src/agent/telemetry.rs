//! Agent-control telemetry bridge. All measurements here are lengths, counts,
//! enums and durations; no page string ever enters this module.

use anarc_telemetry::config::{ConsentState, TelemetryConfig};
use anarc_telemetry::rollup::RollupStore;
use anarc_telemetry::schema::{Arch as CpuArch, Distro, Ping, SCHEMA_VERSION, ping_json};
use anarc_telemetry::sender::{HttpTransport, INGEST_URL, SendGate, send_ping};
use anarc_telemetry::{Counters, now_day};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct AgentTelemetry {
    pub counters: Counters,
    last_flush_day: AtomicU64,
    config_path: PathBuf,
    rollup_dir: PathBuf,
}

impl Default for AgentTelemetry {
    fn default() -> Self {
        Self::with_paths(
            crate::paths::config_dir().join("telemetry.json"),
            crate::paths::data_dir().join("telemetry"),
        )
    }
}

impl AgentTelemetry {
    pub fn with_paths(config_path: PathBuf, rollup_dir: PathBuf) -> Self {
        Self {
            counters: Counters::new(),
            last_flush_day: AtomicU64::new(0),
            config_path,
            rollup_dir,
        }
    }

    /// Builds the exact ping that would be sent next. The consent preview and
    /// the network body both use this struct and canonical serialization.
    pub fn build_ping(&self, app_version: &str, day: u64) -> Option<Ping> {
        let config = TelemetryConfig::load_or_create(&self.config_path, day);
        let install_id = config.install_id()?;
        let arch = arch_from_target(std::env::consts::ARCH)?;
        Some(Ping {
            schema_version: SCHEMA_VERSION,
            app_version: app_version.to_owned(),
            distro: detect_distro(),
            arch,
            install_id,
            agent: Some(self.counters.stats()),
        })
    }

    /// Best-effort, once per UTC day. Consent and all kill switches are
    /// evaluated at the single send site inside `send_ping`.
    pub fn flush_daily(&self, app_version: &str) {
        let day = now_day();
        if self.last_flush_day.load(Ordering::Acquire) == day {
            return;
        }
        let config = TelemetryConfig::load_or_create(&self.config_path, day);
        if config.consent != ConsentState::Granted {
            return;
        }
        if self.last_flush_day.swap(day, Ordering::AcqRel) == day {
            return;
        }
        let Some(ping) = self.build_ping(app_version, day) else {
            return;
        };
        let _ = RollupStore::new(self.rollup_dir.clone()).write(day, &ping);
        let _ = send_ping(
            &ping,
            &SendGate::current(config.consent),
            Arc::new(HttpTransport::new(INGEST_URL)),
        );
    }
}

static TELEMETRY: OnceLock<AgentTelemetry> = OnceLock::new();

pub(crate) fn telemetry() -> &'static AgentTelemetry {
    TELEMETRY.get_or_init(AgentTelemetry::default)
}

pub(crate) fn flush_daily(app_version: &str) {
    telemetry().flush_daily(app_version);
}

// Settings > Privacy / first-run consent helpers. No UI code reads or writes
// the config file directly.
pub(crate) fn consent_state() -> ConsentState {
    TelemetryConfig::load(&crate::paths::config_dir().join("telemetry.json")).consent
}

pub(crate) fn set_consent(consent: ConsentState) -> Result<(), String> {
    let path = crate::paths::config_dir().join("telemetry.json");
    let mut config = TelemetryConfig::load_or_create(&path, now_day());
    config.set_consent(consent);
    config.save(&path)
}

pub(crate) fn reset_install_id() -> Result<(), String> {
    let path = crate::paths::config_dir().join("telemetry.json");
    let mut config = TelemetryConfig::load_or_create(&path, now_day());
    config.reset_install_id(now_day());
    config.save(&path)
}

/// Exact JSON of the next ping, for Settings > Privacy "Show what would be sent".
pub(crate) fn preview_json() -> String {
    let day = now_day();
    telemetry()
        .build_ping(env!("CARGO_PKG_VERSION"), day)
        .and_then(|ping| ping_json(&ping).ok())
        .unwrap_or_default()
}

fn arch_from_target(target: &str) -> Option<CpuArch> {
    match target {
        "x86_64" => Some(CpuArch::X86_64),
        "aarch64" => Some(CpuArch::Aarch64),
        _ => None,
    }
}

fn distro_from_id(id: &str) -> Distro {
    match id.to_ascii_lowercase().as_str() {
        "arch" | "archlinux" => Distro::Arch,
        "ubuntu" => Distro::Ubuntu,
        "fedora" => Distro::Fedora,
        "debian" => Distro::Debian,
        "nixos" => Distro::Nixos,
        _ => Distro::Other,
    }
}

fn detect_distro() -> Distro {
    let Ok(contents) = std::fs::read_to_string("/etc/os-release") else {
        return Distro::Other;
    };
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("ID=") {
            return distro_from_id(&value.trim().trim_matches('"').to_ascii_lowercase());
        }
    }
    Distro::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_and_distro_mapping_is_enum_only() {
        assert_eq!(arch_from_target("x86_64"), Some(CpuArch::X86_64));
        assert_eq!(arch_from_target("aarch64"), Some(CpuArch::Aarch64));
        assert_eq!(arch_from_target("riscv64"), None);
        assert_eq!(distro_from_id("arch"), Distro::Arch);
        assert_eq!(distro_from_id("NIXOS"), Distro::Nixos);
        assert_eq!(distro_from_id("unknown"), Distro::Other);
    }

    #[test]
    fn no_consent_means_no_rollup_file() {
        let dir =
            std::env::temp_dir().join(format!("anarc-agent-telemetry-{}", std::process::id()));
        let config_path = dir.join("telemetry.json");
        let rollup_dir = dir.join("rollup");
        let _ = std::fs::remove_dir_all(&dir);
        let telemetry = AgentTelemetry::with_paths(config_path.clone(), rollup_dir.clone());
        telemetry.flush_daily("0.1.0");
        assert!(!rollup_dir.exists(), "nothing is written before consent");
        let config = TelemetryConfig::load(&config_path);
        assert_eq!(config.consent, ConsentState::Unset);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_ping_bytes_match_preview() {
        let dir =
            std::env::temp_dir().join(format!("anarc-agent-telemetry-preview-{}", std::process::id()));
        let config_path = dir.join("telemetry.json");
        let rollup_dir = dir.join("rollup");
        let _ = std::fs::remove_dir_all(&dir);
        let mut config = TelemetryConfig::load_or_create(&config_path, 20_000);
        config.set_consent(ConsentState::Granted);
        config.save(&config_path).ok();
        let telemetry = AgentTelemetry::with_paths(config_path, rollup_dir);
        telemetry.counters.note_session();
        assert!(telemetry.build_ping("0.1.0", 20_000).is_some());
        let Some(ping) = telemetry.build_ping("0.1.0", 20_000) else {
            return;
        };
        let preview = RollupStore::render_pending(&ping).unwrap_or_default();
        assert_eq!(ping_json(&ping).unwrap_or_default(), preview);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
