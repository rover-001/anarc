//! The single send site. Consent and every kill switch are checked here, and
//! nowhere else. Transport failures are ignored: telemetry never blocks or
//! fails the browser UI or agent calls.

use crate::config::ConsentState;
use crate::schema::{Ping, ping_json};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

pub const INGEST_URL: &str = "https://amicable-sparrow-936.convex.site/ping";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub trait Transport: Send + Sync {
    fn post(&self, url: &str, body: &[u8]) -> Result<(), String>;
}

/// `reqwest` client is built lazily on the worker thread so TLS setup never
/// touches the UI or agent threads. No retry logic exists by design.
pub struct HttpTransport {
    url: String,
    client: OnceLock<reqwest::blocking::Client>,
}

impl HttpTransport {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: OnceLock::new(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    fn client(&self) -> Result<&reqwest::blocking::Client, String> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?;
        let _ = self.client.set(client);
        self.client
            .get()
            .ok_or_else(|| "telemetry client unavailable".to_owned())
    }
}

impl Transport for HttpTransport {
    fn post(&self, url: &str, body: &[u8]) -> Result<(), String> {
        let client = self.client()?;
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_vec())
            .send()
            .map_err(|error| error.to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("unexpected status {}", response.status().as_u16()))
        }
    }
}

/// Exact-match env kill switch: only `ANARC_TELEMETRY=0` disables telemetry.
pub fn env_disabled(value: Option<&str>) -> bool {
    value == Some("0")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendGate {
    pub consent: ConsentState,
    pub cli_disabled: bool,
    pub env_disabled: bool,
}

impl SendGate {
    pub fn current(consent: ConsentState) -> Self {
        Self {
            consent,
            cli_disabled: super::cli_kill_switch(),
            env_disabled: env_disabled(std::env::var("ANARC_TELEMETRY").ok().as_deref()),
        }
    }

    /// One predicate for all three kill switches. Any one of them wins.
    pub fn enabled(&self) -> bool {
        self.consent == ConsentState::Granted && !self.cli_disabled && !self.env_disabled
    }
}

/// Schedules at most one fire-and-forget POST. Returns `true` only if consent
/// and all kill switches allowed the send to be scheduled.
pub fn send_ping(ping: &Ping, gate: &SendGate, transport: Arc<dyn Transport>) -> bool {
    if !gate.enabled() {
        return false;
    }
    let Ok(body) = ping_json(ping) else {
        return false;
    };
    let url = INGEST_URL.to_owned();
    std::thread::Builder::new()
        .name("anarc-telemetry-send".into())
        .spawn(move || {
            // Deliberately discard the result: no retries, no logs with data.
            let _ = transport.post(&url, body.as_bytes());
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rollup::RollupStore;
    use crate::schema::{AgentStats, Arch, Distro};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;
    use uuid::Uuid;

    struct CaptureTransport {
        calls: AtomicUsize,
        tx: mpsc::Sender<Vec<u8>>,
    }

    impl Transport for CaptureTransport {
        fn post(&self, _url: &str, body: &[u8]) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.tx.send(body.to_vec());
            Ok(())
        }
    }

    fn ping_for_test() -> Ping {
        Ping {
            schema_version: 1,
            app_version: "0.1.0".into(),
            distro: Distro::Arch,
            arch: Arch::X86_64,
            install_id: Uuid::from_u128(9),
            agent: Some(AgentStats {
                enabled: true,
                harness: Vec::new(),
                sessions: crate::schema::Bucket::new(0),
                calls_by_action: Default::default(),
                snapshot_token_ratio: crate::schema::Bucket::new(0),
                snapshot_ms: crate::schema::Bucket::new(0),
                element_count: crate::schema::Bucket::new(0),
                ref_resolve_success: crate::schema::Bucket::new(0),
                errors_by_code: Default::default(),
                plugins_installed: crate::schema::Bucket::new(0),
            }),
        }
    }

    fn gate(consent: ConsentState, cli: bool, env: bool) -> SendGate {
        SendGate {
            consent,
            cli_disabled: cli,
            env_disabled: env,
        }
    }

    #[test]
    fn nothing_is_sent_when_any_kill_switch_wins() {
        let cases = [
            gate(ConsentState::Unset, false, false),
            gate(ConsentState::Denied, false, false),
            gate(ConsentState::Granted, true, false),
            gate(ConsentState::Granted, false, true),
            gate(ConsentState::Denied, true, true),
        ];
        for case in cases {
            let (tx, rx) = mpsc::channel();
            let transport = Arc::new(CaptureTransport {
                calls: AtomicUsize::new(0),
                tx,
            });
            assert!(!send_ping(&ping_for_test(), &case, transport.clone()));
            assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
            assert!(rx.try_recv().is_err());
        }
    }

    #[test]
    fn granted_ping_matches_preview_byte_for_byte() {
        let ping = ping_for_test();
        let preview = RollupStore::render_pending(&ping).expect("render");
        let (tx, rx) = mpsc::channel();
        let transport = Arc::new(CaptureTransport {
            calls: AtomicUsize::new(0),
            tx,
        });
        assert!(send_ping(
            &ping,
            &gate(ConsentState::Granted, false, false),
            transport.clone()
        ));
        let received = rx.recv_timeout(Duration::from_secs(2)).expect("posted");
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(received, preview.as_bytes());
        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "no retries"
        );
    }

    #[test]
    fn env_switch_only_matches_zero() {
        assert!(env_disabled(Some("0")));
        assert!(!env_disabled(Some("1")));
        assert!(!env_disabled(Some("false")));
        assert!(!env_disabled(None));
    }
}
