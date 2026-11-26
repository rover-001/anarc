//! DSL batch execution loop, snapshot diffing, and single-op dispatcher.

mod browser_ops;
mod page_ops;
mod upload;

use anarc_telemetry::schema::ErrorCode;

use super::super::policy::{PolicyConfig, resolve_secret};
use super::super::protocol::AgentResponse;
use super::dom::{
    check_occlusion, click_at, dblclick_at, dom_click, press_key, ref_name,
    resolve_semantic_coords, resolve_target_coords, submit, type_at, focus_field,
};
use super::note_error;
use super::set_element_value;
use super::types::{ActionOp, DblClickTarget, ScrollDirection, WaitCondition};
use crate::agent::observe::{build_see_call_script, build_stable_call_script, format_see_output};
use crate::agent::refs::RawIndex;

pub use upload::take_pending_upload_files;

#[derive(Clone, Copy, Debug)]
enum WaitMode {
    Stable(u64),
    Stream { quiescent_ms: u64, timeout_ms: u64 },
}

pub fn execute_dsl(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    ops: &[ActionOp],
    delay_ms: Option<u64>,
    response: async_channel::Sender<AgentResponse>,
) {
    let agent_state = app.engine.agent_state();
    let policy_config = PolicyConfig::load();
    let mut submissions = Vec::new();
    let (actions, wait_mode) = match ops.split_last() {
        Some((
            ActionOp::Wait {
                condition: WaitCondition::Stable(ms),
            },
            actions,
        )) => (actions, Some(WaitMode::Stable(*ms))),
        Some((
            ActionOp::Wait {
                condition:
                    WaitCondition::Stream {
                        quiescent_ms,
                        timeout_ms,
                    },
            },
            actions,
        )) => (
            actions,
            Some(WaitMode::Stream {
                quiescent_ms: *quiescent_ms,
                timeout_ms: *timeout_ms,
            }),
        ),
        _ => (ops, None),
    };
    if actions.iter().any(|op| {
        matches!(
            op,
            ActionOp::Wait {
                condition: WaitCondition::Stable(_) | WaitCondition::Stream { .. }
            }
        )
    }) {
        let _ = response.try_send(AgentResponse::err(
            "stable or stream wait must be last. use: do \"wait stream\"",
        ));
        return;
    }
    let mut target = tab_id.to_owned();
    for (index, op) in actions.iter().enumerate() {
        if let Err(error) = execute_single_op(
            app,
            &target,
            op,
            &agent_state,
            &policy_config,
            &mut submissions,
        ) {
            let error = if error.starts_with("needs_confirm:") {
                error
            } else {
                format!("fail op{}: {error}", index + 1)
            };
            let _ = response.try_send(AgentResponse::err(error));
            return;
        }
        if matches!(
            op,
            ActionOp::Tab { .. } | ActionOp::Space { .. } | ActionOp::Split { .. }
        ) {
            target = app.active_tab_id().to_owned();
        }
        if let Some(ms) = delay_ms.filter(|&ms| ms > 0)
            && index + 1 < actions.len()
        {
            std::thread::sleep(std::time::Duration::from_millis(ms.min(5000)));
        }
    }
    let count = ops.len();
    let summary = if submissions.is_empty() {
        format!("ok {count}")
    } else {
        format!("ok {count} · {}", submissions.join(" · "))
    };
    let tabs = super::super::plugin::compact_tabs(app);
    let script = match wait_mode {
        Some(WaitMode::Stable(ms)) => build_stable_call_script(ms),
        Some(WaitMode::Stream {
            quiescent_ms,
            timeout_ms,
        }) => crate::agent::observe::build_stream_call_script(quiescent_ms, timeout_ms),
        None => build_see_call_script("view"),
    };
    // DevTools awaits the page promise on its own threads. GPUI can render and process other calls.
    let started = std::time::Instant::now();
    let evaluation = app.engine.evaluate(&target, &script);
    let _ = std::thread::Builder::new()
        .name("anarc-do-worker".into())
        .spawn(move || {
            let result = evaluation.recv_blocking().ok().and_then(Result::ok);
            if let Some(mode) = wait_mode {
                let max_timeout = match mode {
                    WaitMode::Stable(_) => std::time::Duration::from_secs(30),
                    WaitMode::Stream { timeout_ms, .. } => {
                        std::time::Duration::from_millis(timeout_ms)
                    }
                };
                if result.as_deref() == Some("streaming")
                    || (result.is_none() && started.elapsed() >= max_timeout)
                {
                    let _ = response.try_send(AgentResponse::success("streaming"));
                    return;
                }
                if result.is_none() {
                    note_error(ErrorCode::Timeout);
                    let _ = response.try_send(AgentResponse::err("wait interrupted. use: see"));
                    return;
                }
            }
            // Character length only; the raw JSON string itself never enters
            // telemetry.
            let raw_dom_chars = result.as_ref().map_or(0, |json| json.len() as u64);
            let raw = result.and_then(|json| serde_json::from_str::<RawIndex>(&json).ok());
            let reply = if let (Some(raw), Ok(mut state)) = (raw, agent_state.lock()) {
                let registry = state.registries.entry(target).or_default();
                let (header, elements, diff, gen_changed) = registry.update_from_raw(raw);
                let mut reply = format!("{summary} · g{}", header.generation);
                if gen_changed {
                    let snapshot = format_see_output(&header, &elements, Some(&tabs));
                    super::super::telemetry::telemetry().counters.note_snapshot(
                        snapshot.len() as u64,
                        raw_dom_chars,
                        started.elapsed().as_millis() as u64,
                        elements.len() as u64,
                    );
                    reply.push('\n');
                    reply.push_str(&snapshot);
                } else if !diff.is_empty() {
                    reply.push('\n');
                    reply.push_str(&compact_diff(&diff));
                }
                reply
            } else {
                summary
            };
            let _ = response.try_send(AgentResponse::success(reply));
        });
}

fn execute_single_op(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    op: &ActionOp,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
    policy_config: &PolicyConfig,
    submissions: &mut Vec<&'static str>,
) -> Result<(), String> {
    match op {
        ActionOp::Click { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, ref_id, state_lock)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            crate::agent::playbook::record_action(ref_id);
            Ok(())
        }
        ActionOp::ForceClick { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            crate::agent::playbook::record_action(&format!("click! {ref_id}"));
            Ok(())
        }
        ActionOp::TypeActive { text } => {
            app.engine.set_focus(tab_id, true);
            app.engine
                .input_type(tab_id, text)
                .recv_blocking()
                .ok()
                .and_then(Result::ok)
                .ok_or("typing failed. use: see")?;
            crate::agent::playbook::record_action(&format!("type \"{text}\""));
            Ok(())
        }
        ActionOp::DomClick { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, ref_id, state_lock)?;
            dom_click(app, tab_id, cx, cy, policy_config)?;
            crate::agent::playbook::record_action(&format!("clickjs {ref_id}"));
            Ok(())
        }
        ActionOp::DblClick { target } => {
            let (cx, cy, label) = match target {
                DblClickTarget::Coords(x, y) => (*x, *y, format!("xy {x} {y}")),
                DblClickTarget::Target(ref_id) => {
                    let (x, y) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
                    check_occlusion(app, tab_id, x, y, ref_id, state_lock)?;
                    (x, y, ref_id.clone())
                }
            };
            dblclick_at(app, tab_id, cx, cy, policy_config)?;
            crate::agent::playbook::record_action(&format!("dblclick {label}"));
            Ok(())
        }
        ActionOp::SetValue {
            target,
            value,
            enter,
        } => {
            set_element_value(app, tab_id, Some(target), value, Some("set"), false, None)?;
            if *enter {
                press_key(app, tab_id, "Enter", &gpui_kit::Modifiers::default());
            }
            Ok(())
        }
        ActionOp::ClearAndType { ref_id, text } | ActionOp::AppendText { ref_id, text } => {
            let replace = matches!(op, ActionOp::ClearAndType { .. });
            page_ops::type_into_ref(app, tab_id, ref_id, text, replace, state_lock)?;
            let operator = if replace { "=" } else { "+" };
            crate::agent::playbook::record_action(&format!("{ref_id}{operator}\"{text}\""));
            Ok(())
        }
        ActionOp::TypeSecret {
            ref_id,
            secret_name,
        } => {
            let secret = resolve_secret(secret_name)
                .filter(|secret| !secret.is_empty())
                .ok_or("secret missing. use: see")?;
            page_ops::type_into_ref(app, tab_id, ref_id, &secret, true, state_lock)?;
            crate::agent::playbook::record_action(&format!("{ref_id}=@secret:{secret_name}"));
            Ok(())
        }
        ActionOp::SemanticClick { kind, label } => {
            let (cx, cy, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, &id, state_lock)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            crate::agent::playbook::record_action(&format!("{kind}:\"{label}\""));
            Ok(())
        }
        ActionOp::SemanticType { kind, label, text } => {
            let (x, y, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, x, y, &id, state_lock)?;
            let name = ref_name(tab_id, &id, state_lock);
            let (x, y) = focus_field(app, tab_id, x, y, &name).unwrap_or((x, y));
            type_at(app, tab_id, x, y, text, true, &name)?;
            crate::agent::playbook::record_action(&format!("{kind}:\"{label}\"=\"{text}\""));
            Ok(())
        }
        ActionOp::Key { chord } => page_ops::execute_key_press(app, tab_id, chord, policy_config),
        ActionOp::KeyDown { chord } => page_ops::execute_key_edge(app, tab_id, chord, true),
        ActionOp::KeyUp { chord } => page_ops::execute_key_edge(app, tab_id, chord, false),
        ActionOp::Submit => {
            submissions.push(submit(app, tab_id, policy_config)?);
            Ok(())
        }
        ActionOp::Go { url } => browser_ops::execute_go(app, tab_id, url, policy_config),
        ActionOp::Back => {
            app.engine.execute_script(tab_id, "history.back();");
            Ok(())
        }
        ActionOp::Fwd => {
            app.engine.execute_script(tab_id, "history.forward();");
            Ok(())
        }
        ActionOp::Reload => {
            app.engine.reload(tab_id, false);
            Ok(())
        }
        ActionOp::Scroll { direction } => match direction {
            ScrollDirection::Down => {
                app.engine.input_scroll(tab_id, 200, 200, 0.0, -300.0);
                Ok(())
            }
            ScrollDirection::Up => {
                app.engine.input_scroll(tab_id, 200, 200, 0.0, 300.0);
                Ok(())
            }
            ScrollDirection::Top => {
                app.engine.execute_script(tab_id, "window.scrollTo(0, 0);");
                Ok(())
            }
            ScrollDirection::End => {
                app.engine
                    .execute_script(tab_id, "window.scrollTo(0, document.body.scrollHeight);");
                Ok(())
            }
            ScrollDirection::Ref(target_ref) => {
                let (_cx, cy) = resolve_target_coords(app, target_ref, tab_id, state_lock)?;
                app.engine.execute_script(
                    tab_id,
                    &format!("window.scrollTo(0, {});", cy.saturating_sub(100)),
                );
                Ok(())
            }
        },
        ActionOp::Hover { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            app.engine.input_move(tab_id, cx, cy);
            Ok(())
        }
        ActionOp::Focus { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            let focus_script = format!(
                "const el = document.elementFromPoint({cx}, {cy}); \
                 if (el) {{ \
                     el.focus(); \
                     const ce = el.closest ? (el.closest('[contenteditable=\"true\"], rich-textarea, [role=\"textbox\"]') || el) : el; \
                     if (ce && ce.focus) ce.focus(); \
                 }}"
            );
            app.engine.execute_script(tab_id, &focus_script);
            Ok(())
        }
        ActionOp::Drag { from, to } => {
            page_ops::execute_drag(app, tab_id, from, to, state_lock, policy_config)
        }
        ActionOp::Paste { ref_id, text } => {
            page_ops::execute_paste(app, tab_id, ref_id.as_deref(), text.as_deref(), state_lock, policy_config)
        }
        ActionOp::XY { x, y } => {
            check_occlusion(app, tab_id, *x, *y, "", state_lock)?;
            click_at(app, tab_id, *x, *y, policy_config)
        }
        ActionOp::Upload { ref_id, file_paths } => {
            upload::execute_upload(app, tab_id, ref_id, file_paths, state_lock, policy_config)
        }
        ActionOp::Dialog { accept } => {
            let script = if *accept {
                "if (window.__dialog_callback) { window.__dialog_callback(true); }"
            } else {
                "if (window.__dialog_callback) { window.__dialog_callback(false); }"
            };
            app.engine.execute_script(tab_id, script);
            Ok(())
        }
        ActionOp::Tab { action } => browser_ops::execute_tab_action(app, action),
        ActionOp::Space { target } => browser_ops::execute_space_action(app, target),
        ActionOp::Split { action } => browser_ops::execute_split_action(app, action),
        ActionOp::Wait { condition } => browser_ops::execute_wait_action(app, tab_id, condition),
    }
}

pub(in crate::agent::act) fn compact_diff(diff: &crate::agent::refs::SnapshotDiff) -> String {
    const MAX_ITEMS: usize = 24;
    let formatted = diff.format_diff();
    let mut parts = formatted.split("  ");
    let shown: Vec<&str> = parts.by_ref().take(MAX_ITEMS).collect();
    let hidden = parts.count();
    let mut out = shown.join("  ");
    if hidden > 0 {
        out.push_str(&format!("  …+{hidden} more"));
    }
    out
}
