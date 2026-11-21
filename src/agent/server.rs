use super::protocol::{AgentRequest, AgentResponse};
use std::fs::{create_dir_all, remove_file, set_permissions, Permissions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static RUNNING: AtomicBool = AtomicBool::new(false);
static ACTIVE_SOCKET_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[repr(C)]
struct UCred {
    pid: i32,
    uid: u32,
    gid: u32,
}

unsafe extern "C" {
    fn getsockopt(
        sockfd: std::os::raw::c_int,
        level: std::os::raw::c_int,
        optname: std::os::raw::c_int,
        optval: *mut std::os::raw::c_void,
        optlen: *mut std::os::raw::c_uint,
    ) -> std::os::raw::c_int;
    fn getuid() -> u32;
}

pub fn verify_peer_uid(stream: &UnixStream) -> bool {
    let mut cred = UCred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<UCred>() as std::os::raw::c_uint;
    let fd = stream.as_raw_fd();
    let ret = unsafe {
        getsockopt(
            fd,
            1, // SOL_SOCKET
            17, // SO_PEERCRED
            &mut cred as *mut UCred as *mut std::os::raw::c_void,
            &mut len,
        )
    };
    if ret == 0 {
        let my_uid = unsafe { getuid() };
        cred.uid == my_uid
    } else {
        false
    }
}

pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("ANARC_SOCKET_PATH") {
        return PathBuf::from(path);
    }
    if let Ok(data_dir) = std::env::var("ANARC_DATA_DIR") {
        return PathBuf::from(data_dir).join("agent").join("ctl.sock");
    }
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("anarc-agent.sock");
    }
    let uid = unsafe { getuid() };
    PathBuf::from(format!("/tmp/anarc-agent-{uid}.sock"))
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

pub fn start_server(
    handler_tx: async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
) -> Result<PathBuf, String> {
    if RUNNING.load(Ordering::SeqCst) {
        if let Ok(guard) = ACTIVE_SOCKET_PATH.lock() {
            if let Some(ref p) = *guard {
                return Ok(p.clone());
            }
        }
    }

    let socket_path = default_socket_path();
    if let Some(parent) = socket_path.parent() {
        let _ = create_dir_all(parent);
    }
    let _ = remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)
        .map_err(|_| "socket unavailable. use: status")?;

    let _ = set_permissions(&socket_path, Permissions::from_mode(0o600));

    RUNNING.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = ACTIVE_SOCKET_PATH.lock() {
        *guard = Some(socket_path.clone());
    }

    let server = std::thread::Builder::new()
        .name("anarc-agent-server".into())
        .spawn(move || {
            let _ = listener.set_nonblocking(true);
            while RUNNING.load(Ordering::SeqCst) && !handler_tx.is_closed() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if !verify_peer_uid(&stream) {
                            eprintln!("anarc-agent: rejected connection with mismatched UID");
                            continue;
                        }
                        let h_tx = handler_tx.clone();
                        std::thread::Builder::new()
                            .name("anarc-agent-conn".into())
                            .spawn(move || {
                                handle_connection(stream, h_tx);
                            })
                            .ok();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(e) => {
                        eprintln!("anarc-agent: accept error: {e}");
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });
    if server.is_err() {
        stop_server();
        return Err("server unavailable. use: status".into());
    }

    Ok(socket_path)
}

pub fn stop_server() {
    RUNNING.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = ACTIVE_SOCKET_PATH.lock() {
        if let Some(path) = guard.take() {
            let _ = remove_file(path);
        }
    }
}

fn handle_connection(
    mut stream: UnixStream,
    handler_tx: async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
) {
    let reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };

    for line in reader.lines() {
        if !RUNNING.load(Ordering::SeqCst) || handler_tx.is_closed() {
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = parse_client_command(trimmed);
        let op = parsed.as_ref().map_or("unknown", AgentRequest::op);
        let response = match parsed {
            Ok(request) => wait_for_tab(request, Duration::from_secs(3), |request| {
                let (resp_tx, resp_rx) = async_channel::bounded(1);
                if handler_tx.try_send((request, resp_tx)).is_err() {
                    return AgentResponse::err("agent busy. use: status");
                }
                resp_rx.recv_blocking().unwrap_or_else(|_| AgentResponse::err("agent disconnected. use: status"))
            }),
            Err(message) => AgentResponse::err(message),
        };

        let reply_bytes = response.result.as_ref().or(response.error.as_ref()).map_or(0, String::len);
        log_call(op, response.ok, reply_bytes);
        let json = match serde_json::to_string(&response) {
            Ok(s) => s,
            Err(_) => r#"{"ok":false,"error":"invalid reply. use: status"}"#.into(),
        };
        if writeln!(stream, "{json}").is_err() {
            break;
        }
    }
}

fn log_call(op: &str, ok: bool, reply_bytes: usize) {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = crate::paths::data_dir().join("agent");
    if create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600)
        .open(dir.join("log.jsonl"))
    {
        let line = serde_json::json!({ "op": op, "ok": ok, "reply_bytes": reply_bytes,
                                      "est_tokens": reply_bytes as f64 / 3.5 });
        let _ = log.write_all(format!("{line}\n").as_bytes());
    }
}

fn wait_for_tab(
    mut request: AgentRequest,
    timeout: Duration,
    mut dispatch: impl FnMut(AgentRequest) -> AgentResponse,
) -> AgentResponse {
    let deadline = Instant::now() + timeout;
    let mut response = dispatch(request.clone());
    while let Some(tab) = response.pending_tab.take() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if let Some(target) = request.target_tab_mut() {
            *target = Some(tab);
        }
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
        response = dispatch(request.clone());
    }
    response
}

pub fn parse_client_command(input: &str) -> Result<AgentRequest, &'static str> {
    if let Ok(req) = serde_json::from_str::<AgentRequest>(input) {
        return Ok(req);
    }

    // Support plain CLI command strings like "see view", "read page text", "find search", "tabs"
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.get(0).copied().unwrap_or("");

    Ok(match cmd {
        "see" => {
            let scope = parts.get(1).map(|s| s.to_string());
            AgentRequest::See { tab_id: None, scope }
        }
        "read" => {
            let target = parts.get(1).map(|s| s.to_string());
            let mode = parts.get(2).map(|s| s.to_string());
            AgentRequest::Read { tab_id: None, target, mode, max: None }
        }
        "find" => {
            let query = parts.get(1..).map(|s| s.join(" ")).unwrap_or_default();
            AgentRequest::Find { tab_id: None, query }
        }
        "wait" => AgentRequest::Do { tab_id: None, ops: input.to_string() },
        "tabs" => AgentRequest::Tabs,
        "pause" => AgentRequest::Pause,
        "resume" => AgentRequest::Resume,
        "status" => AgentRequest::Status,
        _ => return Err("unknown command. use: tabs"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_delayed_browser_and_returns_timeout() {
        let request = AgentRequest::See { tab_id: None, scope: None };
        let pending = || {
            let mut response = AgentResponse::err("tab not ready, retry in 1s");
            response.pending_tab = Some("test-tab".into());
            response
        };
        let mut calls = 0;
        let response = wait_for_tab(request.clone(), Duration::from_secs(3), |request| {
            calls += 1;
            if calls == 1 {
                return pending();
            }
            assert!(matches!(request, AgentRequest::See { tab_id: Some(id), .. } if id == "test-tab"));
            AgentResponse::success("ready")
        });
        assert!(response.ok);
        assert_eq!(calls, 2);

        let response = wait_for_tab(request.clone(), Duration::from_millis(10), |_| pending());
        assert_eq!(response.error.as_deref(), Some("tab not ready, retry in 1s"));

        let mut calls = 0;
        let missing = super::super::plugin::no_such_tab("1* chatgpt.com, 2 about:blank");
        let response = wait_for_tab(request, Duration::from_secs(3), |_| {
            calls += 1;
            AgentResponse::err(&missing)
        });
        assert_eq!(response.error.as_deref(), Some(missing.as_str()));
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_parse_client_command_json_and_plain() {
        assert_eq!(parse_client_command("bogus").err(), Some("unknown command. use: tabs"));
        let json = r#"{"cmd":"see","scope":"all"}"#;
        let req = parse_client_command(json);
        assert!(matches!(req, Ok(AgentRequest::See { scope: Some(scope), .. }) if scope == "all"));

        let plain = "find submit button";
        let req2 = parse_client_command(plain);
        assert!(matches!(req2, Ok(AgentRequest::Find { query, .. }) if query == "submit button"));
    }
}
