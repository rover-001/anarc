//! Discord Rich Presence over Discord's local RPC socket.
//!
//! The plugin owns a blocking worker thread so no IPC traffic ever touches the
//! GPUI loop. The UI side only pushes small, deduplicated activity updates
//! through [`PresenceService`].

use crate::plugin::{Manifest, Plugin, PluginCtx, PluginError, ServiceKey};
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_CLIENT_ID: &str = "1574842938708467825";
const ASSET_KEY: &str = "anarc-logo";
const MAX_FRAME: usize = 64 * 1024;

pub(crate) static PRESENCE: ServiceKey<PresenceService> = ServiceKey::new("anarc.discord-presence");

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresenceUpdate {
    pub(crate) details: String,
    pub(crate) state: String,
}

#[derive(Clone)]
pub(crate) struct PresenceService {
    tx: async_channel::Sender<PresenceUpdate>,
    last: RefCell<Option<PresenceUpdate>>,
}

impl PresenceService {
    pub(crate) fn update(&self, next: PresenceUpdate) {
        if self.last.borrow().as_ref() == Some(&next) {
            return;
        }
        *self.last.borrow_mut() = Some(next.clone());
        let _ = self.tx.try_send(next);
    }
}

pub(crate) struct PresencePlugin {
    manifest: Manifest,
}

impl Default for PresencePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl PresencePlugin {
    pub(crate) fn new() -> Self {
        Self {
            manifest: Manifest {
                id: "anarc.discord-presence".to_owned(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: Vec::new(),
            },
        }
    }
}

impl Plugin for PresencePlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let client_id = std::env::var("ANARC_DISCORD_CLIENT_ID")
            .unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_owned());
        if client_id.trim().is_empty() {
            return Ok(());
        }
        let (tx, rx) = async_channel::unbounded::<PresenceUpdate>();
        if std::thread::Builder::new()
            .name("anarc-discord-presence".into())
            .spawn(move || run(client_id, rx))
            .is_err()
        {
            return Err(PluginError("Discord presence worker unavailable".into()));
        }
        // Dropping the service on unmount closes the channel; the worker sends
        // a clear-activity frame and exits.
        ctx.provide(
            PRESENCE.clone(),
            PresenceService {
                tx,
                last: RefCell::new(None),
            },
        );
        Ok(())
    }
}

#[cfg(unix)]
fn run(client_id: String, rx: async_channel::Receiver<PresenceUpdate>) {
    let started = unix_now();
    loop {
        if rx.is_closed() {
            return;
        }
        let Some(stream) = connect() else {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        };
        let mut current = None;
        let _ = serve(stream, &rx, &mut current, &client_id, started);
        if rx.is_closed() {
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

#[cfg(not(unix))]
fn run(_client_id: String, _rx: async_channel::Receiver<PresenceUpdate>) {}

#[cfg(unix)]
fn connect() -> Option<std::os::unix::net::UnixStream> {
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;

    let mut bases = Vec::new();
    for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(dir) = std::env::var_os(key) {
            bases.push(PathBuf::from(dir));
        }
    }
    bases.push(PathBuf::from("/tmp"));
    for base in bases {
        for index in 0..10 {
            let path = base.join(format!("discord-ipc-{index}"));
            if let Ok(stream) = UnixStream::connect(path) {
                return Some(stream);
            }
        }
    }
    None
}

#[cfg(unix)]
fn serve(
    mut stream: std::os::unix::net::UnixStream,
    rx: &async_channel::Receiver<PresenceUpdate>,
    current: &mut Option<PresenceUpdate>,
    client_id: &str,
    started: u64,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write_frame(
        &mut stream,
        0,
        &serde_json::json!({ "v": 1, "client_id": client_id }),
    )?;

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Discord handshake timeout",
            ));
        }
        match read_frame(&mut stream) {
            Ok((1, _)) => break,
            Ok((2, _)) => return Ok(()),
            Ok((3, value)) => send_pong(&mut stream, &value)?,
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error),
            _ => {}
        }
    }

    let mut sent: Option<PresenceUpdate> = None;
    let mut nonce = 0u64;
    loop {
        while let Ok(update) = rx.try_recv() {
            *current = Some(update);
        }
        if rx.is_closed() {
            nonce += 1;
            write_frame(
                &mut stream,
                1,
                &serde_json::json!({
                    "cmd": "SET_ACTIVITY",
                    "args": { "pid": std::process::id(), "activity": null },
                    "nonce": make_nonce(nonce),
                }),
            )?;
            return Ok(());
        }
        if current.as_ref() != sent.as_ref()
            && let Some(update) = current.as_ref()
        {
            nonce += 1;
            write_frame(&mut stream, 1, &activity_payload(update, started, nonce))?;
            sent = Some(update.clone());
        }
        match read_frame(&mut stream) {
            Ok((2, _)) => return Ok(()),
            Ok((3, value)) => send_pong(&mut stream, &value)?,
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error),
            _ => {}
        }
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn send_pong(stream: &mut impl Write, value: &serde_json::Value) -> io::Result<()> {
    let nonce = value
        .get("nonce")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    write_frame(stream, 4, &serde_json::json!({ "nonce": nonce }))
}

fn write_frame(stream: &mut impl Write, opcode: u32, value: &serde_json::Value) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(|error| io::Error::other(error.to_string()))?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RPC frame too large",
        ));
    }
    let mut header = Vec::with_capacity(8);
    header.extend_from_slice(&opcode.to_le_bytes());
    header.extend_from_slice(&(body.len() as u32).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(&body)?;
    Ok(())
}

fn read_frame(stream: &mut impl Read) -> io::Result<(u32, serde_json::Value)> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RPC frame too large",
        ));
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    Ok((opcode, value))
}

fn activity_payload(update: &PresenceUpdate, started: u64, nonce: u64) -> serde_json::Value {
    serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id(),
            "activity": {
                "type": 0,
                "details": update.details,
                "state": update.state,
                "timestamps": { "start": started },
                "assets": { "large_image": ASSET_KEY, "large_text": "Anarc" }
            }
        },
        "nonce": make_nonce(nonce),
    })
}

fn make_nonce(counter: u64) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}-{counter:x}")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn handshake_activity_and_clear_frames() {
        let (client, mut server) = UnixStream::pair().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (tx, rx) = async_channel::unbounded::<PresenceUpdate>();
        let worker = std::thread::spawn(move || {
            let mut current = None;
            serve(
                client,
                &rx,
                &mut current,
                "1574842938708467825",
                1_700_000_000,
            )
            .unwrap();
        });

        let (opcode, handshake) = read_frame(&mut server).unwrap();
        assert_eq!(opcode, 0);
        assert_eq!(handshake["client_id"], "1574842938708467825");
        assert_eq!(handshake["v"], 1);

        write_frame(&mut server, 1, &serde_json::json!({ "evt": "READY" })).unwrap();
        tx.try_send(PresenceUpdate {
            details: "YouTube".into(),
            state: "youtube.com".into(),
        })
        .unwrap();

        let (opcode, activity) = read_frame(&mut server).unwrap();
        assert_eq!(opcode, 1);
        assert_eq!(activity["cmd"], "SET_ACTIVITY");
        assert_eq!(activity["args"]["activity"]["details"], "YouTube");
        assert_eq!(activity["args"]["activity"]["state"], "youtube.com");
        assert_eq!(
            activity["args"]["activity"]["assets"]["large_image"],
            "anarc-logo"
        );
        assert_eq!(
            activity["args"]["activity"]["timestamps"]["start"],
            1_700_000_000
        );

        write_frame(&mut server, 3, &serde_json::json!({ "nonce": "ping-1" })).unwrap();
        let (opcode, pong) = read_frame(&mut server).unwrap();
        assert_eq!(opcode, 4);
        assert_eq!(pong["nonce"], "ping-1");

        drop(tx);
        let (opcode, clear) = read_frame(&mut server).unwrap();
        assert_eq!(opcode, 1);
        assert_eq!(clear["cmd"], "SET_ACTIVITY");
        assert!(clear["args"]["activity"].is_null());
        worker.join().unwrap();
    }

    #[test]
    fn service_deduplicates_updates() {
        let (tx, rx) = async_channel::unbounded::<PresenceUpdate>();
        let service = PresenceService {
            tx,
            last: RefCell::new(None),
        };
        let update = PresenceUpdate {
            details: "A".into(),
            state: "B".into(),
        };
        service.update(update.clone());
        service.update(update);
        assert_eq!(rx.len(), 1);
    }
}
