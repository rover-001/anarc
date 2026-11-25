use anarc_telemetry::counters::harness_from_name;
use anarc_telemetry::schema::{ActionKind, ErrorCode};

use super::observe::{
    build_read_call_script, build_read_ref_call_script, build_see_call_script, execute_find,
    format_see_output, format_see_output_paged,
};
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

// Telemetry is strictly best-effort: these helpers cannot fail and never
// influence a request's result. Only enums, lengths, counts and durations
// reach the counters.
fn note_call(kind: ActionKind) {
    super::telemetry::telemetry().counters.note_call(kind);
}

fn note_error(code: ErrorCode) {
    super::telemetry::telemetry().counters.note_error(code);
}

/// Classify one DSL operation as an action kind. Only the returned enum is
/// recorded; op arguments (refs, text, URLs) are never inspected here.
fn action_kind_for_op(op: &super::act::ActionOp) -> ActionKind {
    match op {
        super::act::ActionOp::Click { .. }
        | super::act::ActionOp::ForceClick { .. }
        | super::act::ActionOp::DomClick { .. }
        | super::act::ActionOp::DblClick { .. }
        | super::act::ActionOp::SemanticClick { .. }
        | super::act::ActionOp::Hover { .. }
        | super::act::ActionOp::Focus { .. }
        | super::act::ActionOp::Drag { .. }
        | super::act::ActionOp::XY { .. } => ActionKind::Click,
        super::act::ActionOp::ClearAndType { .. }
        | super::act::ActionOp::AppendText { .. }
        | super::act::ActionOp::TypeActive { .. }
        | super::act::ActionOp::TypeSecret { .. }
        | super::act::ActionOp::SemanticType { .. }
        | super::act::ActionOp::SetValue { .. }
        | super::act::ActionOp::Paste { .. }
        | super::act::ActionOp::Key { .. }
        | super::act::ActionOp::KeyDown { .. }
        | super::act::ActionOp::KeyUp { .. }
        | super::act::ActionOp::Submit => ActionKind::Type,
        super::act::ActionOp::Go { .. }
        | super::act::ActionOp::Back
        | super::act::ActionOp::Fwd
        | super::act::ActionOp::Reload
        | super::act::ActionOp::Tab { .. }
        | super::act::ActionOp::Space { .. }
        | super::act::ActionOp::Split { .. } => ActionKind::Navigate,
        super::act::ActionOp::Scroll { .. }
        | super::act::ActionOp::Upload { .. }
        | super::act::ActionOp::Dialog { .. }
        | super::act::ActionOp::Wait { .. } => ActionKind::Other,
    }
}

fn note_dsl_call(op: &super::act::ActionOp) {
    note_call(action_kind_for_op(op));
}

/// Detect the driving harness from explicit env signals. Names are consumed
/// by `harness_from_name`; no string is stored or sent.
fn note_harness_best_effort() {
    let name = std::env::var("ANARC_HARNESS")
        .ok()
        .or_else(|| std::env::var("CLAUDECODE").ok().map(|_| "claude".to_owned()))
        .or_else(|| {
            std::env::var("CODEX_SANDBOX")
                .ok()
                .map(|_| "codex".to_owned())
        })
        .or_else(|| {
            std::env::var("ANTIGRAVITY")
                .ok()
                .map(|_| "antigravity".to_owned())
        });
    if let Some(name) = name {
        let harness = harness_from_name(&name);
        super::telemetry::telemetry().counters.note_harness(harness);
    }
}

/// Count configured plugins from the persisted config; only the count is kept.
fn note_plugins_installed() {
    let count = crate::plugin::PluginConfig::load()
        .map(|config| config.plugins.len() as u64)
        .unwrap_or(0);
    super::telemetry::telemetry()
        .counters
        .set_plugins_installed(count);
}

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
        let (req_tx, req_rx) =
            async_channel::bounded::<(AgentRequest, async_channel::Sender<AgentResponse>)>(32);

        let socket_res = start_server(req_tx);
        let _socket_path = match socket_res {
            Ok(p) => p,
            Err(e) => return Err(PluginError(e)),
        };

        let dispose_rx = req_rx.clone();
        let forwarder = std::thread::Builder::new()
            .name("anarc-agent-forwarder".into())
            .spawn(move || {
                while let Ok((request, resp_tx)) =
                    futures_lite::future::block_on(async { req_rx.recv().await })
                {
                    forward_request_to_engine(request, resp_tx);
                }
            });
        if forwarder.is_err() {
            dispose_rx.close();
            stop_server();
            return Err(PluginError(
                "Agent forwarder unavailable; use: status".into(),
            ));
        }
        note_harness_best_effort();
        note_plugins_installed();
        // Best-effort daily rollup. Consent is unset by default, so this is a
        // no-op until the user opts in; it runs off the plugin thread.
        std::thread::Builder::new()
            .name("anarc-telemetry-flush".into())
            .spawn(|| super::telemetry::flush_daily(env!("CARGO_PKG_VERSION")))
            .ok();
        ctx.effect(move || {
            dispose_rx.close();
            stop_server();
        });

        Ok(())
    }
}

pub(crate) fn handle_agent_request(
    app: &mut crate::app::Anarc,
    mut req: AgentRequest,
    resp_tx: async_channel::Sender<AgentResponse>,
) {
    let agent_state = app.engine.agent_state();
    let is_paused = agent_state.lock().map(|s| s.is_paused).unwrap_or(false);

    if is_paused
        && !matches!(
            req,
            AgentRequest::Resume | AgentRequest::Status | AgentRequest::Doctor | AgentRequest::Tabs
        )
    {
        let _ = resp_tx.try_send(AgentResponse::err("agent paused. use: resume"));
        return;
    }

    let parsed_ops = if let AgentRequest::Do { ops, .. } = &req {
        match super::act::parse_dsl(ops) {
            Ok(ops) => ops,
            Err(e) => {
                note_error(ErrorCode::Other);
                let _ = resp_tx.try_send(AgentResponse::err(e));
                return;
            }
        }
    } else {
        Vec::new()
    };
    for op in &parsed_ops {
        note_dsl_call(op);
    }
    let tab_only = !parsed_ops.is_empty()
        && parsed_ops.iter().all(|op| {
            matches!(
                op,
                super::act::ActionOp::Tab { .. }
                    | super::act::ActionOp::Space { .. }
                    | super::act::ActionOp::Split { .. }
            )
        });
    if !tab_only && let Some(tab_id) = req.target_tab_mut() {
        let raw_target = tab_id
            .clone()
            .unwrap_or_else(|| app.active_tab_id().to_string());
        let target = if let Ok(idx) = raw_target.parse::<usize>() {
            idx.checked_sub(1)
                .and_then(|i| app.active_space().tabs.get(i))
                .map(|t| t.id.clone())
                .unwrap_or(raw_target)
        } else if !app
            .spaces()
            .iter()
            .any(|s| s.tabs.iter().any(|t| t.id == raw_target))
        {
            let lower = raw_target.to_lowercase();
            let matches_tab = |t: &&crate::model::Tab| {
                t.url.contains(&raw_target) || t.title.to_lowercase().contains(&lower)
            };
            app.active_space()
                .tabs
                .iter()
                .find(matches_tab)
                .or_else(|| {
                    app.spaces()
                        .iter()
                        .flat_map(|s| s.tabs.iter())
                        .find(matches_tab)
                })
                .map(|t| t.id.clone())
                .unwrap_or(raw_target)
        } else {
            raw_target
        };
        if !app.engine.has_browser(&target) {
            app.engine.log_agent_browser_miss(&target);
        }
        let Some((space_id, url)) = app.spaces().iter().find_map(|space| {
            space
                .tabs
                .iter()
                .find(|tab| tab.id == target)
                .map(|tab| (space.id.clone(), tab.url.clone()))
        }) else {
            note_error(ErrorCode::NotFound);
            let _ = resp_tx.try_send(AgentResponse::err(no_such_tab(&compact_tabs(app))));
            return;
        };
        if !app.engine.has_browser(&target) {
            if !app.engine.is_initialized() {
                note_error(ErrorCode::Other);
                let _ =
                    resp_tx.try_send(AgentResponse::err("CEF unavailable. use: anarc-ctl doctor"));
                return;
            }
            if !app.engine.agent_browser_pending(&target) {
                let url = if url.is_empty() || url == "lumen://welcome" {
                    "about:blank"
                } else {
                    &url
                };
                if let Err(error) = app
                    .engine
                    .ensure_browser(&space_id, &target, url, 768, 512, 0, 0.0)
                {
                    eprintln!("anarc-agent: wake {target:?} failed: {error}");
                    note_error(ErrorCode::Other);
                    let _ = resp_tx
                        .try_send(AgentResponse::err("tab wake failed. use: anarc-ctl doctor"));
                    return;
                }
            }
            // A still-loading tab is retried by the server; the timeout is
            // recorded there once the retry deadline is actually reached.
            let mut response = AgentResponse::err("tab still loading. use: anarc-ctl doctor");
            response.pending_tab = Some(target);
            let _ = resp_tx.try_send(response);
            return;
        }

        *tab_id = Some(target);
    }

    if let Some(target) = req.target_tab()
        && matches!(
            req,
            AgentRequest::Read { .. }
                | AgentRequest::See { .. }
                | AgentRequest::Find { .. }
                | AgentRequest::Eval { .. }
                | AgentRequest::SetValue { .. }
                | AgentRequest::WaitFor { .. }
        ) {
            let ready_check = "(() => document.readyState)()";
            if let Ok(Ok(state_str)) = app.engine.evaluate(target, ready_check).recv_blocking()
                && state_str == "loading" {
                    for _ in 0..15 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if let Ok(Ok(s)) = app.engine.evaluate(target, ready_check).recv_blocking()
                            && s != "loading" {
                                break;
                            }
                    }
                }
        }

    match req {
        AgentRequest::Doctor => {
            let app = &*app;
            let tabs = app
                .spaces()
                .iter()
                .flat_map(|space| {
                    space
                        .tabs
                        .iter()
                        .map(move |tab| format!("{:?} {}", tab.id, browser_state(app, &tab.id)))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let header = doctor_header(
                true,
                app.engine.is_initialized(),
                app.engine.browser_key_count(),
            );
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
                "anarc-agent active{paused_str} · Workspace: {space_name}"
            )));
        }
        AgentRequest::Tabs => {
            let active_space = app.active_space();
            let active_space_id = active_space.id.clone();
            let active_tab = app.active_tab_id().to_string();
            let mut lines = vec![format!(
                "Active: {} (Workspace: {} ({}) [ACTIVE] · use \"tab new\" to open a tab here)",
                active_tab, active_space.name, active_space_id
            )];
            for tab in &active_space.tabs {
                let active_mark = if active_space.active.as_deref() == Some(&tab.id) {
                    "*"
                } else {
                    " "
                };
                let mut flags = Vec::new();
                if tab.is_pinned() {
                    flags.push("pinned");
                }
                let flag_str = if flags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", flags.join(","))
                };
                lines.push(format!(
                    "  [{active_mark}] {}{} \"{}\" -> {} ({})",
                    tab.id,
                    flag_str,
                    tab.title,
                    tab.url,
                    browser_state(app, &tab.id)
                ));
            }
            let others: Vec<String> = app
                .spaces()
                .iter()
                .filter(|s| s.id != active_space_id)
                .map(|s| format!("{} ({}, {} tabs)", s.name, s.id, s.tabs.len()))
                .collect();
            if !others.is_empty() {
                lines.push(format!(
                    "Other workspaces (switch with do \"space <id|name>\"): {}",
                    others.join(", ")
                ));
            }
            let _ = resp_tx.try_send(AgentResponse::success(lines.join("\n")));
        }
        AgentRequest::See {
            tab_id,
            scope,
            max_name_len,
        } => {
            let target_tab = tab_id.unwrap_or_default();

            let script = super::observe::build_see_call_script_with_len(
                scope.as_deref().unwrap_or("view"),
                max_name_len,
            );
            let is_plus = scope.as_deref() == Some("+");
            let eval_rx = app.engine.evaluate(&target_tab, &script);
            let state_arc = agent_state.clone();
            let tab_id_str = target_tab.clone();
            let tabs = compact_tabs(app);

            std::thread::Builder::new()
                .name("anarc-see-worker".into())
                .spawn(move || {
                    let started = std::time::Instant::now();
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let raw_json = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(_)) => {
                            note_error(ErrorCode::Other);
                            let _ =
                                resp_tx.try_send(AgentResponse::err("page unavailable. use: tabs"));
                            return;
                        }
                        Err(_) => {
                            note_error(ErrorCode::Timeout);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("page disconnected. use: tabs"));
                            return;
                        }
                    };

                    let raw_index: RawIndex = match serde_json::from_str(&raw_json) {
                        Ok(idx) => idx,
                        Err(_) => {
                            note_error(ErrorCode::Other);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("index unavailable. use: tabs"));
                            return;
                        }
                    };

                    let mut guard = match state_arc.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            note_error(ErrorCode::Other);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("state unavailable. use: status"));
                            return;
                        }
                    };

                    let registry = guard.registries.entry(tab_id_str).or_default();
                    let (header, elements, _, _) = registry.update_from_raw(raw_index);
                    let mut formatted = if is_plus {
                        let (text, next, exhausted) = format_see_output_paged(
                            &header,
                            &elements,
                            Some(&tabs),
                            registry.see_plus_offset,
                        );
                        // Page through the full index; wrap after the final page.
                        registry.see_plus_offset = if exhausted { 0 } else { next };
                        text
                    } else {
                        registry.see_plus_offset = 0;
                        format_see_output(&header, &elements, Some(&tabs))
                    };

                    if let Some(reason) = super::policy::check_human_handoff(
                        &header.title,
                        &header.display_url,
                        &formatted,
                    ) {
                        formatted = format!("!human {reason}\n{formatted}");
                    }
                    // Only lengths, an elapsed duration and a count cross into
                    // telemetry; neither the ref snapshot nor the DOM text is
                    // inspected or copied there.
                    super::telemetry::telemetry().counters.note_snapshot(
                        formatted.len() as u64,
                        raw_json.len() as u64,
                        started.elapsed().as_millis() as u64,
                        elements.len() as u64,
                    );
                    let _ = resp_tx.try_send(AgentResponse::success(formatted));
                })
                .ok();
        }
        AgentRequest::Read {
            tab_id,
            target,
            mode,
            max,
        } => {
            let target_tab = tab_id.unwrap_or_default();

            let m = if target.as_deref() == Some("last") {
                "last"
            } else {
                mode.as_deref().unwrap_or("text")
            };
            let max_chars = max.unwrap_or(4000);

            // A non-"last" target is a ref (or `css=` selector). Resolve it
            // against a freshly indexed DOM so reads follow the actual element
            // instead of falling through to article/feed extraction.
            let mut ref_spec: Option<(String, i32, i32)> = None;
            if let Some(raw) = target.as_deref().filter(|t| *t != "last") {
                if let Some(selector) = raw.strip_prefix("css=") {
                    ref_spec = Some((selector.to_string(), 0, 0));
                } else {
                    let see_script = build_see_call_script("all");
                    if let Ok(Ok(raw_json)) =
                        app.engine.evaluate(&target_tab, &see_script).recv_blocking()
                        && let Ok(raw_index) = serde_json::from_str::<RawIndex>(&raw_json)
                        && let Ok(mut guard) = agent_state.lock()
                    {
                        guard
                            .registries
                            .entry(target_tab.clone())
                            .or_default()
                            .update_from_raw(raw_index);
                    }
                    if let Ok(guard) = agent_state.lock()
                        && let Some(registry) = guard.registries.get(&target_tab)
                        && let Some(el) = registry.resolve_ref_with_fallback(raw)
                    {
                        ref_spec = Some((el.dom_path().to_string(), el.center.0, el.center.1));
                    }
                }
            }

            let script = if let Some((path, x, y)) = ref_spec {
                build_read_ref_call_script(&path, m, max_chars, x, y)
            } else {
                build_read_call_script(m, max_chars)
            };
            let eval_rx = app.engine.evaluate(&target_tab, &script);

            std::thread::Builder::new()
                .name("anarc-read-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    let text = match raw_res {
                        Ok(Ok(s)) => s,
                        Ok(Err(_)) => {
                            note_error(ErrorCode::Other);
                            let _ = resp_tx.try_send(AgentResponse::err("read failed. use: see"));
                            return;
                        }
                        Err(_) => {
                            note_error(ErrorCode::Timeout);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("page disconnected. use: tabs"));
                            return;
                        }
                    };
                    let _ = resp_tx.try_send(AgentResponse::success(text));
                })
                .ok();
        }
        AgentRequest::Find { tab_id, query } => {
            let target_tab = tab_id.unwrap_or_default();

            // Always re-index against the LIVE DOM. Typing, async renders and
            // SPA swaps mutate flags/names (e.g. a Send button becoming
            // enabled); searching a stale registry returned old refs/flags.
            // This also hydrates a freshly woken tab with no registry yet.
            let script = build_see_call_script("view");
            if let Ok(Ok(raw_json)) = app.engine.evaluate(&target_tab, &script).recv_blocking()
                && let Ok(raw_index) = serde_json::from_str::<RawIndex>(&raw_json)
                && let Ok(mut guard) = agent_state.lock()
            {
                guard
                    .registries
                    .entry(target_tab.clone())
                    .or_default()
                    .update_from_raw(raw_index);
            }

            let guard = match agent_state.lock() {
                Ok(g) => g,
                Err(_) => {
                    note_error(ErrorCode::Other);
                    let _ = resp_tx.try_send(AgentResponse::err("state unavailable. use: status"));
                    return;
                }
            };

            if let Some(registry) = guard.registries.get(&target_tab) {
                let results = execute_find(registry, &query);
                let _ = resp_tx.try_send(AgentResponse::success(results));
            } else {
                note_error(ErrorCode::NotFound);
                let _ = resp_tx.try_send(AgentResponse::err("index missing. use: see"));
            }
        }
        AgentRequest::Do {
            tab_id,
            delay_ms,
            ..
        } => {
            let target_tab = tab_id.unwrap_or_else(|| app.active_tab_id().to_string());
            super::act::execute_dsl(app, &target_tab, &parsed_ops, delay_ms, resp_tx);
        }
        AgentRequest::Shot {
            tab_id,
            target: _,
            scale,
        } => {
            let target_tab = tab_id.unwrap_or_default();
            let mut params = serde_json::json!({"format": "png"});
            if let Some(s) = scale.filter(|&s| (0.1..1.0).contains(&s))
                && let Ok(Ok(dim_json)) = app
                    .engine
                    .evaluate(&target_tab, "[window.innerWidth, window.innerHeight]")
                    .recv_blocking()
                && let Ok([w, h]) = serde_json::from_str::<[f64; 2]>(&dim_json)
            {
                params["clip"] = serde_json::json!({
                    "x": 0,
                    "y": 0,
                    "width": w.max(1.0),
                    "height": h.max(1.0),
                    "scale": s
                });
            }

            let devtools_rx = app.engine.devtools(
                &target_tab,
                "Page.captureScreenshot",
                Some(&params.to_string()),
            );
            std::thread::Builder::new()
                .name("anarc-shot-worker".into())
                .spawn(move || {
                    let res = futures_lite::future::block_on(async { devtools_rx.recv().await });
                    match res {
                        Ok(Ok(raw_json)) => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw_json)
                                && let Some(data) = parsed.get("data").and_then(|v| v.as_str()) {
                                    let shot_str = format!("data:image/png;base64,{data}");
                                    let _ = resp_tx.try_send(AgentResponse::success(shot_str));
                                    return;
                                }
                            note_error(ErrorCode::Other);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("screenshot invalid. use: see"));
                        }
                        Ok(Err(_)) => {
                            note_error(ErrorCode::Other);
                            let _ =
                                resp_tx.try_send(AgentResponse::err("screenshot failed. use: see"));
                        }
                        Err(_) => {
                            note_error(ErrorCode::Timeout);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("page disconnected. use: tabs"));
                        }
                    }
                })
                .ok();
        }
        AgentRequest::Eval { tab_id, script } => {
            let target_tab = tab_id.unwrap_or_else(|| app.active_tab_id().to_string());
            let eval_rx = app.engine.evaluate(&target_tab, &script);
            std::thread::Builder::new()
                .name("anarc-eval-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    match raw_res {
                        Ok(Ok(s)) => {
                            let _ = resp_tx.try_send(AgentResponse::success(s));
                        }
                        Ok(Err(e)) => {
                            note_error(ErrorCode::Other);
                            let _ = resp_tx.try_send(AgentResponse::err(format!("eval failed: {e}")));
                        }
                        Err(_) => {
                            note_error(ErrorCode::Timeout);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("page disconnected. use: tabs"));
                        }
                    }
                })
                .ok();
        }
        AgentRequest::SetValue {
            tab_id,
            target,
            value,
            mode,
            no_wait,
            wait_ms,
        } => {
            let target_tab = tab_id.unwrap_or_else(|| app.active_tab_id().to_string());
            match super::act::set_element_value(
                app,
                &target_tab,
                target.as_deref(),
                &value,
                mode.as_deref(),
                no_wait.unwrap_or(false),
                wait_ms,
            ) {
                Ok(msg) => {
                    let _ = resp_tx.try_send(AgentResponse::success(msg));
                }
                Err(err) => {
                    note_error(ErrorCode::Other);
                    let _ = resp_tx.try_send(AgentResponse::err(err));
                }
            }
        }
        AgentRequest::WaitFor {
            tab_id,
            condition,
            target,
            timeout_ms,
        } => {
            let target_tab = tab_id.unwrap_or_else(|| app.active_tab_id().to_string());
            let ms = timeout_ms.unwrap_or(30000);
            let target_str = target.unwrap_or_default();
            let script = format!(
                "{} window.__anarc_wait_for({}, {}, {});",
                super::observe::build_indexer_injection_script(),
                serde_json::json!(condition),
                serde_json::json!(target_str),
                ms
            );
            let eval_rx = app.engine.evaluate(&target_tab, &script);
            std::thread::Builder::new()
                .name("anarc-wait-worker".into())
                .spawn(move || {
                    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });
                    match raw_res {
                        Ok(Ok(s)) => {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
                                if val.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                                    let elapsed = val
                                        .get("elapsed_ms")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let url = val.get("url").and_then(|v| v.as_str()).unwrap_or("");
                                    let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("");
                                    let msg = if url.is_empty() {
                                        format!("ok wait_for {condition} ({elapsed}ms)")
                                    } else {
                                        format!("ok wait_for {condition} ({elapsed}ms) · {url} \"{title}\"")
                                    };
                                    let _ = resp_tx.try_send(AgentResponse::success(msg));
                                    return;
                                } else if let Some(err) =
                                    val.get("error").and_then(|v| v.as_str())
                                {
                                    note_error(ErrorCode::Timeout);
                                    let _ = resp_tx.try_send(AgentResponse::err(err.to_string()));
                                    return;
                                }
                            }
                            let _ = resp_tx.try_send(AgentResponse::success(s));
                        }
                        Ok(Err(e)) => {
                            note_error(ErrorCode::Timeout);
                            let _ =
                                resp_tx.try_send(AgentResponse::err(format!("wait_for failed: {e}")));
                        }
                        Err(_) => {
                            note_error(ErrorCode::Timeout);
                            let _ = resp_tx
                                .try_send(AgentResponse::err("page disconnected. use: tabs"));
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
                format!(
                    "Attached to personal workspace '{chosen_space}'. Confirmation required for sensitive operations."
                )
            } else {
                format!("Attached to dedicated workspace '{chosen_space}'.")
            };
            let _ = resp_tx.try_send(AgentResponse::success(notice));
        }
    }
}

fn browser_state(app: &crate::app::Anarc, id: &str) -> &'static str {
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

pub(crate) fn compact_tabs(app: &crate::app::Anarc) -> String {
    let space = app.active_space();
    let active = space.active.as_deref();
    let active_index = space
        .tabs
        .iter()
        .position(|tab| Some(tab.id.as_str()) == active);
    space
        .tabs
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            *index < 7
                || Some(*index) == active_index
                || (*index == 7 && active_index.is_none_or(|active| active < 8))
        })
        .map(|(index, tab)| {
            let star = if Some(tab.id.as_str()) == active {
                "*"
            } else {
                ""
            };
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

pub(crate) fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: AGENT_PLUGIN_ID.to_string(),
        name: "Agent Control".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: PluginKind::Builtin,
        description:
            "Agent control server exposing see, do, read, find, shot tools over local socket"
                .to_string(),
        permissions: Vec::new(),
        enabled_by_default: true,
        can_toggle: true,
    }
}

pub(crate) fn register(host: &mut PluginHost) {
    host.register(descriptor(), || Box::new(AgentPlugin::new()));
}

#[cfg(test)]
mod tests {
    use super::ActionKind;

    #[test]
    fn dsl_ops_map_to_action_kinds_without_a_browser() {
        use super::super::act::{ActionOp, WaitCondition};
        assert_eq!(
            super::action_kind_for_op(&ActionOp::Click {
                ref_id: "b1".into()
            }),
            ActionKind::Click
        );
        assert_eq!(
            super::action_kind_for_op(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: String::new(),
            }),
            ActionKind::Type
        );
        assert_eq!(
            super::action_kind_for_op(&ActionOp::Go {
                url: "https://example.com".into()
            }),
            ActionKind::Navigate
        );
        assert_eq!(
            super::action_kind_for_op(&ActionOp::Wait {
                condition: WaitCondition::Idle
            }),
            ActionKind::Other
        );
    }

    #[test]
    fn missing_tab_includes_selection_hint() {
        assert_eq!(
            super::no_such_tab("1* chatgpt.com, 2 about:blank"),
            "no such tab. tabs: 1* chatgpt.com, 2 about:blank. use: do \"tab 1\""
        );
    }
}
