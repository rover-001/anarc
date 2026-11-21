use super::observe::{build_read_call_script, build_see_call_script, execute_find, format_see_output};
use super::protocol::{AgentRequest, AgentResponse};
use super::refs::{RawIndex, RefRegistry};
use super::server::{start_server, stop_server};
use crate::plugin::{
    Manifest, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind,
};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub static AGENT_PLUGIN_ID: &str = "anarc.agent";

const WINDOW_NOT_READY: &str =
    "anarc window not ready (no frame painted). use: focus the Anarc window, then retry";
static WINDOW_PAINTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn note_window_painted() {
    WINDOW_PAINTED.store(true, Ordering::Release);
}

fn doctor_header(painted: bool, initialized: bool, browsers: usize) -> String {
    let yes_no = |value| if value { "yes" } else { "no" };
    format!(
        "server version: {}\ninstance/profile: {:?}\nwindow painted: {}\nCEF initialized: {}\nbrowser-map keys: {browsers}",
        env!("CARGO_PKG_VERSION"),
        crate::paths::cef_root_dir(),
        yes_no(painted),
        yes_no(initialized),
    )
}

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
    WINDOW_PAINTED.store(false, Ordering::Release);
    if let Ok(mut guard) = ENGINE_REQUEST_CHANNEL.lock() {
        *guard = Some((req_tx, wake_tx));
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
    );

    if requires_paint && !WINDOW_PAINTED.load(Ordering::Acquire) {
        if let Ok(guard) = ENGINE_REQUEST_CHANNEL.lock() {
            if let Some((_, ref w_tx)) = *guard {
                let _ = w_tx.try_send(());
            }
        }
        for _ in 0..60 {
            if WINDOW_PAINTED.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if !WINDOW_PAINTED.load(Ordering::Acquire) {
            let _ = resp_tx.try_send(AgentResponse::err(WINDOW_NOT_READY));
            return;
        }
    }

    if let Ok(guard) = ENGINE_REQUEST_CHANNEL.lock() {
        if let Some((ref r_tx, ref w_tx)) = *guard {
            if let Err(error) = r_tx.try_send((req, resp_tx)) {
                let (_, reply) = error.into_inner();
                let _ = reply.try_send(AgentResponse::err("agent busy. use: status"));
            } else {
                let _ = w_tx.try_send(());
            }
            return;
        }
    }
    let _ = resp_tx.try_send(AgentResponse::err("engine unavailable. use: status"));
}

#[derive(Default)]
pub struct AgentState {
    pub registries: HashMap<String, RefRegistry>,
    pub attached_space: Option<String>,
    pub is_paused: bool,
    pub confirmations: super::policy::Confirmations,
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

        let dispose_rx = req_rx.clone();
        let forwarder = std::thread::Builder::new()
            .name("anarc-agent-forwarder".into())
            .spawn(move || {
                while let Ok((request, resp_tx)) = futures_lite::future::block_on(async { req_rx.recv().await }) {
                    forward_request_to_engine(request, resp_tx);
                }
            });
        if forwarder.is_err() {
            dispose_rx.close();
            stop_server();
            return Err(PluginError("Agent forwarder unavailable; use: status".into()));
        }
        ctx.effect(move || {
            dispose_rx.close();
            stop_server();
        });

        Ok(())
    }
}

pub(crate) fn handle_agent_request(
    app: &mut crate::app::Lumen,
    mut req: AgentRequest,
    resp_tx: async_channel::Sender<AgentResponse>,
) {
    let agent_state = app.engine.agent_state();
    let is_paused = agent_state.lock().map(|s| s.is_paused).unwrap_or(false);

    if is_paused && !matches!(req, AgentRequest::Resume | AgentRequest::Status | AgentRequest::Doctor | AgentRequest::Tabs) {
        let _ = resp_tx.try_send(AgentResponse::err("agent paused. use: resume"));
        return;
    }

    let parsed_ops = if let AgentRequest::Do { ops, .. } = &req {
        match super::act::parse_dsl(ops) {
            Ok(ops) => ops,
            Err(e) => {
                let _ = resp_tx.try_send(AgentResponse::err(e));
                return;
            }
        }
    } else {
        Vec::new()
    };
    let tab_only = !parsed_ops.is_empty()
        && parsed_ops.iter().all(|op| matches!(op, super::act::ActionOp::Tab { .. }));
    if !tab_only && let Some(tab_id) = req.target_tab_mut() {
        let raw_target = tab_id.clone().unwrap_or_else(|| app.active_tab_id().to_string());
        let target = if let Ok(idx) = raw_target.parse::<usize>() {
            idx.checked_sub(1)
                .and_then(|i| app.active_space().tabs.get(i))
                .map(|t| t.id.clone())
                .unwrap_or(raw_target)
        } else if !app.spaces().iter().any(|s| s.tabs.iter().any(|t| t.id == raw_target)) {
            app.spaces().iter().find_map(|s| {
                s.tabs.iter().find(|t| {
                    t.url.contains(&raw_target) || t.title.to_lowercase().contains(&raw_target.to_lowercase())
                }).map(|t| t.id.clone())
            }).unwrap_or(raw_target)
        } else {
            raw_target
        };
        if !app.engine.has_browser(&target) {
            app.engine.log_agent_browser_miss(&target);
        }
        let Some((space_id, url)) = app.spaces().iter().find_map(|space| {
            space.tabs.iter().find(|tab| tab.id == target)
                .map(|tab| (space.id.clone(), tab.url.clone()))
        }) else {
            let _ = resp_tx.try_send(AgentResponse::err(no_such_tab(&compact_tabs(app))));
            return;
        };
        if !app.engine.has_browser(&target) {
            if !app.engine.is_initialized() {
                let _ = resp_tx.try_send(AgentResponse::err("CEF unavailable. use: anarc-ctl doctor"));
                return;
            }
            if !app.engine.agent_browser_pending(&target) {
                let url = if url.is_empty() || url == "lumen://welcome" { "about:blank" } else { &url };
                if let Err(error) = app.engine.ensure_browser(&space_id, &target, url, 768, 512, 0, 0.0) {
                    eprintln!("anarc-agent: wake {target:?} failed: {error}");
                    let _ = resp_tx.try_send(AgentResponse::err("tab wake failed. use: anarc-ctl doctor"));
                    return;
                }
            }
            let mut response = AgentResponse::err("tab still loading. use: anarc-ctl doctor");
            response.pending_tab = Some(target);
            let _ = resp_tx.try_send(response);
            return;
        }
        *tab_id = Some(target);
    }

    match req {
        AgentRequest::Doctor => {
            let app = &*app;
            let tabs = app.spaces().iter().flat_map(|space| {
                space.tabs.iter().map(move |tab| format!("{:?} {}", tab.id, browser_state(app, &tab.id)))
            }).collect::<Vec<_>>().join(", ");
            let header = doctor_header(true, app.engine.is_initialized(), app.engine.browser_key_count());
            let _ = resp_tx.try_send(AgentResponse::success(format!("{header}\ntabs: {tabs}")));
        }
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
                    let mut flags = Vec::new();
                    if tab.pinned {
                        flags.push("pinned");
                    }
                    if tab.favorite {
                        flags.push("fav");
                    }
                    let flag_str = if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(",")) };
                    lines.push(format!("  [{active_mark}] {}{} \"{}\" -> {} ({})", tab.id, flag_str, tab.title, tab.url, browser_state(app, &tab.id)));
                }
            }
            let _ = resp_tx.try_send(AgentResponse::success(lines.join("\n")));
        }
        AgentRequest::See { tab_id, scope } => {
            let target_tab = tab_id.unwrap_or_default();

            let script = build_see_call_script(scope.as_deref().unwrap_or("view"));
            let eval_rx = app.engine.evaluate(&target_tab, &script);
            let state_arc = agent_state.clone();
            let tab_id_str = target_tab.clone();
            let tabs = compact_tabs(app);

            std::thread::Builder::new()
                .name("anarc-see-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let raw_json = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(_)) => {
                            let _ = resp_tx.try_send(AgentResponse::err("page unavailable. use: tabs"));
                            return;
                        }
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("page disconnected. use: tabs"));
                            return;
                        }
                    };

                    let raw_index: RawIndex = match serde_json::from_str(&raw_json) {
                        Ok(idx) => idx,
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("index unavailable. use: tabs"));
                            return;
                        }
                    };

                    let mut guard = match state_arc.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("state unavailable. use: status"));
                            return;
                        }
                    };

                    let registry = guard.registries.entry(tab_id_str).or_default();
                    let (header, elements, _, _) = registry.update_from_raw(raw_index);
                    let mut formatted = format_see_output(&header, &elements, Some(&tabs));

                    if let Some(reason) = super::policy::check_human_handoff(&header.title, &header.display_url, &formatted) {
                        formatted = format!("!human {reason}\n{formatted}");
                    }
                    let _ = resp_tx.try_send(AgentResponse::success(formatted));
                })
                .ok();
        }
        AgentRequest::Read { tab_id, target, mode, max } => {
            let target_tab = tab_id.unwrap_or_default();

            let m = if target.as_deref() == Some("last") { "last" } else { mode.as_deref().unwrap_or("text") };
            let max_chars = max.unwrap_or(4000);
            let script = build_read_call_script(m, max_chars);
            let eval_rx = app.engine.evaluate(&target_tab, &script);

            std::thread::Builder::new()
                .name("anarc-read-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let text = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(_)) => {
                            let _ = resp_tx.try_send(AgentResponse::err("read failed. use: see"));
                            return;
                        }
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("page disconnected. use: tabs"));
                            return;
                        }
                    };
                    let _ = resp_tx.try_send(AgentResponse::success(text));
                })
                .ok();
        }
        AgentRequest::Find { tab_id, query } => {
            let target_tab = tab_id.unwrap_or_default();

            let guard = match agent_state.lock() {
                Ok(g) => g,
                Err(_) => {
                    let _ = resp_tx.try_send(AgentResponse::err("state unavailable. use: status"));
                    return;
                }
            };

            if let Some(registry) = guard.registries.get(&target_tab) {
                let results = execute_find(registry, &query);
                let _ = resp_tx.try_send(AgentResponse::success(results));
            } else {
                let _ = resp_tx.try_send(AgentResponse::err("index missing. use: see"));
            }
        }
        AgentRequest::Do { tab_id, .. } => {
            let target_tab = tab_id.unwrap_or_else(|| app.active_tab_id().to_string());
            super::act::execute_dsl(app, &target_tab, &parsed_ops, resp_tx);
        }
        AgentRequest::Shot { tab_id, target: _, scale: _ } => {
            let target_tab = tab_id.unwrap_or_default();

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
                            let _ = resp_tx.try_send(AgentResponse::err("screenshot invalid. use: see"));
                        }
                        Ok(Err(_)) => {
                            let _ = resp_tx.try_send(AgentResponse::err("screenshot failed. use: see"));
                        }
                        Err(_) => {
                            let _ = resp_tx.try_send(AgentResponse::err("page disconnected. use: tabs"));
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

fn browser_state(app: &crate::app::Lumen, id: &str) -> &'static str {
    if app.engine.has_browser(id) {
        "live"
    } else if app.engine.agent_browser_pending(id) {
        "loading"
    } else {
        "asleep"
    }
}

pub(crate) fn no_such_tab(tabs: &str) -> String {
    if tabs.is_empty() {
        return "no such tab. tabs: none. use: do \"tab new\"".into();
    }
    format!("no such tab. tabs: {tabs}. use: do \"tab 1\"")
}

pub(crate) fn compact_tabs(app: &crate::app::Lumen) -> String {
    let space = app.active_space();
    let active = space.active.as_deref();
    let active_index = space.tabs.iter().position(|tab| Some(tab.id.as_str()) == active);
    space.tabs.iter().enumerate()
        .filter(|(index, _)| *index < 7 || Some(*index) == active_index
            || (*index == 7 && active_index.is_none_or(|active| active < 8)))
        .map(|(index, tab)| {
            let star = if Some(tab.id.as_str()) == active { "*" } else { "" };
            let host = crate::shield::url_host(&tab.url).unwrap_or_else(|| {
                if tab.url.is_empty() || tab.url == "about:blank" {
                    "about:blank".into()
                } else {
                    "local".into()
                }
            });
            format!("{}{star} {host}", index + 1)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_tab_includes_selection_hint() {
        assert_eq!(
            super::no_such_tab("1* chatgpt.com, 2 about:blank"),
            "no such tab. tabs: 1* chatgpt.com, 2 about:blank. use: do \"tab 1\""
        );
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
