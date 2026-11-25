//! Opt-in, privacy-safe daily telemetry for the Anarc agent-control layer.
//!
//! Nothing is sent unless consent is `Granted` and no kill switch is active.
//! The wire schema lives in `schema.rs`; no page-derived string can enter any
//! telemetry code path. See `TELEMETRY.md` at the repository root.

pub mod buckets;
pub mod config;
pub mod counters;
pub mod rollup;
pub mod schema;
pub mod sender;

use std::sync::atomic::{AtomicBool, Ordering};

static CLI_DISABLED: AtomicBool = AtomicBool::new(false);

/// `--no-telemetry` is parsed once at startup and stored here. The send site is
/// the only place that reads it.
pub fn set_cli_kill_switch(disabled: bool) {
    CLI_DISABLED.store(disabled, Ordering::Release);
}

pub fn cli_kill_switch() -> bool {
    CLI_DISABLED.load(Ordering::Acquire)
}

pub use config::{ConsentState, TelemetryConfig, now_day};
pub use counters::Counters;
pub use schema::{Ping, SCHEMA_VERSION};
pub use sender::{HttpTransport, SendGate, Transport, send_ping};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_kill_switch_wiring() {
        set_cli_kill_switch(true);
        assert!(cli_kill_switch());
        set_cli_kill_switch(false);
        assert!(!cli_kill_switch());
    }
}
