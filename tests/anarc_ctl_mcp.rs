#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Mcp {
    home: PathBuf,
    socket: PathBuf,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Mcp {
    fn start(explicit_socket: bool) -> Self {
        Self::start_with(explicit_socket, false)
    }

    fn start_with(explicit_socket: bool, legacy_socket: bool) -> Self {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let home =
            std::env::temp_dir().join(format!("anarc-mcp-{}-{sequence}", std::process::id()));
        let agent_dir = home.join(".local/share/anarc/agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let filename = if legacy_socket {
            "ctl-8.sock"
        } else {
            "ctl.sock"
        };
        let socket = agent_dir.join(filename);
        let mut command = Command::new(env!("CARGO_BIN_EXE_anarc-ctl"));
        command.arg("mcp").env_clear();
        if explicit_socket {
            command.arg("--socket").arg(&socket);
        }
        if legacy_socket {
            let runtime = home.join("runtime");
            fs::create_dir(&runtime).unwrap();
            drop(UnixListener::bind(runtime.join("anarc-agent.sock")).unwrap());
            command.env("XDG_RUNTIME_DIR", runtime);
        }
        let mut child = command
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            home,
            socket,
            child,
            input,
            output,
        }
    }

    fn call(&mut self, id: u64) {
        let request =
            json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":"tabs"}});
        writeln!(self.input, "{request}").unwrap();
    }

    fn reply(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn bind_stub(&self) -> thread::JoinHandle<()> {
        let listener = UnixListener::bind(&self.socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("stub accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["cmd"], "tabs");
            writeln!(stream, "{}", json!({"ok":true,"result":"stub tabs"})).unwrap();
        })
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn assert_stub_reply(reply: Value, id: u64) {
    assert_eq!(reply["id"], id);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert_eq!(reply["result"]["content"][0]["text"], "stub tabs");
}

#[test]
fn scrubbed_mcp_finds_home_fallback() {
    let mut mcp = Mcp::start(false);
    let stub = mcp.bind_stub();
    mcp.call(1);
    assert_stub_reply(mcp.reply(), 1);
    stub.join().unwrap();
}

#[test]
fn scrubbed_mcp_reconnects_after_failure_and_retries_during_call() {
    let mut mcp = Mcp::start(true);
    mcp.call(1);
    let reply = mcp.reply();
    assert_eq!(reply["result"]["isError"], true);
    assert!(
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("anarc not running: start it")
    );

    let mut logs = BufReader::new(mcp.child.stderr.take().unwrap());
    let mut line = String::new();
    for attempt in 1..=3 {
        line.clear();
        logs.read_line(&mut line).unwrap();
        assert!(line.contains(&format!("connect {attempt}/3")), "{line}");
    }

    mcp.call(2);
    line.clear();
    logs.read_line(&mut line).unwrap();
    assert!(
        line.contains("connect 1/3") && line.contains("Err"),
        "{line}"
    );
    let stub = mcp.bind_stub();
    assert_stub_reply(mcp.reply(), 2);
    stub.join().unwrap();
}

#[test]
fn named_instance_takes_precedence_over_refused_legacy_socket() {
    let mut mcp = Mcp::start_with(false, true);
    let stub = mcp.bind_stub();
    mcp.call(1);
    assert_stub_reply(mcp.reply(), 1);
    stub.join().unwrap();
}

#[test]
fn refused_socket_requires_a_dead_recorded_owner_before_removal() {
    use std::os::unix::fs::MetadataExt;

    let mcp = Mcp::start(true);
    drop(UnixListener::bind(&mcp.socket).unwrap());
    let metadata = fs::symlink_metadata(&mcp.socket).unwrap();
    let owner_file = mcp.socket.with_extension("sock.owner");
    let record = |pid| {
        fs::write(
            &owner_file,
            format!("{pid} {} {}\n", metadata.dev(), metadata.ino()),
        )
        .unwrap();
    };
    let status = || {
        let output = Command::new(env!("CARGO_BIN_EXE_anarc-ctl"))
            .arg("--socket")
            .arg(&mcp.socket)
            .arg("status")
            .output()
            .unwrap();
        assert!(!output.status.success());
        String::from_utf8(output.stderr).unwrap()
    };

    assert!(status().contains("connection refused"));
    record(std::process::id());
    assert!(status().contains("connection refused"));
    assert!(mcp.socket.exists());

    let mut former_owner = Command::new(env!("CARGO_BIN_EXE_anarc-ctl"))
        .arg("--help")
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    record(former_owner.id());
    assert!(former_owner.wait().unwrap().success());
    assert!(status().contains("stale socket removed, retry"));
    assert!(!mcp.socket.exists());
}
