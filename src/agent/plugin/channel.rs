use anarc_telemetry::schema::ErrorCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::protocol::{AgentRequest, AgentResponse};
use super::telemetry::note_error;

const WINDOW_NOT_READY: &str =
    "anarc window not ready (no frame painted). use: focus the Anarc window, then retry";
static WINDOW_PAINTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn note_window_painted() {
    WINDOW_PAINTED.store(true, Ordering::Release);
}

type EngineAgentChannel = (
    async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
    async_channel::Sender<()>,
);

static ENGINE_REQUEST_CHANNEL: Mutex<Option<EngineAgentChannel>> = Mutex::new(None);

pub(crate) fn register_engine_agent_channel(
    req_tx: async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
    wake_tx: async_channel::Sender<()>,
) {
    WINDOW_PAINTED.store(false, Ordering::Release);
    if let Ok(mut guard) = ENGINE_REQUEST_CHANNEL.lock() {
        *guard = Some((req_tx, wake_tx));
    }
}

/// Wait briefly for the GUI to register its request channel.
///
/// The agent socket starts during plugin mount, before `Engine::new()` runs. A
/// request arriving in that window would otherwise be rejected as "engine
/// unavailable", which forces callers into a `status`/`doctor` handshake.
fn wait_for_engine_channel(timeout: std::time::Duration) -> Option<EngineAgentChannel> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(guard) = ENGINE_REQUEST_CHANNEL.lock()
            && let Some(channel) = guard.as_ref()
        {
            return Some(channel.clone());
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub(crate) fn forward_request_to_engine(
    req: AgentRequest,
    resp_tx: async_channel::Sender<AgentResponse>,
) {
    let requires_paint = matches!(
        req,
        AgentRequest::See { .. }
            | AgentRequest::Do { .. }
            | AgentRequest::Read { .. }
            | AgentRequest::Find { .. }
            | AgentRequest::Shot { .. }
            | AgentRequest::Eval { .. }
            | AgentRequest::SetValue { .. }
            | AgentRequest::WaitFor { .. }
    );

    if requires_paint && !WINDOW_PAINTED.load(Ordering::Acquire) {
        if let Ok(guard) = ENGINE_REQUEST_CHANNEL.lock()
            && let Some((_, ref w_tx)) = *guard {
                let _ = w_tx.try_send(());
            }
        for _ in 0..60 {
            if WINDOW_PAINTED.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if !WINDOW_PAINTED.load(Ordering::Acquire) {
            note_error(ErrorCode::Timeout);
            let _ = resp_tx.try_send(AgentResponse::err(WINDOW_NOT_READY));
            return;
        }
    }

    let Some((req_tx, wake_tx)) = wait_for_engine_channel(std::time::Duration::from_secs(2)) else {
        note_error(ErrorCode::Other);
        let _ = resp_tx.try_send(AgentResponse::err("engine unavailable. use: status"));
        return;
    };
    if let Err(error) = req_tx.try_send((req, resp_tx)) {
        note_error(ErrorCode::Other);
        let (_, reply) = error.into_inner();
        let _ = reply.try_send(AgentResponse::err("agent busy. use: status"));
    } else {
        let _ = wake_tx.try_send(());
    }
}
