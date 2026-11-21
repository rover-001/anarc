use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const LIVE_CURSOR_PORT: u16 = 7434;

pub const CURSOR_SVG: &[u8] = br##"<svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M5.5 3.5L18.5 11.5L12 13.5L9.5 20L5.5 3.5Z" fill="currentColor" stroke="#FFFFFF" stroke-width="1.5" stroke-linejoin="round"/>
</svg>"##;

pub fn peer_color(name: &str) -> gpui_kit::Rgba {
    let hash = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    const PALETTE: [u32; 8] = [
        0x6366f1, // Indigo
        0xec4899, // Pink
        0x10b981, // Emerald
        0xf59e0b, // Amber
        0x8b5cf6, // Violet
        0x06b6d4, // Cyan
        0xf97316, // Orange
        0x3b82f6, // Blue
    ];
    let hex = PALETTE[(hash as usize) % PALETTE.len()];
    gpui_kit::rgb(hex)
}

#[derive(Clone, Debug)]
pub struct PeerCursor {
    pub x: f32, // 0.0 .. 1.0
    pub y: f32, // 0.0 .. 1.0
    pub name: String,
    pub typing_text: Option<String>,
    pub typing_updated: Option<Instant>,
    pub last_seen: Instant,
}

pub struct LiveCursorState {
    socket: Option<Arc<UdpSocket>>,
    cursors: Arc<Mutex<HashMap<String, PeerCursor>>>,
    last_send: Mutex<Instant>,
    last_pos: Mutex<(f32, f32)>,
    local_typing: Mutex<Option<(String, Instant)>>,
    local_name: String,
}

static LIVE_CURSORS: OnceLock<LiveCursorState> = OnceLock::new();

pub fn init_live_cursors(wake_tx: async_channel::Sender<()>) {
    LIVE_CURSORS.get_or_init(|| {
        let local_name = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Peer".into());

        let cursors = Arc::new(Mutex::new(HashMap::<String, PeerCursor>::new()));
        let socket = match UdpSocket::bind(format!("0.0.0.0:{LIVE_CURSOR_PORT}")) {
            Ok(s) => {
                let arc_socket = Arc::new(s);
                let s_clone = Arc::clone(&arc_socket);
                let cursors_clone = Arc::clone(&cursors);
                let wake_clone = wake_tx.clone();

                std::thread::spawn(move || {
                    let mut buf = [0u8; 512];
                    while let Ok((len, src)) = s_clone.recv_from(&mut buf) {
                        if len >= 10 && buf[0] == b'A' && buf[1] == b'C' {
                            let x = f32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]);
                            let y = f32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
                            let name = if len > 10 {
                                String::from_utf8_lossy(&buf[10..len]).to_string()
                            } else {
                                src.ip().to_string()
                            };
                            let peer_key = if src.ip().is_loopback() {
                                name.clone()
                            } else {
                                format!("{}:{}", src.ip(), name)
                            };
                            if let Ok(mut map) = cursors_clone.lock() {
                                let (existing_typing, existing_updated) = map
                                    .get(&peer_key)
                                    .and_then(|p| {
                                        p.typing_updated.and_then(|ts| {
                                            if Instant::now().duration_since(ts)
                                                < Duration::from_millis(3500)
                                            {
                                                Some((p.typing_text.clone(), Some(ts)))
                                            } else {
                                                None
                                            }
                                        })
                                    })
                                    .unwrap_or((None, None));

                                map.insert(
                                    peer_key,
                                    PeerCursor {
                                        x: x.clamp(0.0, 1.0),
                                        y: y.clamp(0.0, 1.0),
                                        name,
                                        typing_text: existing_typing,
                                        typing_updated: existing_updated,
                                        last_seen: Instant::now(),
                                    },
                                );
                            }
                            let _ = wake_clone.try_send(());
                        } else if len >= 12 && buf[0] == b'A' && buf[1] == b'T' {
                            let x = f32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]);
                            let y = f32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
                            let text_len = u16::from_le_bytes([buf[10], buf[11]]) as usize;
                            let (typing_text, name) = if len >= 12 + text_len {
                                let text_bytes = &buf[12..12 + text_len];
                                let t = String::from_utf8_lossy(text_bytes).to_string();
                                let n = if len > 12 + text_len {
                                    String::from_utf8_lossy(&buf[12 + text_len..len]).to_string()
                                } else {
                                    src.ip().to_string()
                                };
                                (if t.is_empty() { None } else { Some(t) }, n)
                            } else {
                                (None, src.ip().to_string())
                            };
                            let peer_key = if src.ip().is_loopback() {
                                name.clone()
                            } else {
                                format!("{}:{}", src.ip(), name)
                            };
                            if let Ok(mut map) = cursors_clone.lock() {
                                map.insert(
                                    peer_key,
                                    PeerCursor {
                                        x: x.clamp(0.0, 1.0),
                                        y: y.clamp(0.0, 1.0),
                                        name,
                                        typing_text,
                                        typing_updated: Some(Instant::now()),
                                        last_seen: Instant::now(),
                                    },
                                );
                            }
                            let _ = wake_clone.try_send(());
                        }
                    }
                });

                Some(arc_socket)
            }
            Err(_) => None,
        };

        LiveCursorState {
            socket,
            cursors,
            last_send: Mutex::new(Instant::now()),
            last_pos: Mutex::new((0.5, 0.5)),
            local_typing: Mutex::new(None),
            local_name,
        }
    });
}

fn send_packet(x: f32, y: f32, typing: Option<&str>, peer_ips: &[String]) {
    let Some(state) = LIVE_CURSORS.get() else {
        return;
    };
    let Some(ref socket) = state.socket else {
        return;
    };
    if peer_ips.is_empty() {
        return;
    }

    let packet = match typing {
        Some(text) if !text.is_empty() => {
            let text_bytes = text.as_bytes();
            let text_len = (text_bytes.len().min(u16::MAX as usize)) as u16;
            let mut p = Vec::with_capacity(12 + text_len as usize + state.local_name.len());
            p.extend_from_slice(b"AT");
            p.extend_from_slice(&x.to_le_bytes());
            p.extend_from_slice(&y.to_le_bytes());
            p.extend_from_slice(&text_len.to_le_bytes());
            p.extend_from_slice(&text_bytes[..text_len as usize]);
            p.extend_from_slice(state.local_name.as_bytes());
            p
        }
        _ => {
            let mut p = Vec::with_capacity(10 + state.local_name.len());
            p.extend_from_slice(b"AC");
            p.extend_from_slice(&x.to_le_bytes());
            p.extend_from_slice(&y.to_le_bytes());
            p.extend_from_slice(state.local_name.as_bytes());
            p
        }
    };

    for ip in peer_ips {
        let dest = format!("{ip}:{LIVE_CURSOR_PORT}");
        let _ = socket.send_to(&packet, dest);
    }
}

pub fn broadcast_cursor(x: f32, y: f32, peer_ips: &[String]) {
    let Some(state) = LIVE_CURSORS.get() else {
        return;
    };
    if peer_ips.is_empty() {
        return;
    }

    if let Ok(mut last) = state.last_send.lock() {
        if last.elapsed() < Duration::from_millis(30) {
            return;
        }
        *last = Instant::now();
    }
    if let Ok(mut pos) = state.last_pos.lock() {
        *pos = (x, y);
    }

    let active_typing = if let Ok(typing) = state.local_typing.lock() {
        typing.as_ref().and_then(|(text, ts)| {
            if ts.elapsed() < Duration::from_millis(3500) {
                Some(text.clone())
            } else {
                None
            }
        })
    } else {
        None
    };

    send_packet(x, y, active_typing.as_deref(), peer_ips);
}

pub fn record_local_keystroke(key: &str, is_ctrl_or_alt: bool, peer_ips: &[String]) {
    let Some(state) = LIVE_CURSORS.get() else {
        return;
    };
    if peer_ips.is_empty() || is_ctrl_or_alt {
        return;
    }

    let mut typing_to_send = None;
    if let Ok(mut typing) = state.local_typing.lock() {
        let now = Instant::now();
        if let Some((_, ts)) = typing.as_ref() {
            if now.duration_since(*ts) > Duration::from_millis(4000) {
                *typing = None;
            }
        }

        match key.to_lowercase().as_str() {
            "escape" => {
                *typing = None;
            }
            "enter" => {
                if let Some((current, _)) = typing.as_mut() {
                    let finished = format!("{current} ⏎");
                    *typing = Some((finished.clone(), now));
                    typing_to_send = Some(finished);
                }
            }
            "backspace" => {
                if let Some((current, ts)) = typing.as_mut() {
                    current.pop();
                    *ts = now;
                    if current.is_empty() {
                        *typing = None;
                    } else {
                        typing_to_send = Some(current.clone());
                    }
                }
            }
            "space" => {
                let current = match typing.as_mut() {
                    Some((buf, ts)) => {
                        buf.push(' ');
                        *ts = now;
                        buf.clone()
                    }
                    None => {
                        let buf = " ".to_string();
                        *typing = Some((buf.clone(), now));
                        buf
                    }
                };
                typing_to_send = Some(current);
            }
            single_key => {
                if single_key.chars().count() == 1 {
                    let ch = key.chars().next().unwrap_or(' ');
                    let current = match typing.as_mut() {
                        Some((buf, ts)) => {
                            if buf.len() > 60 {
                                let start = buf.len() - 40;
                                *buf = buf[start..].to_string();
                            }
                            buf.push(ch);
                            *ts = now;
                            buf.clone()
                        }
                        None => {
                            let mut buf = String::new();
                            buf.push(ch);
                            *typing = Some((buf.clone(), now));
                            buf
                        }
                    };
                    typing_to_send = Some(current);
                }
            }
        }
    }

    let (x, y) = state
        .last_pos
        .lock()
        .map(|p| *p)
        .unwrap_or((0.5, 0.5));

    send_packet(x, y, typing_to_send.as_deref(), peer_ips);
}

pub fn set_local_typing_text(text: Option<String>, peer_ips: &[String]) {
    let Some(state) = LIVE_CURSORS.get() else {
        return;
    };
    if peer_ips.is_empty() {
        return;
    }
    if let Ok(mut typing) = state.local_typing.lock() {
        *typing = text.as_ref().map(|t| (t.clone(), Instant::now()));
    }
    let (x, y) = state
        .last_pos
        .lock()
        .map(|p| *p)
        .unwrap_or((0.5, 0.5));
    send_packet(x, y, text.as_deref(), peer_ips);
}

pub fn get_active_peer_cursors() -> Vec<PeerCursor> {
    let Some(state) = LIVE_CURSORS.get() else {
        return Vec::new();
    };
    let Ok(map) = state.cursors.lock() else {
        return Vec::new();
    };
    let now = Instant::now();
    map.values()
        .filter(|c| {
            now.duration_since(c.last_seen) < Duration::from_secs(3)
                && c.name != state.local_name
        })
        .map(|c| {
            let mut cursor = c.clone();
            if let Some(updated) = cursor.typing_updated {
                if now.duration_since(updated) > Duration::from_millis(3500) {
                    cursor.typing_text = None;
                }
            }
            cursor
        })
        .collect()
}
