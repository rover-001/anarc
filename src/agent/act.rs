use super::observe::{build_see_call_script, format_see_output};
use super::policy::{check_navigation_policy, check_sensitive_intent, log_audit_event, resolve_secret, NavigationCheck, PolicyConfig};
use super::protocol::AgentResponse;
use super::refs::RawIndex;

#[derive(Clone, Debug, PartialEq)]
pub enum ScrollDirection {
    Down,
    Up,
    Top,
    End,
    Ref(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TabAction {
    New,
    Select(usize),
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaitCondition {
    Idle,
    Millis(u64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionOp {
    Click { ref_id: String },
    SemanticClick { kind: char, label: String },
    ClearAndType { ref_id: String, text: String },
    AppendText { ref_id: String, text: String },
    TypeSecret { ref_id: String, secret_name: String },
    SemanticType { kind: char, label: String, text: String },
    Key { chord: String },
    Go { url: String },
    Back,
    Fwd,
    Reload,
    Scroll { direction: ScrollDirection },
    Hover { ref_id: String },
    Focus { ref_id: String },
    Drag { from_ref: String, to_ref: String },
    XY { x: i32, y: i32 },
    Upload { ref_id: String, file_path: String },
    Dialog { accept: bool },
    Tab { action: TabAction },
    Wait { condition: WaitCondition },
}

pub fn parse_dsl(input: &str) -> Result<Vec<ActionOp>, String> {
    let mut ops = Vec::new();
    let sanitized = input.replace('\r', "");
    let statements = sanitized.split([';', '\n']);

    for stmt in statements {
        let trimmed = stmt.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        // Check for playbook macro expansion (@macro_name arg1 arg2...)
        if let Some(stripped) = trimmed.strip_prefix('@') {
            if !stripped.starts_with("secret:") {
                let parts: Vec<&str> = stripped.split_whitespace().collect();
                let macro_name = parts.get(0).copied().unwrap_or("");
                let raw_args: Vec<String> = parts.get(1..).unwrap_or(&[]).iter().map(|a| unquote(a)).collect();
                let playbook = super::playbook::get_or_init_registry();
                let expanded = playbook.expand_macro(macro_name, &raw_args)?;
                let expanded_ops = parse_dsl(&expanded)?;
                ops.extend(expanded_ops);
                continue;
            }
        }

        let op = parse_single_op(trimmed)?;
        ops.push(op);
    }

    Ok(ops)
}

fn parse_single_op(raw: &str) -> Result<ActionOp, String> {
    let s = raw.trim();

    // Key chord: "key Enter", "key ctrl+a"
    if let Some(stripped) = s.strip_prefix("key ").or_else(|| (s == "key").then_some("")) {
        let chord = stripped.trim().to_string();
        if chord.is_empty() {
            return Err("missing key. use: do \"key Enter\"".into());
        }
        return Ok(ActionOp::Key { chord });
    }

    // Go url: "go https://..."
    if let Some(stripped) = s.strip_prefix("go ").or_else(|| (s == "go").then_some("")) {
        let url = stripped.trim().to_string();
        if url.is_empty() {
            return Err("missing URL. use: do \"go https://example.com\"".into());
        }
        return Ok(ActionOp::Go { url });
    }

    if s == "back" {
        return Ok(ActionOp::Back);
    }
    if s == "fwd" || s == "forward" {
        return Ok(ActionOp::Fwd);
    }
    if s == "reload" {
        return Ok(ActionOp::Reload);
    }

    // Scroll: "scroll down", "scroll up", "scroll top", "scroll end", "scroll b1"
    if let Some(stripped) = s.strip_prefix("scroll ") {
        let dir_str = stripped.trim();
        let direction = match dir_str {
            "down" => ScrollDirection::Down,
            "up" => ScrollDirection::Up,
            "top" => ScrollDirection::Top,
            "end" => ScrollDirection::End,
            target => ScrollDirection::Ref(target.to_string()),
        };
        return Ok(ActionOp::Scroll { direction });
    }

    // Hover
    if let Some(stripped) = s.strip_prefix("hover ") {
        let r = stripped.trim().to_string();
        return Ok(ActionOp::Hover { ref_id: r });
    }

    // Focus
    if let Some(stripped) = s.strip_prefix("focus ") {
        let r = stripped.trim().to_string();
        return Ok(ActionOp::Focus { ref_id: r });
    }

    // Drag: "drag b1 b2"
    if let Some(stripped) = s.strip_prefix("drag ") {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let from = parts.get(0).copied().unwrap_or("").to_string();
        let to = parts.get(1).copied().unwrap_or("").to_string();
        if from.is_empty() || to.is_empty() {
            return Err("missing drag refs. use: see".into());
        }
        return Ok(ActionOp::Drag { from_ref: from, to_ref: to });
    }

    // XY: "xy 100 200"
    if let Some(stripped) = s.strip_prefix("xy ") {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let x: i32 = parts.get(0).and_then(|v| v.parse().ok()).unwrap_or(0);
        let y: i32 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        return Ok(ActionOp::XY { x, y });
    }

    // Upload: "up ref /path/file"
    if let Some(stripped) = s.strip_prefix("up ") {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let r = parts.get(0).copied().unwrap_or("").to_string();
        let path = parts.get(1).copied().unwrap_or("").to_string();
        return Ok(ActionOp::Upload { ref_id: r, file_path: path });
    }

    // Dialog: "dialog ok", "dialog cancel"
    if let Some(stripped) = s.strip_prefix("dialog ") {
        let d = stripped.trim();
        let accept = d == "ok" || d == "accept" || d == "true";
        return Ok(ActionOp::Dialog { accept });
    }

    // Tab: "tab new", "tab 2", "tab close"
    if let Some(stripped) = s.strip_prefix("tab ") {
        let t = stripped.trim();
        let action = if t == "new" {
            TabAction::New
        } else if t == "close" {
            TabAction::Close
        } else if let Ok(num) = t.parse::<usize>() {
            TabAction::Select(num)
        } else {
            return Err("unknown tab action. use: tabs".into());
        };
        return Ok(ActionOp::Tab { action });
    }

    // Wait: "wait idle", "wait 500", "wait \"Text\""
    if let Some(stripped) = s.strip_prefix("wait ") {
        let w = stripped.trim();
        let condition = if w == "idle" {
            WaitCondition::Idle
        } else if let Ok(ms) = w.parse::<u64>() {
            WaitCondition::Millis(ms)
        } else {
            let unquoted = unquote(w);
            WaitCondition::Text(unquoted)
        };
        return Ok(ActionOp::Wait { condition });
    }

    // Semantic selectors: b:"Sign in" or i:"Email"="alice"
    if let Some(colon_idx) = s.find(':') {
        let kind_str = s.get(0..colon_idx).unwrap_or("");
        if kind_str.len() == 1 {
            let kind_char = kind_str.chars().next().unwrap_or('x');
            let rest = s.get(colon_idx.saturating_add(1)..).unwrap_or("");

            if let Some(eq_idx) = rest.find('=') {
                let label_part = rest.get(0..eq_idx).unwrap_or("");
                let text_part = rest.get(eq_idx.saturating_add(1)..).unwrap_or("");
                return Ok(ActionOp::SemanticType {
                    kind: kind_char,
                    label: unquote(label_part),
                    text: unquote(text_part),
                });
            } else {
                return Ok(ActionOp::SemanticClick {
                    kind: kind_char,
                    label: unquote(rest),
                });
            }
        }
    }

    // Assignment: ref=text or ref+text or ref=@secret:name
    if let Some(eq_idx) = s.find('=') {
        let ref_id = s.get(0..eq_idx).unwrap_or("").trim().to_string();
        let val_part = s.get(eq_idx.saturating_add(1)..).unwrap_or("").trim();

        if let Some(secret_name) = val_part.strip_prefix("@secret:") {
            return Ok(ActionOp::TypeSecret {
                ref_id,
                secret_name: secret_name.trim().to_string(),
            });
        }
        return Ok(ActionOp::ClearAndType {
            ref_id,
            text: unquote(val_part),
        });
    }

    if let Some(plus_idx) = s.find('+') {
        let ref_id = s.get(0..plus_idx).unwrap_or("").trim().to_string();
        let val_part = s.get(plus_idx.saturating_add(1)..).unwrap_or("").trim();
        return Ok(ActionOp::AppendText {
            ref_id,
            text: unquote(val_part),
        });
    }

    // Default: simple click on ref
    if !s.is_empty() {
        return Ok(ActionOp::Click { ref_id: s.to_string() });
    }

    Err("unknown action. use: see".into())
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        if t.len() >= 2 {
            t.get(1..t.len().saturating_sub(1)).unwrap_or("").to_string()
        } else {
            String::new()
        }
    } else {
        t.to_string()
    }
}

pub fn execute_dsl(
    app: &mut crate::app::Lumen,
    tab_id: &str,
    ops: &[ActionOp],
) -> AgentResponse {
    let agent_state = app.engine.agent_state();
    let mut op_count: usize = 0;
    let policy_config = PolicyConfig::default();

    for (index, op) in ops.iter().enumerate() {
        let op_num = index.saturating_add(1);
        let res = execute_single_op(app, tab_id, op, &agent_state, &policy_config);
        if let Err(e) = res {
            log_audit_event(&format!("op{op_num}: {op:?}"), tab_id, &format!("fail: {e}"));
            return AgentResponse::err(format!("fail op{op_num}: {e}"));
        }
        op_count = op_count.saturating_add(1);
    }

    // Settle wait and post-action observation
    std::thread::sleep(std::time::Duration::from_millis(150));

    let script = build_see_call_script("view");
    let eval_rx = app.engine.evaluate(tab_id, &script);
    let raw_res = futures_lite::future::block_on(async { eval_rx.recv().await });

    let raw_json = match raw_res {
        Ok(Ok(s)) => s,
        _ => {
            log_audit_event("batch", tab_id, &format!("ok {op_count}"));
            return AgentResponse::success(format!("ok {op_count}"));
        }
    };

    let raw_index: RawIndex = match serde_json::from_str(&raw_json) {
        Ok(idx) => idx,
        _ => {
            log_audit_event("batch", tab_id, &format!("ok {op_count}"));
            return AgentResponse::success(format!("ok {op_count}"));
        }
    };

    let mut guard = match agent_state.lock() {
        Ok(g) => g,
        Err(_) => return AgentResponse::success(format!("ok {op_count}")),
    };

    let registry = guard.registries.entry(tab_id.to_string()).or_default();
    let (header, elements, diff, gen_changed) = registry.update_from_raw(raw_index);

    log_audit_event("batch", tab_id, &format!("ok {op_count} · g{}", header.generation));

    if gen_changed {
        let see_output = format_see_output(&header, &elements, Some(&super::plugin::compact_tabs(app)));
        AgentResponse::success(format!("ok {op_count} · g{}\n{see_output}", header.generation))
    } else if !diff.is_empty() {
        let diff_str = diff.format_diff();
        AgentResponse::success(format!("ok {op_count} · g{}\n{diff_str}", header.generation))
    } else {
        AgentResponse::success(format!("ok {op_count} · g{}", header.generation))
    }
}

fn execute_single_op(
    app: &mut crate::app::Lumen,
    tab_id: &str,
    op: &ActionOp,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
    match op {
        ActionOp::Click { ref_id } => {
            // Check sensitive intent confirm gate
            if let Ok(guard) = state_lock.lock() {
                if let Some(reg) = guard.registries.get(tab_id) {
                    if let Some(el) = reg.resolve_ref(ref_id) {
                        if let Some(confirm_msg) = check_sensitive_intent(&el.name, policy_config) {
                            return Err(confirm_msg);
                        }
                    }
                }
            }

            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, ref_id)?;
            app.engine.input_move(tab_id, cx, cy);
            app.engine.input_click(tab_id, cx, cy, None);
            super::playbook::record_action(ref_id);
            Ok(())
        }
        ActionOp::ClearAndType { ref_id, text } => {
            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, ref_id)?;
            app.engine.input_move(tab_id, cx, cy);
            app.engine.input_click(tab_id, cx, cy, None);
            let focus_script = format!(
                "const el = document.elementFromPoint({cx}, {cy}); \
                 if (el) {{ \
                     el.focus(); \
                     const ce = el.closest ? (el.closest('[contenteditable=\"true\"], rich-textarea, [role=\"textbox\"]') || el) : el; \
                     if (ce && ce.focus) ce.focus(); \
                     if ('value' in el) {{ el.value = ''; }} \
                     else if (el.isContentEditable) {{ el.innerText = ''; }} \
                 }}"
            );
            app.engine.execute_script(tab_id, &focus_script);
            std::thread::sleep(std::time::Duration::from_millis(50));
            let type_rx = app.engine.input_type(tab_id, text);
            let _ = futures_lite::future::block_on(async { type_rx.recv().await });
            let rich_fallback = format!(
                "const el = document.activeElement; \
                 if (el) {{ \
                     const target = (el.isContentEditable || (el.closest && el.closest('[contenteditable=\"true\"], rich-textarea, [role=\"textbox\"]'))) ? el : null; \
                     if (target && (!target.innerText || target.innerText.trim().length === 0)) {{ \
                         document.execCommand('insertText', false, '{}'); \
                         target.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                     }} \
                 }}",
                text.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n")
            );
            app.engine.execute_script(tab_id, &rich_fallback);
            super::playbook::record_action(&format!("{ref_id}=\"{text}\""));
            Ok(())
        }
        ActionOp::AppendText { ref_id, text } => {
            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            app.engine.input_move(tab_id, cx, cy);
            app.engine.input_click(tab_id, cx, cy, None);
            std::thread::sleep(std::time::Duration::from_millis(50));
            let type_rx = app.engine.input_type(tab_id, text);
            let _ = futures_lite::future::block_on(async { type_rx.recv().await });
            let rich_fallback = format!(
                "const el = document.activeElement; \
                 if (el) {{ \
                     const target = (el.isContentEditable || (el.closest && el.closest('[contenteditable=\"true\"], rich-textarea, [role=\"textbox\"]'))) ? el : null; \
                     if (target && (!target.innerText || !target.innerText.includes('{}'))) {{ \
                         document.execCommand('insertText', false, '{}'); \
                         target.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                     }} \
                 }}",
                text.replace('\\', "\\\\").replace('\'', "\\'"),
                text.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n")
            );
            app.engine.execute_script(tab_id, &rich_fallback);
            super::playbook::record_action(&format!("{ref_id}+\"{text}\""));
            Ok(())
        }
        ActionOp::TypeSecret { ref_id, secret_name } => {
            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            app.engine.input_move(tab_id, cx, cy);
            app.engine.input_click(tab_id, cx, cy, None);
            let clear_script = "if (document.activeElement) { document.activeElement.value = ''; }";
            app.engine.execute_script(tab_id, clear_script);
            let secret_val = resolve_secret(secret_name).unwrap_or_default();
            if secret_val.is_empty() {
                return Err("secret missing. use: see".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            let type_rx = app.engine.input_type(tab_id, &secret_val);
            let _ = futures_lite::future::block_on(async { type_rx.recv().await });
            super::playbook::record_action(&format!("{ref_id}=@secret:{secret_name}"));
            Ok(())
        }
        ActionOp::SemanticClick { kind, label } => {
            if let Some(confirm_msg) = check_sensitive_intent(label, policy_config) {
                return Err(confirm_msg);
            }
            let (cx, cy, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, &id)?;
            app.engine.input_click(tab_id, cx, cy, None);
            super::playbook::record_action(&format!("{kind}:\"{label}\""));
            Ok(())
        }
        ActionOp::SemanticType { kind, label, text } => {
            let (cx, cy, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, &id)?;
            app.engine.input_click(tab_id, cx, cy, None);
            let clear_script = "if (document.activeElement) { document.activeElement.value = ''; }";
            app.engine.execute_script(tab_id, clear_script);
            let type_rx = app.engine.input_type(tab_id, text);
            let _ = futures_lite::future::block_on(async { type_rx.recv().await });
            super::playbook::record_action(&format!("{kind}:\"{label}\"=\"{text}\""));
            Ok(())
        }
        ActionOp::Key { chord } => {
            let mut mods = gpui_kit::Modifiers::default();
            let key = parse_chord_modifiers(chord, &mut mods);
            app.engine.input_key(tab_id, key, &mods, true);
            app.engine.input_key(tab_id, key, &mods, false);
            if key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return") {
                let enter_js = "if (document.activeElement) { \
                    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true })); \
                    document.activeElement.dispatchEvent(new KeyboardEvent('keypress', { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true })); \
                    document.activeElement.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true })); \
                }";
                app.engine.execute_script(tab_id, enter_js);
            }
            super::playbook::record_action(&format!("key {chord}"));
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
                NavigationCheck::RequiresConfirmation(_) => {
                    return Err("needs_confirm: approve navigation in Anarc, then retry".into());
                }
            }

            let script = format!("location.href = '{}';", url.replace('\'', "\\'"));
            app.engine.execute_script(tab_id, &script);
            super::playbook::record_action(&format!("go {url}"));
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
                ScrollDirection::End => app.engine.execute_script(tab_id, "window.scrollTo(0, document.body.scrollHeight);"),
                ScrollDirection::Ref(target_ref) => {
                    let (cx, cy) = resolve_ref_coords(target_ref, tab_id, state_lock)?;
                    app.engine.execute_script(tab_id, &format!("window.scrollTo(0, {});", cy.saturating_sub(100)));
                    let _ = cx;
                }
            }
            Ok(())
        }
        ActionOp::Hover { ref_id } => {
            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            app.engine.input_move(tab_id, cx, cy);
            Ok(())
        }
        ActionOp::Focus { ref_id } => {
            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            app.engine.input_move(tab_id, cx, cy);
            app.engine.input_click(tab_id, cx, cy, None);
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
        ActionOp::Drag { from_ref, to_ref } => {
            let (x1, y1) = resolve_ref_coords(from_ref, tab_id, state_lock)?;
            let (x2, y2) = resolve_ref_coords(to_ref, tab_id, state_lock)?;
            let mods = gpui_kit::Modifiers::default();
            app.engine.send_mouse_click(tab_id, x1, y1, cef::MouseButtonType::LEFT, false, 1, &mods);
            // Intermediate drag steps
            let steps = 5;
            for i in 1..=steps {
                let cur_x = x1.saturating_add(((x2.saturating_sub(x1)) * i) / steps);
                let cur_y = y1.saturating_add(((y2.saturating_sub(y1)) * i) / steps);
                app.engine.send_mouse_move(tab_id, cur_x, cur_y, false, &mods, Some(gpui_kit::MouseButton::Left));
            }
            app.engine.send_mouse_click(tab_id, x2, y2, cef::MouseButtonType::LEFT, true, 1, &mods);
            Ok(())
        }
        ActionOp::XY { x, y } => {
            app.engine.input_click(tab_id, *x, *y, None);
            Ok(())
        }
        ActionOp::Upload { ref_id, file_path } => {
            let p = std::path::Path::new(file_path);
            if !p.exists() {
                return Err("file missing. use: see".into());
            }
            let canonical = p.canonicalize().map_err(|_| "file path unavailable. use: see")?;
            let path_str = canonical.to_string_lossy();
            if path_str.contains("/.ssh") || path_str.contains("/.gnupg") || path_str.starts_with("/etc") {
                return Err("upload blocked. use: see".into());
            }

            let (cx, cy) = resolve_ref_coords(ref_id, tab_id, state_lock)?;
            app.engine.input_click(tab_id, cx, cy, None);
            let script = "(function() { var el = document.activeElement; if (el && el.type === 'file') { el.dispatchEvent(new Event('change', { bubbles: true })); } })()";
            app.engine.execute_script(tab_id, script);
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
                TabAction::New => app.add_tab(),
                TabAction::Close => app.close_active_tab(),
                TabAction::Select(n) => {
                    let id = n.checked_sub(1).and_then(|index| app.active_space().tabs.get(index))
                        .map(|tab| tab.id.clone())
                        .ok_or_else(|| super::plugin::no_such_tab(&super::plugin::compact_tabs(app)))?;
                    app.activate_tab_in_active_space(&id);
                }
            }
            Ok(())
        }
        ActionOp::Wait { condition } => {
            match condition {
                WaitCondition::Millis(ms) => std::thread::sleep(std::time::Duration::from_millis(*ms)),
                WaitCondition::Idle => std::thread::sleep(std::time::Duration::from_millis(150)),
                WaitCondition::Text(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
            Ok(())
        }
    }
}

fn resolve_ref_coords(
    ref_id: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
) -> Result<(i32, i32), String> {
    let guard = state_lock.lock().map_err(|_| "state unavailable. use: status")?;
    let registry = guard.registries.get(tab_id).ok_or_else(|| {
        "index missing. use: see"
    })?;
    let el = registry.resolve_ref(ref_id).ok_or("stale ref. use: see")?;
    Ok(el.center)
}

fn resolve_semantic_coords(
    kind: char,
    label: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
) -> Result<(i32, i32, String), String> {
    let guard = state_lock.lock().map_err(|_| "state unavailable. use: status")?;
    let registry = guard.registries.get(tab_id).ok_or_else(|| {
        "index missing. use: see"
    })?;

    let label_lower = label.to_lowercase();
    for el in registry.current_elements.values() {
        if el.kind == kind && el.name.to_lowercase().contains(&label_lower) {
            return Ok((el.center.0, el.center.1, el.id.clone()));
        }
    }
    Err("ref missing. use: see".into())
}

fn check_occlusion(
    app: &crate::app::Lumen,
    tab_id: &str,
    x: i32,
    y: i32,
    _ref_id: &str,
) -> Result<(), String> {
    let script = format!(
        "(function() {{ \
            var el = document.elementFromPoint({x}, {y}); \
            if (!el) return 'ok'; \
            var b = el.closest('[role=\"banner\"], [role=\"alert\"], .cookie-banner, .toast'); \
            if (b) return '!banner'; \
            return 'ok'; \
        }})()"
    );
    let eval_rx = app.engine.evaluate(tab_id, &script);
    if let Ok(Ok(res)) = futures_lite::future::block_on(async { eval_rx.recv().await }) {
        if res == "!banner" {
            return Err("ref covered by !banner. use: see".into());
        }
    }
    Ok(())
}

fn parse_chord_modifiers<'a>(chord: &'a str, mods: &mut gpui_kit::Modifiers) -> &'a str {
    let lower = chord.to_lowercase();
    if lower.contains("ctrl+") || lower.contains("control+") {
        mods.control = true;
    }
    if lower.contains("alt+") {
        mods.alt = true;
    }
    if lower.contains("shift+") {
        mods.shift = true;
    }
    if lower.contains("cmd+") || lower.contains("meta+") {
        mods.platform = true;
    }

    if let Some(pos) = chord.rfind('+') {
        chord.get(pos.saturating_add(1)..).unwrap_or(chord).trim()
    } else {
        chord.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_errors_give_next_commands() {
        for input in ["key", "go", "drag b1", "tab invalid", "bogus", "@missing"] {
            let error = parse_dsl(input).err().unwrap_or_default();
            assert!(error.contains("use:") || error.contains("retry"), "{input}: {error}");
            assert!(error.len() <= 86, "{input}: {error}");
        }
    }

    #[test]
    fn test_dsl_parser_variety() {
        let script = r#"
            b1;
            i1="rover@test.com";
            i2=@secret:github;
            b:"Sign in";
            key Enter;
            go https://example.com;
            scroll down;
            wait idle;
        "#;

        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 8);
        assert_eq!(ops.get(0), Some(&ActionOp::Click { ref_id: "b1".into() }));
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "rover@test.com".into()
            })
        );
        assert_eq!(
            ops.get(2),
            Some(&ActionOp::TypeSecret {
                ref_id: "i2".into(),
                secret_name: "github".into()
            })
        );
        assert_eq!(
            ops.get(3),
            Some(&ActionOp::SemanticClick {
                kind: 'b',
                label: "Sign in".into()
            })
        );
        assert_eq!(ops.get(4), Some(&ActionOp::Key { chord: "Enter".into() }));
        assert_eq!(ops.get(5), Some(&ActionOp::Go { url: "https://example.com".into() }));
        assert_eq!(
            ops.get(6),
            Some(&ActionOp::Scroll {
                direction: ScrollDirection::Down
            })
        );
        assert_eq!(ops.get(7), Some(&ActionOp::Wait { condition: WaitCondition::Idle }));
    }

    #[test]
    fn test_macro_expansion_in_dsl() {
        let script = r#"
            @search "agent control";
            scroll down;
        "#;

        let ops = parse_dsl(script).unwrap_or_default();
        assert!(ops.len() >= 2);
    }

    #[test]
    fn test_dsl_parser_fuzz_never_panics() {
        let garbage = [
            "",
            ";;;;",
            "\n\n\n",
            "=",
            "+",
            ":",
            "key",
            "go",
            "scroll",
            "hover",
            "focus",
            "drag",
            "xy",
            "up",
            "tab",
            "wait",
            "b1=foo=bar=baz",
            "i1+\"unfinished quote",
            "x:\"weird\":extra:colons",
            "@unknown_macro_404",
            "@secret:",
            "dialog whatever",
            "scroll non_existent_direction",
            "xy not_a_number not_a_number",
            "tab not_a_number",
            "wait not_a_number",
            "very_long_string_without_delimiters_1234567890_abcdefghijklmnopqrstuvwxyz",
        ];

        for &sample in &garbage {
            let _ = parse_dsl(sample);
        }
    }
}
