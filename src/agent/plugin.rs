use super::observe::{build_read_call_script, build_see_call_script, execute_find, format_see_output};
use super::protocol::{AgentRequest, AgentResponse};
use super::refs::{RawIndex, RefRegistry};
use super::server::start_server;
use crate::plugin::{
    Manifest, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind,
};
use std::collections::HashMap;
use std::sync::Mutex;

pub static AGENT_PLUGIN_ID: &str = "anarc.agent";

static ENGINE_REQUEST_CHANNEL: Mutex<
    Option<(
        async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
        async_channel::Sender<()>,
    )>,
> = Mutex::new(None);

pub(crate) fn register_engine_agent_channel(
    req_tx: async_channel::Sender<(AgentRequest, async_channel::Sender<AgentResponse>)>,
    wake_tx: async_channel::Sender<()>,
) {
    if let Ok(mut guard) = ENGINE_REQUEST_CHANNEL.lock() {
        *guard = Some((req_tx, wake_tx));
    }
}

pub(crate) fn forward_request_to_engine(
    req: AgentRequest,
    resp_tx: async_channel::Sender<AgentResponse>,
) {
    if let Ok(guard) = ENGINE_REQUEST_CHANNEL.lock() {
        if let Some((ref r_tx, ref w_tx)) = *guard {
            let _ = r_tx.try_send((req, resp_tx));
            let _ = w_tx.try_send(());
            return;
        }
    }
    let _ = resp_tx.try_send(AgentResponse::err("Engine agent channel not registered"));
}

#[derive(Default)]
pub struct AgentState {
    pub registries: HashMap<String, RefRegistry>,
    pub attached_space: Option<String>,
    pub is_paused: bool,
}

pub struct AgentPlugin {
    manifest: Manifest,
}

impl Default for AgentPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPlugin {
    pub fn new() -> Self {
        Self {
            manifest: Manifest {
                id: AGENT_PLUGIN_ID.to_string(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: vec!["agent".to_string()],
            },
        }
    }
}

impl Plugin for AgentPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let (req_tx, req_rx) = async_channel::bounded::<(AgentRequest, async_channel::Sender<AgentResponse>)>(32);

        let socket_res = start_server(req_tx);
        let _socket_path = match socket_res {
            Ok(p) => p,
            Err(e) => return Err(PluginError(e)),
        };

        std::thread::Builder::new()
            .name("anarc-agent-forwarder".into())
            .spawn(move || {
                while let Ok((request, resp_tx)) = futures_lite::future::block_on(async { req_rx.recv().await }) {
                    forward_request_to_engine(request, resp_tx);
                }
            })
            .ok();

        ctx.effect(move || {
            // Teardown disposer
        });

        Ok(())
    }
}

pub(crate) fn handle_agent_request(
    app: &mut crate::app::Lumen,
    req: AgentRequest,
    resp_tx: async_channel::Sender<AgentResponse>,
) {
    let agent_state = app.engine.agent_state();
    let is_paused = agent_state.lock().map(|s| s.is_paused).unwrap_or(false);

    if is_paused && !matches!(req, AgentRequest::Resume | AgentRequest::Status) {
        let _ = resp_tx.try_send(AgentResponse::err("Agent is paused. Use 'resume' to continue."));
        return;
    }

    match req {
        AgentRequest::Pause => {
            if let Ok(mut state) = agent_state.lock() {
                state.is_paused = true;
            }
            let _ = resp_tx.try_send(AgentResponse::success("Agent paused"));
        }
        AgentRequest::Resume => {
            if let Ok(mut state) = agent_state.lock() {
                state.is_paused = false;
            }
            let _ = resp_tx.try_send(AgentResponse::success("Agent resumed"));
        }
        AgentRequest::Status => {
            let space_name = agent_state
                .lock()
                .ok()
                .and_then(|s| s.attached_space.clone())
                .unwrap_or_else(|| app.active_space().name.clone());
            let paused_str = if is_paused { " (PAUSED)" } else { "" };
            let _ = resp_tx.try_send(AgentResponse::success(format!(
                "anarc-agent active{paused_str} · Space: {space_name}"
            )));
        }
        AgentRequest::Tabs => {
            let mut lines = Vec::new();
            for space in app.spaces() {
                lines.push(format!("Space: {} ({})", space.name, space.id));
                for tab in &space.tabs {
                    let active_mark = if space.active.as_deref() == Some(&tab.id) { "*" } else { " " };
                    lines.push(format!("  [{active_mark}] {} \"{}\" -> {}", tab.id, tab.title, tab.url));
                }
            }
            let _ = resp_tx.try_send(AgentResponse::success(lines.join("\n")));
        }
        AgentRequest::See { tab_id, scope } => {
            let target_tab = match resolve_target_tab_id(app, tab_id.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(e));
                    return;
                }
            };

            let script = build_see_call_script(scope.as_deref().unwrap_or("view"));
            let eval_rx = app.engine.evaluate(&target_tab, &script);
            let state_arc = agent_state.clone();
            let tab_id_str = target_tab.clone();

            std::thread::Builder::new()
                .name("anarc-see-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let raw_json = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(e)) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("See evaluation failed: {e}")));
                            return;
                        }
                        Err(e) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("See channel closed: {e}")));
                            return;
                        }
                    };

                    let raw_index: RawIndex = match serde_json::from_str(&raw_json) {
                        Ok(idx) => idx,
                        Err(e) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("Failed to parse index JSON: {e}")));
                            return;
                        }
                    };

                    let mut guard = match state_arc.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("Agent state poisoned"));
                            return;
                        }
                    };

                    let registry = guard.registries.entry(tab_id_str).or_default();
                    let (header, elements, _, _) = registry.update_from_raw(raw_index);
                    let mut formatted = format_see_output(&header, &elements);

                    if let Some(reason) = super::policy::check_human_handoff(&header.title, &header.display_url, &formatted) {
                        formatted = format!("!human {reason}\n{formatted}");
                    }
                    let _ = resp_tx.try_send(AgentResponse::success(formatted));
                })
                .ok();
        }
        AgentRequest::Read { tab_id, target: _, mode, max } => {
            let target_tab = match resolve_target_tab_id(app, tab_id.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(e));
                    return;
                }
            };

            let m = mode.as_deref().unwrap_or("text");
            let max_chars = max.unwrap_or(4000);
            let script = build_read_call_script(m, max_chars);
            let eval_rx = app.engine.evaluate(&target_tab, &script);

            std::thread::Builder::new()
                .name("anarc-read-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let text = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(e)) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("Read evaluation failed: {e}")));
                            return;
                        }
                        Err(e) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("Read channel closed: {e}")));
                            return;
                        }
                    };
                    let _ = resp_tx.try_send(AgentResponse::success(text));
                })
                .ok();
        }
        AgentRequest::Find { tab_id, query } => {
            let target_tab = match resolve_target_tab_id(app, tab_id.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(e));
                    return;
                }
            };

            let guard = match agent_state.lock() {
                Ok(g) => g,
                Err(_) => {
                    let _ = resp_tx.try_send(AgentResponse::err("Agent state poisoned"));
                    return;
                }
            };

            if let Some(registry) = guard.registries.get(&target_tab) {
                let results = execute_find(registry, &query);
                let _ = resp_tx.try_send(AgentResponse::success(results));
            } else {
                let _ = resp_tx.try_send(AgentResponse::err(format!(
                    "No index available for tab '{target_tab}'. Run 'see' first."
                )));
            }
        }
        AgentRequest::Do { tab_id, ops } => {
            let target_tab = match resolve_target_tab_id(app, tab_id.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(e));
                    return;
                }
            };

            let parsed_ops = match super::act::parse_dsl(&ops) {
                Ok(o) => o,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(format!("DSL parse error: {e}")));
                    return;
                }
            };

            let resp = super::act::execute_dsl(app, &target_tab, &parsed_ops);
            let _ = resp_tx.try_send(resp);
        }
        AgentRequest::Shot { tab_id, target: _, scale: _ } => {
            let target_tab = match resolve_target_tab_id(app, tab_id.as_deref()) {
                Ok(id) => id,
                Err(e) => {
                    let _ = resp_tx.try_send(AgentResponse::err(e));
                    return;
                }
            };

            let devtools_rx = app.engine.devtools(&target_tab, "Page.captureScreenshot", Some("{\"format\":\"png\"}"));
            std::thread::Builder::new()
                .name("anarc-shot-worker".into())
                .spawn(move || {
                    let res = futures_lite::future::block_on(async { devtools_rx.recv().await });
                    match res {
                        Ok(Ok(raw_json)) => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw_json) {
                                if let Some(data) = parsed.get("data").and_then(|v| v.as_str()) {
                                    let shot_str = format!("data:image/png;base64,{data}");
                                    let _ = resp_tx.try_send(AgentResponse::success(shot_str));
                                    return;
                                }
                            }
                            let _ = resp_tx.try_send(AgentResponse::err("Screenshot capture returned invalid payload"));
                        }
                        Ok(Err(e)) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("Screenshot DevTools error: {e}")));
                        }
                        Err(e) => {
                            let _ = resp_tx.try_send(AgentResponse::err(format!("Screenshot channel error: {e}")));
                        }
                    }
                })
                .ok();
        }
        AgentRequest::Attach { space_id } => {
            let chosen_space = space_id.clone().unwrap_or_else(|| "Agent".into());
            let is_personal = chosen_space != "Agent";
            if let Ok(mut state) = agent_state.lock() {
                state.attached_space = Some(chosen_space.clone());
            }
            let notice = if is_personal {
                format!("Attached to personal space '{chosen_space}'. Confirmation required for sensitive operations.")
            } else {
                format!("Attached to dedicated space '{chosen_space}'.")
            };
            let _ = resp_tx.try_send(AgentResponse::success(notice));
        }
    }
}

fn resolve_target_tab_id(
    app: &crate::app::Lumen,
    explicit_tab: Option<&str>,
) -> Result<String, String> {
    if let Some(t) = explicit_tab {
        return Ok(t.to_string());
    }
    let act = app.active_tab_id();
    if !act.is_empty() {
        Ok(act.to_string())
    } else {
        Err("No active browser tab found".to_string())
    }
}

pub(crate) fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: AGENT_PLUGIN_ID.to_string(),
        name: "Agent Control".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: PluginKind::Builtin,
        description: "Agent control server exposing see, do, read, find, shot tools over local socket".to_string(),
        permissions: Vec::new(),
        enabled_by_default: true,
        can_toggle: true,
    }
}

pub(crate) fn register(host: &mut PluginHost) {
    host.register(descriptor(), || Box::new(AgentPlugin::new()));
}
