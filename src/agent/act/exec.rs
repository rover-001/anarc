use anarc_telemetry::schema::ErrorCode;

use super::super::policy::{
    NavigationCheck, PolicyConfig, check_navigation_policy, resolve_secret,
};
use super::super::protocol::AgentResponse;
use super::dom::{
    check_action_policy, check_click_policy, check_occlusion, click_at, dblclick_at, dom_click,
    evaluate_action, focus_field, parse_chord_modifiers, press_key, ref_name,
    resolve_semantic_coords, resolve_tab_target, resolve_target_coords, submit, type_at,
};
use super::note_error;
use super::set_element_value;
use super::types::{
    ActionOp, DblClickTarget, DragTarget, ScrollDirection, SplitAction, TabAction, WaitCondition,
};
use crate::agent::observe::{build_see_call_script, build_stable_call_script, format_see_output};
use crate::agent::refs::RawIndex;

static PENDING_UPLOADS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

pub fn set_pending_upload_files(files: Vec<String>) {
    if let Ok(mut guard) = PENDING_UPLOADS.lock() {
        *guard = Some(files);
    }
}

pub fn take_pending_upload_files() -> Option<Vec<String>> {
    PENDING_UPLOADS.lock().ok()?.take()
}

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
            let (x, y) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, x, y, ref_id, state_lock)?;
            let name = ref_name(tab_id, ref_id, state_lock);
            let (x, y) = focus_field(app, tab_id, x, y, &name).unwrap_or((x, y));
            let replace = matches!(op, ActionOp::ClearAndType { .. });
            type_at(app, tab_id, x, y, text, replace, &name)?;
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
            let (x, y) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, x, y, ref_id, state_lock)?;
            let name = ref_name(tab_id, ref_id, state_lock);
            let (x, y) = focus_field(app, tab_id, x, y, &name).unwrap_or((x, y));
            type_at(app, tab_id, x, y, &secret, true, &name)?;
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
        ActionOp::Key { chord } => {
            let mut mods = gpui_kit::Modifiers::default();
            let key = parse_chord_modifiers(chord, &mut mods);
            if crate::engine::windows_key_code_for_key(key) == 0 {
                return Err("unknown key. use: do \"key Enter\"".into());
            }
            let is_enter = matches!(key.to_ascii_lowercase().as_str(), "enter" | "return");
            if is_enter && mods == gpui_kit::Modifiers::default() {
                let in_editor = evaluate_action(
                    app,
                    tab_id,
                    r#"(() => { const a = document.activeElement; return !!(a && (a.isContentEditable || a.getAttribute && a.getAttribute('role') === 'textbox')); })()"#,
                ).unwrap_or_default() == "true";
                if in_editor {
                    crate::agent::playbook::record_action(&format!("key {chord}"));
                    // Rich-text editors often swallow a native Enter silently.
                    // `submit` verifies page change and falls back to the Send
                    // control; keep the historical `ok` result either way.
                    let _ = submit(app, tab_id, policy_config);
                    return Ok(());
                }
            }
            if is_enter || key.eq_ignore_ascii_case("space") {
                check_action_policy(app, tab_id, "document.activeElement", policy_config)?;
            }
            press_key(app, tab_id, key, &mods);
            crate::agent::playbook::record_action(&format!("key {chord}"));
            Ok(())
        }
        ActionOp::KeyDown { chord } => {
            let mut mods = gpui_kit::Modifiers::default();
            let key = parse_chord_modifiers(chord, &mut mods);
            if crate::engine::windows_key_code_for_key(key) == 0 {
                return Err("unknown key. use: do \"keydown Shift\"".into());
            }
            app.engine.set_focus(tab_id, true);
            app.engine.input_key(tab_id, key, &mods, true);
            crate::agent::playbook::record_action(&format!("keydown {chord}"));
            Ok(())
        }
        ActionOp::KeyUp { chord } => {
            let mut mods = gpui_kit::Modifiers::default();
            let key = parse_chord_modifiers(chord, &mut mods);
            if crate::engine::windows_key_code_for_key(key) == 0 {
                return Err("unknown key. use: do \"keyup Shift\"".into());
            }
            app.engine.set_focus(tab_id, true);
            app.engine.input_key(tab_id, key, &mods, false);
            crate::agent::playbook::record_action(&format!("keyup {chord}"));
            Ok(())
        }
        ActionOp::Submit => {
            submissions.push(submit(app, tab_id, policy_config)?);
            Ok(())
        }
        ActionOp::Go { url } => {
            // Check navigation policy
            let cur_url = app
                .spaces()
                .iter()
                .flat_map(|s| &s.tabs)
                .find(|t| t.id == tab_id)
                .map(|t| t.url.as_str())
                .unwrap_or("");
            let nav_check = check_navigation_policy(cur_url, url, policy_config.navigation_mode);
            match nav_check {
                NavigationCheck::Allowed => {}
                NavigationCheck::CrossSiteWarning(detail) => {
                    eprintln!("[anarc-agent] Cross-site navigation warning: {detail}");
                }
                NavigationCheck::RequiresConfirmation => {
                    return Err("needs_confirm: approve navigation in Anarc, then retry".into());
                }
            }

            app.engine.navigate(tab_id, url);
            crate::agent::playbook::record_action(&format!("go {url}"));
            Ok(())
        }
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
        ActionOp::Scroll { direction } => {
            match direction {
                ScrollDirection::Down => app.engine.input_scroll(tab_id, 200, 200, 0.0, -300.0),
                ScrollDirection::Up => app.engine.input_scroll(tab_id, 200, 200, 0.0, 300.0),
                ScrollDirection::Top => app.engine.execute_script(tab_id, "window.scrollTo(0, 0);"),
                ScrollDirection::End => app
                    .engine
                    .execute_script(tab_id, "window.scrollTo(0, document.body.scrollHeight);"),
                ScrollDirection::Ref(target_ref) => {
                    let (cx, cy) = resolve_target_coords(app, target_ref, tab_id, state_lock)?;
                    app.engine.execute_script(
                        tab_id,
                        &format!("window.scrollTo(0, {});", cy.saturating_sub(100)),
                    );
                    let _ = cx;
                }
            }
            Ok(())
        }
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
            let (x1, y1) = match from {
                DragTarget::Ref(r) => resolve_target_coords(app, r, tab_id, state_lock)?,
                DragTarget::Coords(x, y) => (*x, *y),
            };
            let (x2, y2) = match to {
                DragTarget::Ref(r) => resolve_target_coords(app, r, tab_id, state_lock)?,
                DragTarget::Coords(x, y) => (*x, *y),
            };
            check_click_policy(app, tab_id, x1, y1, policy_config)?;
            check_click_policy(app, tab_id, x2, y2, policy_config)?;
            let mods = gpui_kit::Modifiers::default();
            app.engine.send_mouse_click(
                tab_id,
                x1,
                y1,
                cef::MouseButtonType::LEFT,
                false,
                1,
                &mods,
            );
            // Intermediate drag steps with brief pacing for SPA pointermove/rAF handlers
            let steps = 5;
            for i in 1..=steps {
                let cur_x = x1.saturating_add(x2.saturating_sub(x1).saturating_mul(i) / steps);
                let cur_y = y1.saturating_add(y2.saturating_sub(y1).saturating_mul(i) / steps);
                app.engine.send_mouse_move(
                    tab_id,
                    cur_x,
                    cur_y,
                    false,
                    &mods,
                    Some(gpui_kit::MouseButton::Left),
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            app.engine
                .send_mouse_click(tab_id, x2, y2, cef::MouseButtonType::LEFT, true, 1, &mods);
            Ok(())
        }
        ActionOp::Paste { ref_id, text } => {
            if let Some(r) = ref_id {
                let (cx, cy) = resolve_target_coords(app, r, tab_id, state_lock)?;
                check_occlusion(app, tab_id, cx, cy, r, state_lock)?;
                click_at(app, tab_id, cx, cy, policy_config)?;
            }
            let paste_content = if let Some(t) = text {
                let unescaped = t.replace("\\t", "\t").replace("\\n", "\n");
                crate::engine::clipboard::write_system_clipboard(&unescaped);
                unescaped
            } else {
                crate::engine::clipboard::read_system_clipboard().unwrap_or_default()
            };
            app.engine.set_focus(tab_id, true);
            let script = format!(
                r#"(function() {{
                    const text = {text_json};
                    const active = document.activeElement;
                    const isTableOrCanvasInput = text.includes('\t') || text.includes('\n')
                        || (active && active.classList && (active.classList.contains('trix-offscreen') || active.classList.contains('cell-input')));
                    let handled = false;
                    if (isTableOrCanvasInput || !(active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA'))) {{
                        try {{
                            const event = new ClipboardEvent('paste', {{
                                clipboardData: new DataTransfer(),
                                bubbles: true,
                                cancelable: true
                            }});
                            event.clipboardData.setData('text/plain', text);
                            if (active) handled = !active.dispatchEvent(event);
                            else handled = !document.dispatchEvent(event);
                        }} catch (_) {{}}
                    }}
                    if (!handled && active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {{
                        const start = active.selectionStart ?? active.value.length;
                        const end = active.selectionEnd ?? active.value.length;
                        active.setRangeText(text, start, end, 'end');
                        active.dispatchEvent(new Event('input', {{ bubbles: true }}));
                        active.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        return 'ok';
                    }}
                    if (!handled) {{
                        try {{
                            document.execCommand('insertText', false, text);
                            handled = true;
                        }} catch (_) {{}}
                    }}
                    return handled ? 'ok' : 'fallback';
                }})()"#,
                text_json = serde_json::json!(paste_content)
            );
            let res = evaluate_action(app, tab_id, &script).unwrap_or_else(|_| "fallback".into());
            if res != "ok" {
                let control = gpui_kit::Modifiers {
                    control: true,
                    ..Default::default()
                };
                press_key(app, tab_id, "v", &control);
            }
            let op_str = if let Some(r) = ref_id {
                format!("paste {r}")
            } else {
                "paste".into()
            };
            crate::agent::playbook::record_action(&op_str);
            Ok(())
        }
        ActionOp::XY { x, y } => {
            check_occlusion(app, tab_id, *x, *y, "", state_lock)?;
            click_at(app, tab_id, *x, *y, policy_config)
        }
        ActionOp::Upload { ref_id, file_paths } => {
            let mut valid_paths = Vec::new();
            for file_path in file_paths {
                let p = std::path::Path::new(file_path);
                if !p.exists() {
                    return Err(format!("file missing: {file_path}. use: see"));
                }
                let canonical = p
                    .canonicalize()
                    .map_err(|_| "file path unavailable. use: see")?;
                let path_str = canonical.to_string_lossy().to_string();
                let path_lower = path_str.to_lowercase();
                if path_lower.contains("/.ssh")
                    || path_lower.contains("\\.ssh")
                    || path_lower.contains("/.gnupg")
                    || path_lower.contains("\\.gnupg")
                    || path_lower.starts_with("/etc")
                    || path_lower.starts_with("c:\\windows\\system32")
                {
                    return Err("upload blocked. use: see".into());
                }
                valid_paths.push(path_str);
            }
            if valid_paths.is_empty() {
                return Err("no valid files to upload. use: see".into());
            }

            // Set pending upload files for CEF on_file_dialog interception
            set_pending_upload_files(valid_paths.clone());

            let coords = if ref_id.is_empty() {
                None
            } else {
                resolve_target_coords(app, ref_id, tab_id, state_lock).ok()
            };

            // Try CDP DOM.setFileInputFiles first if ref is / contains input[type=file]
            let cdp_eval_script = if let Some((cx, cy)) = coords {
                format!(
                    r#"(() => {{
                        const el = document.elementFromPoint({cx}, {cy});
                        if (!el) return false;
                        const input = el.matches('input[type=file]') ? el :
                            (el.querySelector('input[type=file]') || el.closest('label')?.querySelector('input[type=file]') || document.querySelector('input[type=file]'));
                        return input ? true : false;
                    }})()"#
                )
            } else {
                r#"(() => Boolean(document.querySelector('input[type=file]')))()"#.to_string()
            };

            let has_file_input = evaluate_action(app, tab_id, &cdp_eval_script)
                .map(|s| s == "true")
                .unwrap_or(false);

            if has_file_input {
                let eval_expr = if let Some((cx, cy)) = coords {
                    format!(
                        r#"document.elementFromPoint({cx}, {cy})?.matches('input[type=file]') ?
                            document.elementFromPoint({cx}, {cy}) :
                            (document.elementFromPoint({cx}, {cy})?.querySelector('input[type=file]') ||
                             document.elementFromPoint({cx}, {cy})?.closest('label')?.querySelector('input[type=file]') ||
                             document.querySelector('input[type=file]'))"#
                    )
                } else {
                    "document.querySelector('input[type=file]')".to_string()
                };
                let eval_params = serde_json::json!({
                    "expression": eval_expr,
                    "returnByValue": false
                });
                if let Ok(Ok(obj_res)) = app
                    .engine
                    .devtools(tab_id, "Runtime.evaluate", Some(&eval_params.to_string()))
                    .recv_blocking()
                    && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&obj_res)
                    && let Some(object_id) =
                        parsed.pointer("/result/objectId").and_then(|v| v.as_str())
                {
                    let set_files_params = serde_json::json!({
                        "files": valid_paths,
                        "objectId": object_id
                    });
                    let _ = app
                        .engine
                        .devtools(
                            tab_id,
                            "DOM.setFileInputFiles",
                            Some(&set_files_params.to_string()),
                        )
                        .recv_blocking();
                }
            } else {
                // Fallback for cross-origin iframes (e.g. Google Picker): pierce all
                // frames via CDP DOM.getFlattenedDocument and set files by backendNodeId.
                let flat_params = serde_json::json!({ "depth": -1, "pierce": true });
                if let Ok(Ok(flat_res)) = app
                    .engine
                    .devtools(
                        tab_id,
                        "DOM.getFlattenedDocument",
                        Some(&flat_params.to_string()),
                    )
                    .recv_blocking()
                    && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&flat_res)
                    && let Some(nodes) = parsed.get("nodes").and_then(|v| v.as_array())
                {
                    let backend_id = nodes.iter().find_map(|node| {
                        let is_input = node
                            .get("nodeName")
                            .and_then(|v| v.as_str())
                            .is_some_and(|n| n.eq_ignore_ascii_case("INPUT"));
                        if !is_input {
                            return None;
                        }
                        let attrs = node.get("attributes").and_then(|v| v.as_array())?;
                        let has_file_type = attrs.as_chunks::<2>().0.iter().any(|[key, val]| {
                            key.as_str() == Some("type")
                                && val.as_str().is_some_and(|v| v.eq_ignore_ascii_case("file"))
                        });
                        if has_file_type {
                            node.get("backendNodeId").and_then(|v| v.as_i64())
                        } else {
                            None
                        }
                    });
                    if let Some(backend_node_id) = backend_id {
                        let set_files_params = serde_json::json!({
                            "files": valid_paths,
                            "backendNodeId": backend_node_id
                        });
                        let _ = app
                            .engine
                            .devtools(
                                tab_id,
                                "DOM.setFileInputFiles",
                                Some(&set_files_params.to_string()),
                            )
                            .recv_blocking();
                    }
                }
            }

            // Click to trigger native file chooser if necessary (which LumenDialogHandler intercepts)
            if let Some((cx, cy)) = coords {
                let _ = click_at(app, tab_id, cx, cy, policy_config);
            }

            let change_script = r#"(() => {
                const active = document.activeElement;
                if (active && active.type === 'file') {
                    active.dispatchEvent(new Event('input', { bubbles: true }));
                    active.dispatchEvent(new Event('change', { bubbles: true }));
                }
                const inputs = Array.from(document.querySelectorAll('input[type=file]'));
                for (const inp of inputs) {
                    if (inp.files && inp.files.length > 0) {
                        inp.dispatchEvent(new Event('input', { bubbles: true }));
                        inp.dispatchEvent(new Event('change', { bubbles: true }));
                    }
                }
            })()"#;
            app.engine.execute_script(tab_id, change_script);

            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = take_pending_upload_files();
            crate::agent::playbook::record_action(&format!("upload {ref_id}"));
            Ok(())
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
        ActionOp::Tab { action } => {
            match action {
                TabAction::New { url } => {
                    app.add_tab();
                    if let Some(url) = url {
                        let id = app.active_tab_id().to_owned();
                        app.engine.navigate(&id, url);
                        if let Some(tab) =
                            app.active_space_mut().tabs.iter_mut().find(|t| t.id == id)
                        {
                            tab.url = url.clone();
                        }
                        app.save_state();
                    }
                }
                TabAction::Close { target } => {
                    if let Some(target) = target {
                        let id = resolve_tab_target(app, Some(target))?;
                        app.close_tab_by_id(&id);
                    } else {
                        app.close_active_tab();
                    }
                }
                TabAction::Pin { target } => {
                    let id = resolve_tab_target(app, target.as_deref())?;
                    app.pin_tab(&id, true);
                }
                TabAction::Unpin { target } => {
                    let id = resolve_tab_target(app, target.as_deref())?;
                    app.pin_tab(&id, false);
                }
                TabAction::Favorite { target } => {
                    let id = resolve_tab_target(app, target.as_deref())?;
                    app.favorite_tab(&id, true);
                }
                TabAction::Unfavorite { target } => {
                    let id = resolve_tab_target(app, target.as_deref())?;
                    app.favorite_tab(&id, false);
                }
                TabAction::Select(target) => {
                    let id = resolve_tab_target(app, Some(target))?;
                    app.activate_tab_in_active_space(&id);
                    // Ensure browser is awake for newly activated tab
                    let space_id = app.active_space().id.clone();
                    let url = app
                        .active_space()
                        .tabs
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| t.url.clone())
                        .unwrap_or_default();
                    if !app.engine.has_browser(&id) && !app.engine.agent_browser_pending(&id) {
                        let wake_url = if url.is_empty() || url == "lumen://welcome" {
                            "about:blank"
                        } else {
                            &url
                        };
                        let _ = app
                            .engine
                            .ensure_browser(&space_id, &id, wake_url, 768, 512, 0, 0.0);
                    }
                    // Wait for the execution context AND a hydrated DOM. A
                    // successful eval alone can still report "loading", which
                    // caused `see` to index about:blank on freshly woken tabs.
                    for _ in 0..10 {
                        if app.engine.has_browser(&id)
                            && let Ok(Ok(state)) = app
                                .engine
                                .evaluate(&id, "document.readyState")
                                .recv_blocking()
                            && state.trim_matches('"') != "loading"
                        {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
            Ok(())
        }
        ActionOp::Space { target } => {
            let t = target.trim();
            let space_id = if let Ok(idx) = t.parse::<usize>() {
                idx.checked_sub(1)
                    .and_then(|i| app.spaces().get(i))
                    .map(|s| s.id.clone())
            } else {
                let lower = t.to_lowercase();
                app.spaces()
                    .iter()
                    .find(|s| s.id == t || s.name.to_lowercase() == lower)
                    .or_else(|| {
                        app.spaces()
                            .iter()
                            .find(|s| s.name.to_lowercase().contains(&lower))
                    })
                    .map(|s| s.id.clone())
            }
            .ok_or_else(|| {
                note_error(ErrorCode::NotFound);
                format!("unknown space '{t}'. use: tabs")
            })?;
            app.activate_space(&space_id);
            Ok(())
        }
        ActionOp::Split { action } => {
            match action {
                SplitAction::WithNext => {
                    app.split_active_with_next();
                }
                SplitAction::Exit => {
                    app.exit_split();
                }
                SplitAction::WithUrl(url) => {
                    let first = app.active_tab_id().to_owned();
                    app.add_tab();
                    let second = app.active_tab_id().to_owned();
                    app.engine.navigate(&second, url);
                    if let Some(tab) = app
                        .active_space_mut()
                        .tabs
                        .iter_mut()
                        .find(|t| t.id == second)
                    {
                        tab.url = url.clone();
                    }
                    app.save_state();
                    if !first.is_empty() && first != second {
                        let _ = app.split_tabs(&first, &second);
                    }
                }
                SplitAction::Pair(a, b) => {
                    let first = resolve_tab_target(app, Some(a))?;
                    let second = if b.starts_with("http://") || b.starts_with("https://") {
                        app.add_tab();
                        let id = app.active_tab_id().to_owned();
                        app.engine.navigate(&id, b);
                        if let Some(tab) =
                            app.active_space_mut().tabs.iter_mut().find(|t| t.id == id)
                        {
                            tab.url = b.clone();
                        }
                        app.save_state();
                        id
                    } else {
                        resolve_tab_target(app, Some(b))?
                    };
                    if !app.split_tabs(&first, &second) {
                        return Err("split failed. use: tabs".into());
                    }
                }
            }
            Ok(())
        }
        ActionOp::Wait { condition } => {
            match condition {
                WaitCondition::Stable(_) | WaitCondition::Stream { .. } => {}
                WaitCondition::Millis(ms) => {
                    std::thread::sleep(std::time::Duration::from_millis((*ms).min(30000)))
                }
                WaitCondition::Idle => std::thread::sleep(std::time::Duration::from_millis(150)),
                WaitCondition::Text(text) => {
                    let wait_script = format!(
                        "{} window.__anarc_wait_for('text_appears', {}, 8000);",
                        crate::agent::observe::build_indexer_injection_script(),
                        serde_json::json!(text)
                    );
                    let _ = evaluate_action(app, tab_id, &wait_script);
                }
            }
            Ok(())
        }
    }
}

pub(super) fn compact_diff(diff: &crate::agent::refs::SnapshotDiff) -> String {
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
