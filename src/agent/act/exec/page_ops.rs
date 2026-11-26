//! Page-level input DSL operations: typing, key chords, drag, and paste.

use std::sync::{Arc, Mutex};

use super::super::dom::{
    check_action_policy, check_click_policy, check_occlusion, click_at, evaluate_action,
    focus_field, parse_chord_modifiers, press_key, ref_name, resolve_target_coords, submit,
    type_at,
};
use super::super::types::DragTarget;
use crate::agent::plugin::AgentState;
use crate::agent::policy::PolicyConfig;

pub(super) fn type_into_ref(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    ref_id: &str,
    text: &str,
    replace: bool,
    state_lock: &Arc<Mutex<AgentState>>,
) -> Result<(), String> {
    let (x, y) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
    check_occlusion(app, tab_id, x, y, ref_id, state_lock)?;
    let name = ref_name(tab_id, ref_id, state_lock);
    let (x, y) = focus_field(app, tab_id, x, y, &name).unwrap_or((x, y));
    type_at(app, tab_id, x, y, text, replace, &name)
}

pub(super) fn execute_key_press(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    chord: &str,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
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
        )
        .unwrap_or_default()
            == "true";
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

pub(super) fn execute_key_edge(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    chord: &str,
    pressed: bool,
) -> Result<(), String> {
    let mut mods = gpui_kit::Modifiers::default();
    let key = parse_chord_modifiers(chord, &mut mods);
    if crate::engine::windows_key_code_for_key(key) == 0 {
        let verb = if pressed { "keydown" } else { "keyup" };
        return Err(format!("unknown key. use: do \"{verb} Shift\""));
    }
    app.engine.set_focus(tab_id, true);
    app.engine.input_key(tab_id, key, &mods, pressed);
    let verb = if pressed { "keydown" } else { "keyup" };
    crate::agent::playbook::record_action(&format!("{verb} {chord}"));
    Ok(())
}

pub(super) fn execute_drag(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    from: &DragTarget,
    to: &DragTarget,
    state_lock: &Arc<Mutex<AgentState>>,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
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
    app.engine
        .send_mouse_click(tab_id, x1, y1, cef::MouseButtonType::LEFT, false, 1, &mods);
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

pub(super) fn execute_paste(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    ref_id: Option<&str>,
    text: Option<&str>,
    state_lock: &Arc<Mutex<AgentState>>,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
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
    let op_str = match ref_id {
        Some(r) => format!("paste {r}"),
        None => "paste".into(),
    };
    crate::agent::playbook::record_action(&op_str);
    Ok(())
}
