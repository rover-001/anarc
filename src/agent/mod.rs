#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

pub mod act;
pub mod observe;
pub mod playbook;
pub mod plugin;
pub mod policy;
pub mod protocol;
pub mod refs;
#[cfg(target_family = "unix")]
pub mod server;
#[cfg(not(target_family = "unix"))]
pub mod server {
    use super::protocol::{AgentRequest, AgentResponse};
    use std::path::PathBuf;

    pub fn is_running() -> bool {
        false
    }
    pub fn start_server(
        _handler_tx: async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
    ) -> Result<PathBuf, String> {
        Ok(PathBuf::from("NUL"))
    }
    pub fn stop_server() {}
}

#[cfg(test)]
mod tests {
    use super::observe::{format_see_output, MAX_SEE_ELEMENTS};
    use super::protocol::{AgentRequest, AgentResponse};
    use super::refs::{RawElement, RawIndex, RefRegistry};
    #[cfg(target_family = "unix")]
    use super::server::{start_server, stop_server};
    #[cfg(target_family = "unix")]
    use std::io::{BufRead, BufReader, Write};
    #[cfg(target_family = "unix")]
    use std::os::unix::net::UnixStream;

    #[test]
    fn test_see_output_token_budget_p50() {
        let mut reg = RefRegistry::new();
        let mut elements = Vec::new();
        for i in 0..30 {
            elements.push(RawElement {
                kind: if i % 3 == 0 { "b".into() } else if i % 3 == 1 { "i".into() } else { "l".into() },
                name: format!("Button Item {i}"),
                flags: if i % 2 == 0 { vec!["primary".into()] } else { vec![] },
                x: 100,
                y: (i as i32) * 30,
                box_bounds: Some([0, (i as i32) * 30, 200, 25]),
                path: format!("div.list>item#{i}"),
                is_banner: false,
                is_dialog: false,
            });
        }

        let raw = RawIndex {
            title: "Shopping Cart Checkout Page".into(),
            url: "https://shop.example.com/checkout".into(),
            scroll_remaining: 150,
            elements,
        };

        let (header, els, _, _) = reg.update_from_raw(raw);
        let output = format_see_output(&header, &els, None);

        // Approximate tokens: 1 token ~= 4 chars or whitespace-delimited words
        let char_count = output.len();
        let estimated_tokens = char_count / 4;

        // Plan requirement: p50 <= 400 tokens
        assert!(
            estimated_tokens <= 400,
            "Estimated tokens ({estimated_tokens}) exceeded p50 limit of 400! Output:\n{output}"
        );
        assert!(els.len() <= MAX_SEE_ELEMENTS);
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn test_agent_server_lifecycle_and_socket_cleanup() -> Result<(), Box<dyn std::error::Error>> {
        let (req_tx, req_rx) = async_channel::bounded(8);
        let test_socket = format!("/tmp/anarc-test-agent-{}.sock", std::process::id());
        unsafe {
            std::env::set_var("ANARC_SOCKET_PATH", &test_socket);
        }

        let bound_path = start_server(req_tx)?;
        assert!(bound_path.exists());
        assert!(super::server::is_running());

        // Spawn mock handler
        std::thread::spawn(move || {
            if let Ok((_req, resp_tx)) = futures_lite::future::block_on(async { req_rx.recv().await }) {
                let _ = resp_tx.try_send(AgentResponse::success("ok from mock handler"));
            }
        });

        // Connect client
        let mut client = UnixStream::connect(&bound_path)?;
        let req = AgentRequest::Tabs;
        let req_json = serde_json::to_string(&req)?;
        writeln!(client, "{req_json}")?;

        let mut reader = BufReader::new(client);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let resp: AgentResponse = serde_json::from_str(line.trim())?;
        assert!(resp.ok);
        assert_eq!(resp.result.as_deref(), Some("ok from mock handler"));

        // Stop server and verify socket is unlinked
        stop_server();
        assert!(!super::server::is_running());
        assert!(!bound_path.exists(), "Socket file must be unlinked on stop");
        Ok(())
    }
}

