use anarc_telemetry::counters::harness_from_name;
use anarc_telemetry::schema::{ActionKind, ErrorCode};

use super::super::act::ActionOp;

pub(super) fn note_call(kind: ActionKind) {
    crate::agent::telemetry::telemetry().counters.note_call(kind);
}

pub(super) fn note_error(code: ErrorCode) {
    crate::agent::telemetry::telemetry().counters.note_error(code);
}

pub(super) fn action_kind_for_op(op: &ActionOp) -> ActionKind {
    match op {
        ActionOp::Click { .. }
        | ActionOp::ForceClick { .. }
        | ActionOp::DomClick { .. }
        | ActionOp::DblClick { .. }
        | ActionOp::SemanticClick { .. }
        | ActionOp::Hover { .. }
        | ActionOp::Focus { .. }
        | ActionOp::Drag { .. }
        | ActionOp::XY { .. } => ActionKind::Click,
        ActionOp::ClearAndType { .. }
        | ActionOp::AppendText { .. }
        | ActionOp::TypeActive { .. }
        | ActionOp::TypeSecret { .. }
        | ActionOp::SemanticType { .. }
        | ActionOp::SetValue { .. }
        | ActionOp::Paste { .. }
        | ActionOp::Key { .. }
        | ActionOp::KeyDown { .. }
        | ActionOp::KeyUp { .. }
        | ActionOp::Submit => ActionKind::Type,
        ActionOp::Go { .. }
        | ActionOp::Back
        | ActionOp::Fwd
        | ActionOp::Reload
        | ActionOp::Tab { .. }
        | ActionOp::Space { .. }
        | ActionOp::Split { .. } => ActionKind::Navigate,
        ActionOp::Scroll { .. }
        | ActionOp::Upload { .. }
        | ActionOp::Dialog { .. }
        | ActionOp::Wait { .. } => ActionKind::Other,
    }
}


pub(super) fn note_dsl_call(op: &ActionOp) {
    note_call(action_kind_for_op(op));
}

pub(super) fn note_harness_best_effort() {
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
        crate::agent::telemetry::telemetry().counters.note_harness(harness);
    }
}

/// Count configured plugins from the persisted config; only the count is kept.
pub(super) fn note_plugins_installed() {
    let count = crate::plugin::PluginConfig::load()
        .map(|config| config.plugins.len() as u64)
        .unwrap_or(0);
    crate::agent::telemetry::telemetry()
        .counters
        .set_plugins_installed(count);
}
