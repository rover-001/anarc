use std::io::{BufRead, BufReader, Write};
#[cfg(target_family = "unix")]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

#[cfg(unix)]
#[allow(dead_code)]
#[path = "../paths.rs"]
mod paths;

#[derive(Default)]
struct SocketTarget {
    explicit: Option<PathBuf>,
    instance: Option<u32>,
}

impl SocketTarget {
    #[cfg(unix)]
    fn resolve(&self) -> paths::agent_socket::Discovery {
        paths::agent_socket::resolve_socket(self.explicit.as_deref(), self.instance)
    }

    fn path(&self) -> PathBuf {
        #[cfg(unix)]
        { self.resolve().resolved }
        #[cfg(not(unix))]
        { self.explicit.clone().unwrap_or_else(|| PathBuf::from("anarc-agent.sock")) }
    }
}

fn print_usage() {
    eprintln!(
        "Usage: anarc-ctl <command> [options]\n\
         \n\
         Commands:\n\
           see [view|all|main|<ref>|+]  Compact index of interactable elements\n\
           read [<ref>|page|last]            Read page content (modes: text, table, links, outline)\n\
           find <query>                 Find elements matching query (<= 5 refs)\n\
           tabs                         List open tabs and spaces\n\
           doctor                       Diagnose agent control failures\n\
           do <ops>                     Execute action DSL (click, type, key, wait)\n\
           shot [ref|view]              Save a PNG and print its path\n\
           wait stable [ms]             Wait for the latest text to stop growing\n\
           stats                        Show agent call and token statistics\n\
           status                       Show agent server status\n\
           pause                        Pause agent actions\n\
           resume                       Resume agent actions\n\
           mcp                          Run as stdio MCP server for agent harnesses\n\
           install                      Install/print MCP configuration for harnesses\n\
         \n\
         Options:\n\
           --tab <id>                   Target tab ID (defaults to active tab)\n\
           --instance <slot>            Target Anarc instance (1–9)\n\
           --socket <path>              Override agent socket path\n\
           --mode <mode>                Read mode (text, table, links, outline)\n\
           --max <n>                    Max output characters/budget\n\
           --harness <harness>          For install: claude | codex | antigravity | print\n\
           --shot-as path               Return a PNG path from MCP shot\n\
           --help                       Show this help message"
    );
}

#[cfg(unix)]
fn connect_socket(target: &SocketTarget, mcp: bool, doctor: bool) -> Result<(UnixStream, PathBuf), String> {
    use std::io::ErrorKind;
    use std::time::Duration;

    let tries = if mcp { 3 } else { 1 };
    for attempt in 1..=tries {
        let discovery = target.resolve();
        let path = discovery.resolved;
        let result = UnixStream::connect(&path);
        if mcp {
            eprintln!("[anarc-mcp] connect {attempt}/{tries} path={path:?} instance={:?} env={:?} result={:?}",
                target.instance, discovery.environment, result.as_ref().map(|_| ()));
        }
        let error = match result {
            Ok(stream) => return Ok((stream, path)),
            Err(error) => error,
        };
        let kind = error.kind();
        if matches!(kind, ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::TimedOut | ErrorKind::WouldBlock)
            && attempt < tries
        {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }

        let message = match kind {
            ErrorKind::NotFound => "anarc not running: start it. use: anarc-ctl doctor",
            ErrorKind::ConnectionRefused => {
                use paths::agent_socket::{lock_owner, remove_dead_socket};
                if lock_owner(&path, false).is_ok_and(|owner| remove_dead_socket(&path, &owner)) {
                    "stale socket removed, retry. use: start Anarc and run anarc-ctl doctor"
                } else {
                    "connection refused. use: check the Anarc instance and run anarc-ctl doctor"
                }
            }
            ErrorKind::PermissionDenied => "connection rejected: socket permission denied. use: run Anarc and the harness as the same user",
            ErrorKind::TimedOut | ErrorKind::WouldBlock => "connection timed out. use: anarc-ctl doctor",
            _ => "connection failed. use: anarc-ctl doctor",
        };
        return Err(if doctor { format!("socket: {path:?}\n{message}") } else { message.into() });
    }
    Err("connection failed. use: anarc-ctl doctor".into())
}

#[cfg(target_family = "unix")]
fn send_socket_request(
    target: &SocketTarget,
    request_val: &serde_json::Value,
    mcp: bool,
) -> Result<String, String> {
    use std::io::ErrorKind;
    use std::time::Duration;

    let doctor = request_val.get("cmd").and_then(|cmd| cmd.as_str()) == Some("doctor");
    let (mut stream, socket_path) = connect_socket(target, mcp, doctor)?;
    let timeout = Some(Duration::from_secs(28));
    stream.set_read_timeout(timeout).and_then(|()| stream.set_write_timeout(timeout))
        .map_err(|_| "socket timeout configuration failed. use: anarc-ctl doctor")?;
    let req_str = serde_json::to_string(request_val).map_err(|_| "invalid request. use: anarc-ctl doctor")?;
    let write_result = writeln!(stream, "{req_str}");

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    let bytes = reader.read_line(&mut resp_line).map_err(|error| match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "connection timed out waiting for Anarc. use: anarc-ctl doctor",
        _ => "reply failed. use: anarc-ctl doctor",
    })?;
    if bytes == 0 {
        return Err(if write_result.is_err() {
            "request failed. use: anarc-ctl doctor"
        } else {
            "connection closed before reply. use: anarc-ctl doctor"
        }.into());
    }

    let parsed: serde_json::Value = match serde_json::from_str(resp_line.trim()) {
        Ok(v) => v,
        Err(_) => return Err("invalid reply. use: status".into()),
    };

    let is_ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if is_ok {
        let res = parsed
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if doctor {
            Ok(format!("socket: {socket_path:?}\n{res}"))
        } else {
            Ok(res)
        }
    } else {
        let err = parsed
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("agent error. use: tabs")
            .to_string();
        Err(err)
    }
}

#[cfg(not(target_family = "unix"))]
fn send_socket_request(
    _target: &SocketTarget,
    _request_val: &serde_json::Value,
    _mcp: bool,
) -> Result<String, String> {
    Err("Unix required. use: anarc-ctl --help".to_string())
}

// ---------------------------------------------------------------------------
// MCP Server Implementation (hand-rolled JSON-RPC over stdio, zero extra deps)
// ---------------------------------------------------------------------------

fn get_tool_definitions() -> serde_json::Value {
    const RECOVERY: &str = "On any error run `doctor` first.";
    serde_json::json!([
        {
            "name": "see",
            "description": format!("Compact interactive elements index (inputs, buttons, links). {RECOVERY}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "description": "Scope: view (default), all, main, <ref>, or + (next page)",
                        "enum": ["view", "all", "main", "+"]
                    },
                    "tab_id": {
                        "type": "string"
                    }
                }
            }
        },
        {
            "name": "do",
            "description": format!("Browser actions: click, type, key, submit, navigate, scroll, wait stable. {RECOVERY}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ops": {
                        "type": "string",
                        "description": "DSL ops (e.g. b1; i2=\"text\"; key Enter; scroll down)"
                    },
                    "tab_id": {
                        "type": "string"
                    }
                },
                "required": ["ops"]
            }
        },
        {
            "name": "read",
            "description": format!("Read page content. Target last reads the latest message. {RECOVERY}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string"
                    },
                    "mode": {
                        "type": "string",
                        "description": "Mode: text, table, links, or outline",
                        "enum": ["text", "table", "links", "outline"]
                    },
                    "max": {
                        "type": "integer"
                    },
                    "tab_id": {
                        "type": "string"
                    }
                }
            }
        },
        {
            "name": "find",
            "description": format!("Find up to 5 elements matching text or keyword query. {RECOVERY}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "q": {
                        "type": "string"
                    },
                    "tab_id": {
                        "type": "string"
                    }
                },
                "required": ["q"]
            }
        },
        {
            "name": "tabs",
            "description": format!("List tabs and spaces. {RECOVERY}"),
            "inputSchema": { "type": "object" }
        },
        {
            "name": "doctor",
            "description": "Diagnose agent control failures. Run first on any error.",
            "inputSchema": { "type": "object" }
        },
        {
            "name": "shot",
            "description": format!("Capture cropped element or viewport screenshot. {RECOVERY}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string"
                    },
                    "scale": {
                        "type": "number"
                    },
                    "tab_id": {
                        "type": "string"
                    }
                }
            }
        }
    ])
}

fn handle_mcp_request(
    req: &serde_json::Value,
    socket: &SocketTarget,
    shot_as_path: bool,
) -> Option<serde_json::Value> {
    let method = req.get("method").and_then(|m| m.as_str())?;
    let id = req.get("id");

    // Notifications (no id) require no response
    if id.is_none() {
        if method == "notifications/initialized" {
            eprintln!("[anarc-mcp] Client initialized notification received");
        }
        return None;
    }
    let req_id = id.cloned().unwrap_or(serde_json::Value::Null);

    match method {
        "initialize" => {
            let requested_proto = req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");

            // Negotiate protocol version
            let negotiated_version = match requested_proto {
                "2024-11-05" | "2024-10-07" | "2024-09-24" => requested_proto,
                _ => "2024-11-05",
            };

            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": negotiated_version,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "anarc",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }))
        }
        "ping" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {}
        })),
        "tools/list" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "tools": get_tool_definitions()
            }
        })),
        "tools/call" => {
            let params = match req.get("params") {
                Some(p) => p,
                None => {
                    return Some(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": { "code": -32602, "message": "missing params. use: tools/list" }
                    }));
                }
            };

            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

            let tab_id = arguments.get("tab_id").and_then(|v| v.as_str()).map(|s| s.to_string());

            let socket_req = match tool_name {
                "tabs" | "doctor" => serde_json::json!({"cmd": tool_name}),
                "see" => serde_json::json!({
                    "cmd": "see",
                    "tab_id": tab_id,
                    "scope": arguments.get("scope").and_then(|v| v.as_str())
                }),
                "do" => {
                    let ops = arguments.get("ops").and_then(|v| v.as_str()).unwrap_or("");
                    serde_json::json!({
                        "cmd": "do",
                        "tab_id": tab_id,
                        "ops": ops
                    })
                }
                "read" => serde_json::json!({
                    "cmd": "read",
                    "tab_id": tab_id,
                    "target": arguments.get("target").and_then(|v| v.as_str()),
                    "mode": arguments.get("mode").and_then(|v| v.as_str()),
                    "max": arguments.get("max").and_then(|v| v.as_u64()).map(|u| u as usize)
                }),
                "find" => {
                    let q = arguments.get("q").and_then(|v| v.as_str()).unwrap_or("");
                    serde_json::json!({
                        "cmd": "find",
                        "tab_id": tab_id,
                        "query": q
                    })
                }
                "shot" => serde_json::json!({
                    "cmd": "shot",
                    "tab_id": tab_id,
                    "target": arguments.get("target").and_then(|v| v.as_str()),
                    "scale": arguments.get("scale").and_then(|v| v.as_f64()).map(|f| f as f32)
                }),
                _ => {
                    return Some(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": { "code": -32601, "message": "unknown tool. use: tools/list" }
                    }));
                }
            };

            let result = send_socket_request(socket, &socket_req, true);
            let content = if tool_name == "shot" {
                result.and_then(|reply| {
                    if shot_as_path {
                        save_screenshot(&reply).map(|text| serde_json::json!({"type": "text", "text": text}))
                    } else {
                        let image = screenshot_data(&reply)?;
                        Ok(serde_json::json!({"type": "image", "data": image, "mimeType": "image/png"}))
                    }
                })
            } else {
                result.map(|text| {
                    let end = text.floor_char_boundary(16000);
                    let text = if text.len() > end {
                        format!("{}... [truncated]", text.get(..end).unwrap_or_default())
                    } else {
                        text
                    };
                    serde_json::json!({"type": "text", "text": text})
                })
            };

            let (block, is_error) = match content {
                Ok(block) => (block, false),
                Err(error) => (serde_json::json!({"type": "text", "text": error}), true),
            };
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": { "content": [block], "isError": is_error }
            }))
        }
        _ => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "error": { "code": -32601, "message": "unknown method. use: tools/list" }
        })),
    }
}

fn run_mcp_server(socket: SocketTarget, shot_as_path: bool) {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed_req: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                let err_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": "invalid JSON. use: tools/list" }
                });
                let _ = writeln!(stdout_lock, "{}", serde_json::to_string(&err_resp).unwrap_or_default());
                let _ = stdout_lock.flush();
                continue;
            }
        };

        if let Some(resp) = handle_mcp_request(&parsed_req, &socket, shot_as_path) {
            let resp_str = serde_json::to_string(&resp).unwrap_or_default();
            let _ = writeln!(stdout_lock, "{resp_str}");
            let _ = stdout_lock.flush();
        }
    }
}

fn handle_install(harness: Option<&str>, socket: &SocketTarget) {
    let path = socket.path();
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map_or(path.clone(), |dir| dir.join(path))
    };
    let Some(path) = path.to_str() else {
        eprintln!("socket path is not UTF-8. use: --socket with a UTF-8 path");
        std::process::exit(1);
    };
    let json = serde_json::json!({
        "mcpServers": {
            "anarc": {
                "command": "anarc-ctl",
                "args": ["mcp"],
                "env": {"ANARC_CTL_SOCKET": path}
            }
        }
    });
    let codex = format!(
        "[mcp_servers.anarc]\ncommand = \"anarc-ctl\"\nargs = [\"mcp\"]\n\n[mcp_servers.anarc.env]\nANARC_CTL_SOCKET = {}",
        toml::Value::String(path.into())
    );
    let json = serde_json::to_string_pretty(&json).unwrap_or_default();
    match harness.unwrap_or("print") {
        "claude" => println!("# Claude Code\nAdd to .mcp.json\n\n{json}"),
        "antigravity" => println!("# Antigravity\nAdd to ~/.gemini/config/mcp_config.json or .agents/mcp_config.json\n\n{json}"),
        "codex" => println!("# Codex CLI\nAdd to ~/.codex/config.toml\n\n{codex}"),
        _ => println!("# Claude Code and Antigravity (JSON)\n\n{json}\n\n# Codex CLI (TOML)\n\n{codex}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }

    let mut socket = SocketTarget::default();
    let mut explicit_tab: Option<String> = None;
    let mut explicit_mode: Option<String> = None;
    let mut explicit_max: Option<usize> = None;
    let mut explicit_harness: Option<String> = None;
    let mut shot_as_path = false;

    // Extract global flags
    let mut filtered_args = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                if i + 1 < args.len() {
                    socket.explicit = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--instance" => {
                socket.instance = args.get(i + 1).and_then(|value| value.parse().ok())
                    .filter(|slot| (1..=9).contains(slot));
                if socket.instance.is_none() {
                    eprintln!("invalid instance. use: --instance <1–9>");
                    std::process::exit(1);
                }
                i += 2;
            }
            "--tab" => {
                if i + 1 < args.len() {
                    explicit_tab = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--mode" => {
                if i + 1 < args.len() {
                    explicit_mode = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--max" => {
                if i + 1 < args.len() {
                    explicit_max = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--shot-as" => {
                if args.get(i + 1).is_none_or(|mode| mode != "path") {
                    eprintln!("invalid screenshot mode. use: anarc-ctl mcp --shot-as path");
                    std::process::exit(1);
                }
                shot_as_path = true;
                i += 2;
            }
            "--harness" => {
                if i + 1 < args.len() {
                    explicit_harness = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                filtered_args.push(args[i].clone());
                i += 1;
            }
        }
    }

    if filtered_args.is_empty() {
        print_usage();
        return;
    }

    let cmd = &filtered_args[0];

    if cmd == "mcp" {
        run_mcp_server(socket, shot_as_path);
        return;
    }

    if cmd == "install" {
        handle_install(explicit_harness.as_deref(), &socket);
        return;
    }

    if cmd == "stats" {
        match read_stats() {
            Ok(stats) => println!("{stats}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Build JSON request for Anarc agent server
    let request_val = match cmd.as_str() {
        "see" => {
            let scope = filtered_args.get(1).cloned();
            serde_json::json!({
                "cmd": "see",
                "tab_id": explicit_tab,
                "scope": scope
            })
        }
        "read" => {
            let target = filtered_args.get(1).cloned();
            serde_json::json!({
                "cmd": "read",
                "tab_id": explicit_tab,
                "target": target,
                "mode": explicit_mode,
                "max": explicit_max
            })
        }
        "find" => {
            let q = if filtered_args.len() > 1 {
                filtered_args[1..].join(" ")
            } else {
                String::new()
            };
            serde_json::json!({
                "cmd": "find",
                "tab_id": explicit_tab,
                "query": q
            })
        }
        "tabs" | "doctor" => {
            serde_json::json!({
                "cmd": cmd
            })
        }
        "do" => {
            let ops = if filtered_args.len() > 1 {
                filtered_args[1..].join(" ")
            } else {
                String::new()
            };
            serde_json::json!({
                "cmd": "do",
                "tab_id": explicit_tab,
                "ops": ops
            })
        }
        "wait" => serde_json::json!({
            "cmd": "do",
            "tab_id": explicit_tab,
            "ops": filtered_args.join(" "),
        }),
        "shot" => {
            let target = filtered_args.get(1).cloned();
            serde_json::json!({
                "cmd": "shot",
                "tab_id": explicit_tab,
                "target": target
            })
        }
        "status" => {
            serde_json::json!({
                "cmd": "status"
            })
        }
        "pause" => {
            serde_json::json!({
                "cmd": "pause"
            })
        }
        "resume" => {
            serde_json::json!({
                "cmd": "resume"
            })
        }
        _ => {
            eprintln!("unknown command. use: anarc-ctl --help");
            std::process::exit(1);
        }
    };

    let result = send_socket_request(&socket, &request_val, false);
    let result = if cmd == "shot" {
        result.and_then(|reply| save_screenshot(&reply))
    } else {
        result
    };
    match result {
        Ok(res) => {
            if !res.is_empty() {
                println!("{res}");
            }
        }
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}

fn screenshot_data(reply: &str) -> Result<&str, String> {
    reply.strip_prefix("data:image/png;base64,")
        .filter(|data| data.starts_with("iVBORw0KGgo"))
        .ok_or_else(|| "screenshot invalid. use: see".into())
}

fn decode_screenshot(data: &str) -> Option<Vec<u8>> {
    let bytes = data.trim_end_matches('=').as_bytes();
    if data.len() % 4 != 0 || data.len() - bytes.len() > 2 || bytes.len() % 4 == 1 {
        return None;
    }
    let mut png = Vec::with_capacity(bytes.len().saturating_mul(3) / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for &byte in bytes {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            png.push((accumulator >> bits) as u8);
        }
    }
    Some(png)
}

fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if !png.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR") {
        return None;
    }
    let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    (width != 0 && height != 0).then_some((width, height))
}

fn save_screenshot(reply: &str) -> Result<String, String> {
    let invalid = "screenshot invalid. use: see";
    let png = decode_screenshot(screenshot_data(reply)?).ok_or(invalid)?;
    let (width, height) = png_dimensions(&png).ok_or(invalid)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos()).unwrap_or_default();
    let path = std::env::temp_dir().join(format!("anarc-{}-{now}.png", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_family = "unix")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&path).and_then(|mut file| file.write_all(&png))
        .map_err(|_| "PNG write failed. use: see")?;
    Ok(format!("saved {} {width}x{height} (image file: do not read_file/cat it)", path.display()))
}

fn read_stats() -> Result<String, String> {
    let dir = std::env::var_os("ANARC_DATA_DIR").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| PathBuf::from(home).join(".local/share/anarc")))
        .unwrap_or_else(|| std::env::temp_dir().join(".local/share/anarc"));
    let log = match std::fs::File::open(dir.join("agent/log.jsonl")) {
        Ok(log) => log,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("no calls recorded".into()),
        Err(_) => return Err("stats unavailable. use: status".into()),
    };
    let mut groups = std::collections::BTreeMap::<String, (Vec<f64>, usize)>::new();
    for line in BufReader::new(log).lines() {
        let line = line.map_err(|_| "stats unreadable. use: status")?;
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let (Some(op), Some(ok), Some(tokens)) = (
            record.get("op").and_then(|value| value.as_str()),
            record.get("ok").and_then(|value| value.as_bool()),
            record.get("est_tokens").and_then(|value| value.as_f64()),
        ) else { continue };
        let (samples, errors) = groups.entry(op.to_string()).or_default();
        samples.push(tokens);
        *errors += usize::from(!ok);
    }
    if groups.is_empty() {
        return Ok("no calls recorded".into());
    }
    let mut lines = vec!["op count p50_est_tokens p95_est_tokens error_rate".to_string()];
    for (op, (mut samples, errors)) in groups {
        samples.sort_by(f64::total_cmp);
        let count = samples.len();
        let quantile = |percent: usize| {
            samples.get((count * percent).div_ceil(100).saturating_sub(1)).copied().unwrap_or(0.0)
        };
        let error_rate = 100.0 * errors as f64 / count as f64;
        lines.push(format!("{op} {count} {:.1} {:.1} {error_rate:.1}%", quantile(50), quantile(95)));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_path_contains_only_dimensions_and_file() -> Result<(), String> {
        let reply = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRhkAAAAASUVORK5CYII=";
        let line = save_screenshot(reply)?;
        assert!(!line.contains("base64") && !line.contains("iVBOR"));
        assert!(line.ends_with("1x1 (image file: do not read_file/cat it)"));
        let path = line.strip_prefix("saved ").and_then(|line| line.split_once(" 1x1"))
            .map(|(path, _)| path).ok_or("missing test PNG path")?;
        let png = std::fs::read(path).map_err(|error| error.to_string())?;
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
        assert_eq!(png_dimensions(&png), Some((1, 1)));
        Ok(())
    }

    #[test]
    fn test_tools_schema_token_cost_under_700() {
        let tools = get_tool_definitions();
        let json_str = serde_json::to_string(&tools).unwrap();

        // Check total token cost using standard approximation:
        // 1 token ~= 4 characters, or whitespace/punctuation count
        let char_len = json_str.len();
        let token_estimate_by_char = char_len / 4;
        let token_estimate_by_words = (json_str.split(|c: char| c.is_whitespace() || c == '"' || c == ',' || c == ':').filter(|s| !s.is_empty()).count() as f64 * 1.1) as usize;

        let max_estimate = token_estimate_by_char.max(token_estimate_by_words);
        eprintln!(
            "Total tool schema JSON length: {char_len} chars, estimated tokens: {max_estimate} (char-based: {token_estimate_by_char}, word-based: {token_estimate_by_words})"
        );

        // Target: Total tool-schema cost across all 5 tools <= 700 tokens
        assert!(
            max_estimate <= 700,
            "Total tool schema token estimate ({max_estimate}) exceeded target limit of 700 tokens!"
        );

        // Also assert each individual tool description is <= 60 tokens (~240 chars)
        let tool_array = tools.as_array().unwrap();
        assert_eq!(tool_array.len(), 7, "Expected five core tools, tabs, and doctor");
        for tool in tool_array {
            let desc = tool.get("description").and_then(|d| d.as_str()).unwrap();
            let desc_tokens = desc.len() / 4;
            assert!(
                desc_tokens <= 60,
                "Tool '{}' description too long ({} tokens): {}",
                tool.get("name").and_then(|n| n.as_str()).unwrap(),
                desc_tokens,
                desc
            );
        }
    }

    #[test]
    fn mcp_protocol_errors_give_next_commands() {
        let socket_path = SocketTarget { explicit: Some("/tmp/nonexistent-anarc-test.sock".into()), instance: None };
        let requests = [
            serde_json::json!({"id": 1, "method": "unknown"}),
            serde_json::json!({"id": 2, "method": "tools/call"}),
            serde_json::json!({"id": 3, "method": "tools/call", "params": {"name": "unknown"}}),
        ];
        for request in requests {
            let reply = handle_mcp_request(&request, &socket_path, false);
            let error = reply.as_ref().and_then(|reply| reply.pointer("/error/message"))
                .and_then(serde_json::Value::as_str).unwrap_or_default();
            assert!(error.ends_with("use: tools/list"), "{error}");
        }
    }

    #[test]
    fn test_mcp_mock_lifecycle_and_protocol_negotiation() {
        let socket_path = SocketTarget { explicit: Some("/tmp/nonexistent-anarc-test.sock".into()), instance: None };

        // 1. Initialize
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "1.0" }
            }
        });
        let init_resp = handle_mcp_request(&init_req, &socket_path, false).unwrap();
        assert_eq!(init_resp.get("id").unwrap().as_i64(), Some(1));
        let res = init_resp.get("result").unwrap();
        assert_eq!(res.get("protocolVersion").unwrap().as_str(), Some("2024-11-05"));
        assert!(res.get("capabilities").unwrap().get("tools").is_some());

        // 2. Initialized notification
        let notif_req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let notif_resp = handle_mcp_request(&notif_req, &socket_path, false);
        assert!(notif_resp.is_none(), "Notifications must not receive a response");

        // 3. Ping
        let ping_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        });
        let ping_resp = handle_mcp_request(&ping_req, &socket_path, false).unwrap();
        assert_eq!(ping_resp.get("id").unwrap().as_i64(), Some(2));

        // 4. Tools list
        let list_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/list"
        });
        let list_resp = handle_mcp_request(&list_req, &socket_path, false).unwrap();
        let tools = list_resp.get("result").unwrap().get("tools").unwrap().as_array().unwrap();
        assert_eq!(tools.len(), 7);

        // 5. Tools call with Anarc offline - should return clean error message without hanging
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "see",
                "arguments": { "scope": "view" }
            }
        });
        let call_resp = handle_mcp_request(&call_req, &socket_path, false).unwrap();
        let call_res = call_resp.get("result").unwrap();
        assert_eq!(call_res.get("isError").unwrap().as_bool(), Some(true));
        let content = call_res.get("content").unwrap().as_array().unwrap();
        let text = content[0].get("text").unwrap().as_str().unwrap();
        assert!(text.contains("anarc not running"));
    }
}
