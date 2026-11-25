use anarc_telemetry::schema::ErrorCode;

use super::super::policy::{ACTION_CONTEXT, PolicyConfig, confirmation_message};
use super::note_error;
use super::parse::{is_ref_id, normalize_ref_token, unquote};

pub(super) fn resolve_tab_target(app: &crate::app::Anarc, target: Option<&str>) -> Result<String, String> {
    if let Some(t) = target {
        let t = t.trim();
        if let Ok(n) = t.parse::<usize>() {
            n.checked_sub(1)
                .and_then(|idx| app.active_space().tabs.get(idx))
                .map(|tab| tab.id.clone())
                .ok_or_else(|| {
                    note_error(ErrorCode::NotFound);
                    super::super::plugin::no_such_tab(&super::super::plugin::compact_tabs(app))
                })
        } else if app.active_space().tabs.iter().any(|tab| tab.id == t) {
            Ok(t.to_string())
        } else if let Some(tab) =
            app.active_space().tabs.iter().find(|tab| {
                tab.url.contains(t) || tab.title.to_lowercase().contains(&t.to_lowercase())
            })
        {
            Ok(tab.id.clone())
        } else {
            note_error(ErrorCode::NotFound);
            Err(super::super::plugin::no_such_tab(&super::super::plugin::compact_tabs(
                app,
            )))
        }
    } else {
        let active = app.active_tab_id();
        if active.is_empty() {
            note_error(ErrorCode::NotFound);
            Err("no active tab".into())
        } else {
            Ok(active.to_string())
        }
    }
}

pub(super) fn check_click_policy(
    app: &crate::app::Anarc,
    tab: &str,
    x: i32,
    y: i32,
    policy: &PolicyConfig,
) -> Result<(), String> {
    check_action_policy(
        app,
        tab,
        &format!("document.elementFromPoint({x}, {y})"),
        policy,
    )
}

pub(super) fn check_action_policy(
    app: &crate::app::Anarc,
    tab: &str,
    target: &str,
    policy: &PolicyConfig,
) -> Result<(), String> {
    let reply = evaluate_action(app, tab, &format!("({ACTION_CONTEXT})({target})"))?;
    let (origin, intent) = serde_json::from_str::<(String, String)>(&reply)
        .map_err(|_| "action context unavailable. use: see")?;
    let state = app.engine.agent_state();
    let state = state.lock().map_err(|_| "state unavailable. use: status")?;
    if state.confirmations.required(policy, &origin, &intent) {
        return Err(confirmation_message(&intent));
    }
    Ok(())
}

pub(super) fn click_at(
    app: &crate::app::Anarc,
    tab: &str,
    x: i32,
    y: i32,
    policy: &PolicyConfig,
) -> Result<(), String> {
    check_click_policy(app, tab, x, y, policy)?;
    app.engine.set_focus(tab, true);
    for kind in ["mousePressed", "mouseReleased"] {
        let params = serde_json::json!({
            "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1,
        });
        let result =
            app.engine
                .devtools(tab, "Input.dispatchMouseEvent", Some(&params.to_string()));
        result
            .recv_blocking()
            .ok()
            .and_then(Result::ok)
            .ok_or("click failed. use: see")?;
    }
    Ok(())
}

pub(super) fn dblclick_at(
    app: &crate::app::Anarc,
    tab: &str,
    x: i32,
    y: i32,
    policy: &PolicyConfig,
) -> Result<(), String> {
    check_click_policy(app, tab, x, y, policy)?;
    app.engine.set_focus(tab, true);
    for click_count in [1, 2] {
        for kind in ["mousePressed", "mouseReleased"] {
            let params = serde_json::json!({
                "type": kind, "x": x, "y": y, "button": "left", "clickCount": click_count,
            });
            let result =
                app.engine
                    .devtools(tab, "Input.dispatchMouseEvent", Some(&params.to_string()));
            result
                .recv_blocking()
                .ok()
                .and_then(Result::ok)
                .ok_or("dblclick failed. use: see")?;
        }
    }
    Ok(())
}

pub(super) fn press_key(app: &crate::app::Anarc, tab: &str, key: &str, mods: &gpui_kit::Modifiers) {
    app.engine.set_focus(tab, true);
    app.engine.input_key(tab, key, mods, true);
    app.engine.input_key(tab, key, mods, false);
}

pub(super) fn type_at(
    app: &crate::app::Anarc,
    tab: &str,
    x: i32,
    y: i32,
    text: &str,
    replace: bool,
    label: &str,
) -> Result<(), String> {
    app.engine.set_focus(tab, true);
    let script = format!(
        r#"(() => {{
        const target = document.elementFromPoint({x}, {y})
            ?.closest('input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]');
        if (!target) return 'no-field';
        if (target.disabled) return 'disabled';
        if (target.readOnly) return 'readonly';
        if (target.matches('input[type=submit], input[type=button], input[type=file], input[type=reset], input[type=checkbox], input[type=radio]')) return 'not-editable';
        target.focus();
        if (target instanceof HTMLSelectElement) {{
            target.value = {text};
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'select';
        }}
        const isContentEditable = target.isContentEditable || target.getAttribute('contenteditable') === 'true' || target.getAttribute('role') === 'textbox';
        if (isContentEditable) {{
            const range = document.createRange();
            range.selectNodeContents(target);
            if (!{replace}) range.collapse(false);
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            // Do not inject via execCommand or synthetic InputEvents here: they
            // are untrusted, so Slate.js/Draft.js never update their model and
            // a later Enter cannot dispatch. Insertion uses CDP Input.insertText.
            return 'contenteditable';
        }}
        return 'editable';
    }})()"#,
        x = x,
        y = y,
        text = serde_json::json!(text),
        replace = replace
    );
    let result = evaluate_action(app, tab, &script)?;
    match result.as_str() {
        "select" | "ok" => return Ok(()),
        "contenteditable" => {
            if replace && text.is_empty() {
                press_key(app, tab, "Backspace", &gpui_kit::Modifiers::default());
                return Ok(());
            }
            app.engine
                .input_type(tab, text)
                .recv_blocking()
                .ok()
                .and_then(Result::ok)
                .ok_or("typing failed. use: see")?;
            return Ok(());
        }
        "editable" => {}
        reason => return Err(field_unavailable_error(reason, label)),
    }
    let control = gpui_kit::Modifiers {
        control: true,
        ..Default::default()
    };
    if replace {
        press_key(app, tab, "a", &control);
        if text.is_empty() {
            press_key(app, tab, "Backspace", &gpui_kit::Modifiers::default());
            return Ok(());
        }
    } else {
        press_key(app, tab, "End", &control);
    }

    if text.contains('\n') || text.len() > 300 {
        let fast_paste_script = format!(
            r#"(function() {{
                const text = {text_json};
                const active = document.activeElement;
                if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {{
                    const start = {replace} ? 0 : (active.selectionStart ?? active.value.length);
                    const end = {replace} ? active.value.length : (active.selectionEnd ?? active.value.length);
                    active.setRangeText(text, start, end, 'end');
                    active.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    active.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return 'ok';
                }}
                let handled = false;
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
                if (!handled) {{
                    try {{
                        document.execCommand('insertText', false, text);
                        handled = true;
                    }} catch (_) {{}}
                }}
                return handled ? 'ok' : 'fallback';
            }})()"#,
            text_json = serde_json::json!(text),
            replace = replace
        );
        if let Ok(res) = evaluate_action(app, tab, &fast_paste_script)
            && res == "ok"
        {
            return Ok(());
        }
    }

    app.engine
        .input_type(tab, text)
        .recv_blocking()
        .ok()
        .and_then(Result::ok)
        .ok_or("typing failed. use: see")?;
    Ok(())
}

pub fn set_element_value(
    app: &crate::app::Anarc,
    tab_id: &str,
    target: Option<&str>,
    value: &str,
    mode: Option<&str>,
    no_wait: bool,
    wait_ms: Option<u64>,
) -> Result<String, String> {
    app.engine.set_focus(tab_id, true);
    let mode_str = mode.unwrap_or("set");
    let target_str = normalize_ref_token(target.unwrap_or("").trim());

    let mut ref_coords = None;
    if !target_str.is_empty() {
        let state = app.engine.agent_state();
        ref_coords = resolve_target_coords(app, target_str, tab_id, &state).ok();
    }

    let script = format!(
        r#"(() => {{
            const val = {val_json};
            const mode = {mode_json};
            const targetRef = {target_json};
            let el = null;

            if (targetRef) {{
                try {{
                    el = document.querySelector(targetRef);
                }} catch (_) {{}}
                if (!el && {has_coords}) {{
                    el = document.elementFromPoint({cx}, {cy});
                }}
                if (!el) {{
                    el = Array.from(document.querySelectorAll('input, textarea, [contenteditable]:not([contenteditable=false]), [role=textbox]'))
                        .find(n => (n.getAttribute('aria-label') === targetRef || n.placeholder === targetRef || n.id === targetRef || n.name === targetRef));
                }}
            }}

            if (!el) {{
                el = document.activeElement;
            }}

            if (!el || el === document.body || el === document.documentElement) {{
                el = document.querySelector('textarea, [contenteditable]:not([contenteditable=false]), [role=textbox], input:not([type=hidden]):not([type=checkbox]):not([type=radio])');
            }}

            if (!el) return JSON.stringify({{ ok: false, error: 'no editable element found' }});

            const targetEditable = el.closest('input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]') || el;
            targetEditable.focus();

            const isSelect = targetEditable instanceof HTMLSelectElement;
            const isStandardInput = targetEditable.tagName === 'INPUT' || targetEditable.tagName === 'TEXTAREA';
            const isContentEditable = targetEditable.isContentEditable || targetEditable.getAttribute('contenteditable') === 'true' || targetEditable.getAttribute('role') === 'textbox';

            if (mode === 'paste') {{
                let pasted = false;
                try {{
                    const evt = new ClipboardEvent('paste', {{
                        clipboardData: new DataTransfer(),
                        bubbles: true,
                        cancelable: true
                    }});
                    evt.clipboardData.setData('text/plain', val);
                    pasted = !targetEditable.dispatchEvent(evt);
                }} catch (_) {{}}
                if (pasted || (!isSelect && !isStandardInput && !isContentEditable)) {{
                    return JSON.stringify({{ ok: true, tag: targetEditable.tagName || 'element', chars: val.length }});
                }}
            }}

            if (isSelect) {{
                targetEditable.value = val;
                targetEditable.dispatchEvent(new Event('input', {{ bubbles: true }}));
                targetEditable.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return JSON.stringify({{ ok: true, tag: targetEditable.tagName, chars: val.length }});
            }}

            if (isStandardInput) {{
                if (mode === 'append') {{
                    const start = targetEditable.value.length;
                    targetEditable.setRangeText(val, start, start, 'end');
                }} else {{
                    const proto = targetEditable.tagName === 'INPUT' ? window.HTMLInputElement.prototype : window.HTMLTextAreaElement.prototype;
                    const descriptor = Object.getOwnPropertyDescriptor(proto, 'value');
                    if (descriptor && descriptor.set) {{
                        descriptor.set.call(targetEditable, val);
                    }} else {{
                        targetEditable.value = val;
                    }}
                }}
                targetEditable.dispatchEvent(new Event('beforeinput', {{ bubbles: true, inputType: mode === 'append' ? 'insertText' : 'insertFromPaste' }}));
                targetEditable.dispatchEvent(new Event('input', {{ bubbles: true }}));
                targetEditable.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return JSON.stringify({{ ok: true, tag: targetEditable.tagName, chars: val.length }});
            }}

            if (isContentEditable) {{
                const range = document.createRange();
                range.selectNodeContents(targetEditable);
                if (mode === 'append') {{
                    range.collapse(false);
                }}
                const sel = window.getSelection();
                sel.removeAllRanges();
                sel.addRange(range);

                let inserted = false;
                try {{
                    const evt = new ClipboardEvent('paste', {{
                        clipboardData: new DataTransfer(),
                        bubbles: true,
                        cancelable: true
                    }});
                    evt.clipboardData.setData('text/plain', val);
                    inserted = !targetEditable.dispatchEvent(evt);
                }} catch (_) {{}}

                if (!inserted) {{
                    try {{
                        inserted = document.execCommand('insertText', false, val);
                    }} catch (_) {{}}
                }}

                if (!inserted) {{
                    if (mode === 'append') {{
                        targetEditable.innerText += val;
                    }} else {{
                        targetEditable.innerText = val;
                    }}
                }}

                targetEditable.dispatchEvent(new Event('input', {{ bubbles: true }}));
                targetEditable.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return JSON.stringify({{ ok: true, tag: 'contenteditable', chars: val.length }});
            }}

            return JSON.stringify({{ ok: false, error: 'target is not editable' }});
        }})()"#,
        val_json = serde_json::json!(value),
        mode_json = serde_json::json!(mode_str),
        target_json = serde_json::json!(target_str),
        has_coords = ref_coords.is_some(),
        cx = ref_coords.map(|(x, _)| x).unwrap_or(0),
        cy = ref_coords.map(|(_, y)| y).unwrap_or(0),
    );

    if no_wait || wait_ms == Some(0) {
        app.engine.execute_script(tab_id, &script);
        crate::agent::playbook::record_action(&format!("set_value {target_str}"));
        return Ok(format!("set value ({} chars, no_wait)", value.len()));
    }

    let res_str = if let Some(ms) = wait_ms {
        let eval_rx = app.engine.evaluate(tab_id, &script);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        loop {
            match eval_rx.try_recv() {
                Ok(Ok(s)) => break s,
                Ok(Err(_)) | Err(async_channel::TryRecvError::Closed) => {
                    return Err("page unavailable. use: see".into());
                }
                Err(async_channel::TryRecvError::Empty) => {
                    if std::time::Instant::now() >= deadline {
                        crate::agent::playbook::record_action(&format!("set_value {target_str}"));
                        return Ok(format!(
                            "set value ({} chars, wait_ms {ms} elapsed)",
                            value.len()
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    } else {
        evaluate_action(app, tab_id, &script)?
    };
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&res_str) {
        if parsed.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            let chars = parsed
                .get("chars")
                .and_then(|v| v.as_u64())
                .unwrap_or(value.len() as u64);
            let tag = parsed
                .get("tag")
                .and_then(|v| v.as_str())
                .unwrap_or("element");
            crate::agent::playbook::record_action(&format!("set_value {target_str}"));
            return Ok(format!("set {tag} value ({chars} chars)"));
        } else if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
            note_error(ErrorCode::Other);
            return Err(format!("set_value failed: {err}"));
        }
    }
    crate::agent::playbook::record_action(&format!("set_value {target_str}"));
    Ok(format!("set value ({} chars)", value.len()))
}

pub(super) fn evaluate_action(app: &crate::app::Anarc, tab: &str, script: &str) -> Result<String, String> {
    app.engine
        .evaluate(tab, script)
        .recv_blocking()
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| "page unavailable. use: see".into())
}

pub(super) fn submit(
    app: &crate::app::Anarc,
    tab: &str,
    policy: &PolicyConfig,
) -> Result<&'static str, String> {
    let focus = r#"(() => {
        const selector = 'input:not([type=hidden]):not([type=submit]):not([type=button]), textarea, [contenteditable]:not([contenteditable=false]), [role=textbox]';
        const active = document.activeElement;
        const field = active?.closest(selector) || Array.from(document.querySelectorAll('form'))
            .flatMap(form => Array.from(form.querySelectorAll(selector)))
            .find(el => el.getClientRects().length && !el.disabled);
        if (field) field.focus();
        return field ? 'field' : 'none';
    })()"#;
    // Ignore a line break in a contenteditable when detecting a submitted reply.
    let fingerprint = r#"(() => {
        const active = document.activeElement;
        const text = (document.querySelector('main,[role=main]') || document.body).innerText;
        const editor = active?.isContentEditable ? active.innerText.trim() : '';
        const reply = (editor ? text.replace(editor, '') : text).replace(/\s+/g, ' ').trim();
        return JSON.stringify([location.href, reply, active?.value ?? editor]);
    })()"#;
    if evaluate_action(app, tab, focus)? == "field" {
        let before = evaluate_action(app, tab, fingerprint)?;
        check_action_policy(app, tab, "document.activeElement", policy)?;
        press_key(app, tab, "Enter", &gpui_kit::Modifiers::default());
        std::thread::sleep(std::time::Duration::from_millis(200));
        match evaluate_action(app, tab, fingerprint) {
            Ok(after) if before == after => {}
            // Navigation can close the old execution context after Enter.
            _ => return Ok("submit via Enter"),
        }
    }

    let button = r#"(() => {
        const field = document.activeElement;
        const form = field?.form || field?.closest('form');
        const rect = field?.getBoundingClientRect();
        const candidates = Array.from(document.querySelectorAll('button, [role=button], input[type=submit], input[type=button]'))
            .filter(el => /\b(send|submit|post|go)\b/i.test(
                el.getAttribute('aria-label') || el.innerText || el.value || el.title || ''
            ) && !el.disabled && el.getAttribute('aria-disabled') !== 'true' && el.checkVisibility());
        const score = el => {
            const r = el.getBoundingClientRect();
            return (form && (el.form === form || form.contains(el)) ? 0 : 1e6)
                + (rect ? Math.hypot(r.x - rect.x, r.y - rect.y) : 0);
        };
        const target = candidates.sort((a, b) => score(a) - score(b)).at(0);
        if (!target) return null;
        target.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        const r = target.getBoundingClientRect();
        const x = Math.round(r.x + r.width / 2), y = Math.round(r.y + r.height / 2);
        return target.contains(document.elementFromPoint(x, y)) ? [x, y] : null;
    })()"#;
    let coords = evaluate_action(app, tab, button)?;
    let (x, y) = serde_json::from_str::<(i32, i32)>(&coords)
        .map_err(|_| "submit button unavailable. use: see")?;
    click_at(app, tab, x, y, policy)?;
    Ok("submit via button")
}

fn resolve_ref_coords(
    ref_id: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
) -> Result<(i32, i32), String> {
    let guard = state_lock
        .lock()
        .map_err(|_| {
            note_error(ErrorCode::Other);
            "state unavailable. use: status"
        })?;
    let registry = guard
        .registries
        .get(tab_id)
        .ok_or_else(|| {
            note_error(ErrorCode::NotFound);
            "index missing. use: see"
        })?;
    let el = registry.resolve_ref_with_fallback(ref_id).ok_or_else(|| {
        note_error(ErrorCode::StaleRef);
        "stale ref. use: see"
    })?;
    Ok(el.center)
}

/// Find a live interactive element by the name stored in the ref index.
/// Walks shadow roots because Discord-style editors often live inside them.
fn live_coords_for_name(app: &crate::app::Anarc, tab_id: &str, name: &str) -> Option<(i32, i32)> {
    if name.is_empty() {
        return None;
    }
    let script = format!(
        r#"(() => {{
            const name = {n};
            const all = [];
            const sel = 'input, textarea, select, button, [contenteditable]:not([contenteditable=false]), [role=textbox], [role=button], [role=option], [role=menuitem], [role=tab], [role=radio], [role=checkbox], [role=switch], [role=listitem], a[href], [aria-label], [title]';
            const walk = root => {{
                for (const el of root.querySelectorAll(sel)) all.push(el);
                for (const host of root.querySelectorAll('*')) if (host.shadowRoot) walk(host.shadowRoot);
            }};
            walk(document);
            const labelOf = el => (el.getAttribute('aria-label') || el.getAttribute('title') || el.placeholder || el.value || el.innerText || '').trim().toLowerCase();
            const isVis = el => {{
                const r = el.getBoundingClientRect();
                return r.width > 0 && r.height > 0;
            }};
            const inDialog = el => Boolean(el.closest && el.closest('dialog, [role=dialog], [role=alertdialog], [role=menu], [role=listbox], [aria-modal=true], .modal, .popover'));
            const matches = all.filter(el => isVis(el) && labelOf(el).includes(name));
            matches.sort((a, b) => {{
                const aExact = labelOf(a) === name ? 1 : 0;
                const bExact = labelOf(b) === name ? 1 : 0;
                if (aExact !== bExact) return bExact - aExact;
                const aDlg = inDialog(a) ? 1 : 0;
                const bDlg = inDialog(b) ? 1 : 0;
                return bDlg - aDlg;
            }});
            const el = matches.at(0);
            if (!el) return null;
            el.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
            el.focus?.();
            const r = el.getBoundingClientRect();
            return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
        }})()"#,
        n = serde_json::json!(name)
    );
    evaluate_action(app, tab_id, &script).ok().and_then(|json| {
        serde_json::from_str::<Option<(i32, i32)>>(&json)
            .ok()
            .flatten()
    })
}

/// Cached ref centers go stale the moment an SPA re-renders. Check that the
/// point still sits on an interactive element before trusting it.
fn coords_still_valid(app: &crate::app::Anarc, tab_id: &str, (x, y): (i32, i32)) -> bool {
    let script = format!(
        r#"(() => {{
            const el = document.elementFromPoint({x}, {y});
            return !!(el && el.closest('input, textarea, select, button, [contenteditable]:not([contenteditable=false]), [role=textbox], a[href]'));
        }})()"#,
        x = x,
        y = y
    );
    matches!(evaluate_action(app, tab_id, &script).as_deref(), Ok("true"))
}

pub(super) fn resolve_target_coords(
    app: &crate::app::Anarc,
    target: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
) -> Result<(i32, i32), String> {
    let target = normalize_ref_token(target.trim());
    if let Some(css) = target
        .strip_prefix("css=")
        .or_else(|| target.strip_prefix("css:"))
    {
        let selector = unquote(css);
        let script = format!(
            r#"(() => {{
                const el = document.querySelector({sel});
                if (!el) return null;
                el.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
                const r = el.getBoundingClientRect();
                return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
            }})()"#,
            sel = serde_json::json!(selector)
        );
        let coords_res = evaluate_action(app, tab_id, &script)?;
        let coords: Option<(i32, i32)> = serde_json::from_str(&coords_res).ok().flatten();
        return coords
            .ok_or_else(|| format!("element not found for css selector: {selector}. use: see"));
    }
    if let Some(aria) = target
        .strip_prefix("aria=")
        .or_else(|| target.strip_prefix("aria:"))
    {
        let label = unquote(aria);
        let script = format!(
            r#"(() => {{
                const label = {lbl};
                const lower = label.toLowerCase();
                const candidates = Array.from(document.querySelectorAll('*')).filter(el => {{
                    const aria = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || '').toLowerCase();
                    return aria.includes(lower) || (el.innerText && el.innerText.trim().toLowerCase() === lower);
                }});
                const el = candidates.at(0);
                if (!el) return null;
                el.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
                const r = el.getBoundingClientRect();
                return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
            }})()"#,
            lbl = serde_json::json!(label)
        );
        let coords_res = evaluate_action(app, tab_id, &script)?;
        let coords: Option<(i32, i32)> = serde_json::from_str(&coords_res).ok().flatten();
        return coords.ok_or_else(|| format!("element not found for aria: {label}. use: see"));
    }

    if !is_ref_id(target) {
        let lower = target.to_lowercase();
        if let Ok(guard) = state_lock.lock()
            && let Some(registry) = guard.registries.get(tab_id)
        {
            // Prefer exact match and dialog elements first, then substring match.
            let mut best: Option<&super::super::refs::ElementRef> = None;
            let mut best_score = 0u8;
            for el in registry.current_elements.values() {
                let el_lower = el.name.to_lowercase();
                let match_score = if el_lower == lower {
                    2
                } else if el_lower.contains(&lower) {
                    1
                } else {
                    0
                };
                if match_score == 0 {
                    continue;
                }
                let score = match_score * 2 + u8::from(el.is_dialog);
                if score > best_score {
                    best = Some(el);
                    best_score = score;
                }
            }
            if let Some(el) = best
                && coords_still_valid(app, tab_id, el.center)
            {
                return Ok(el.center);
            }
        }
        if let Some(coords) = live_coords_for_name(app, tab_id, &lower) {
            return Ok(coords);
        }
        note_error(ErrorCode::NotFound);
        return Err(format!("element not found for text: {target}. use: find"));
    }

    let cached = resolve_ref_coords(target, tab_id, state_lock);
    if let Ok(coords) = cached
        && coords_still_valid(app, tab_id, coords)
    {
        return Ok(coords);
    }

    // The cached point moved or vanished. Re-resolve against the live DOM
    // instead of failing the rest of the batch with "field unavailable".
    let name = state_lock.lock().ok().and_then(|guard| {
        let registry = guard.registries.get(tab_id)?;
        let current = registry
            .current_elements
            .get(target)
            .map(|el| el.name.clone());
        let previous = registry
            .previous_elements
            .get(target)
            .map(|el| el.name.clone());
        current.filter(|name| !name.is_empty()).or(previous)
    });
    if let Some(name) = name
        && let Some(coords) = live_coords_for_name(app, tab_id, &name.to_lowercase())
    {
        return Ok(coords);
    }
    cached
}

pub(super) fn resolve_semantic_coords(
    kind: char,
    label: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
) -> Result<(i32, i32, String), String> {
    let guard = state_lock
        .lock()
        .map_err(|_| {
            note_error(ErrorCode::Other);
            "state unavailable. use: status"
        })?;
    let registry = guard.registries.get(tab_id).ok_or_else(|| {
        note_error(ErrorCode::NotFound);
        "index missing. use: see"
    })?;

    let label_lower = label.to_lowercase();
    for el in registry.current_elements.values() {
        if el.kind == kind && el.name.to_lowercase().contains(&label_lower) {
            return Ok((el.center.0, el.center.1, el.id.clone()));
        }
    }
    for el in registry.previous_elements.values() {
        if el.kind == kind
            && el.name.to_lowercase().contains(&label_lower)
            && let Some(cur) = registry.resolve_ref_with_fallback(&el.id)
        {
            return Ok((cur.center.0, cur.center.1, cur.id.clone()));
        }
    }
    note_error(ErrorCode::StaleRef);
    Err("ref missing. use: see".into())
}

/// A single focus/typing action can re-index the whole page, producing a diff
/// with over a hundred removed refs. Cap it so `do` replies stay token-dense.
pub(super) fn field_unavailable_error(reason: &str, label: &str) -> String {
    let suffix = if label.is_empty() {
        String::new()
    } else {
        format!(": {label}")
    };
    match reason {
        "no-field" => format!("input field not focused or off-screen{suffix}. use: see"),
        "disabled" => format!("input field disabled{suffix}. use: see"),
        "readonly" => format!("input field read-only{suffix}. use: see"),
        "not-editable" => format!("element not editable{suffix}. use: see"),
        other => format!("field unavailable{suffix}: {other}. use: see"),
    }
}

/// Accessible name recorded for a ref, used for occlusion identity and fill diagnostics.
pub(super) fn ref_name(
    tab_id: &str,
    ref_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
) -> String {
    let Ok(guard) = state_lock.lock() else {
        return String::new();
    };
    let Some(registry) = guard.registries.get(tab_id) else {
        return String::new();
    };
    registry
        .current_elements
        .get(ref_id)
        .or_else(|| registry.previous_elements.get(ref_id))
        .map(|element| element.name.clone())
        .unwrap_or_default()
}

/// Scroll a field into view and give it DOM focus before input events fire.
/// Re-resolves by accessible name so a generation-bumped ref still finds its element.
pub(super) fn focus_field(
    app: &crate::app::Anarc,
    tab_id: &str,
    x: i32,
    y: i32,
    name: &str,
) -> Option<(i32, i32)> {
    let (px, py) = if name.is_empty() {
        (x, y)
    } else {
        live_coords_for_name(app, tab_id, &name.to_lowercase()).unwrap_or((x, y))
    };
    let script = format!(
        r#"(() => {{
            const el = document.elementFromPoint({px}, {py});
            const field = (el && el.closest)
                ? el.closest('input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]')
                : null;
            if (!field) return null;
            field.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
            field.focus?.();
            const r = field.getBoundingClientRect();
            return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
        }})()"#
    );
    evaluate_action(app, tab_id, &script).ok().and_then(|json| {
        serde_json::from_str::<Option<(i32, i32)>>(&json)
            .ok()
            .flatten()
    })
}

/// Dispatch a direct DOM click. SPAs with delegated synthetic handlers can
/// swallow CDP mouse events even when the element is visually at the right point.
pub(super) fn dom_click(
    app: &crate::app::Anarc,
    tab: &str,
    x: i32,
    y: i32,
    policy: &PolicyConfig,
) -> Result<(), String> {
    check_click_policy(app, tab, x, y, policy)?;
    let script = format!(
        r#"(function() {{
            const el = document.elementFromPoint({x}, {y});
            if (!el) return 'none';
            const target = el.closest('a[href], button, [role=button], [role=link], [role=menuitem], input[type=submit], input[type=button]') || el;
            target.focus?.();
            target.click?.();
            return 'ok';
        }})()"#
    );
    match evaluate_action(app, tab, &script)?.as_str() {
        "ok" => Ok(()),
        "none" => Err("no clickable element at coordinates. use: see".into()),
        _ => Err("DOM click failed. use: see".into()),
    }
}

pub(super) fn check_occlusion(
    app: &crate::app::Anarc,
    tab_id: &str,
    x: i32,
    y: i32,
    ref_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::super::plugin::AgentState>>,
) -> Result<(), String> {
    let norm = normalize_ref_token(ref_id.trim());
    let mut expected = ref_name(tab_id, norm, state_lock).to_lowercase();
    if expected.is_empty()
        && !is_ref_id(norm)
        && !norm.starts_with("css=")
        && !norm.starts_with("css:")
        && !norm.starts_with("aria=")
        && !norm.starts_with("aria:")
    {
        expected = norm.to_lowercase();
    }
    let script = format!(
        r#"(function() {{
            const el = document.elementFromPoint({x}, {y});
            if (!el) return 'ok';
            const hit = el.closest('a[href], button, [role=button], [role=link], [role=menuitem], [role=option], input, textarea, [contenteditable]') || el;
            const actual = ((hit.getAttribute('aria-label') || hit.getAttribute('title') || hit.innerText || hit.value || '') + '').trim().toLowerCase();
            const wanted = {expected};
            if (wanted && actual && (actual.includes(wanted) || wanted.includes(actual))) return 'ok';
            if (el.closest('.cookie-banner, #cookie-banner, .toast, [data-testid="cookie-banner"]')) return '!banner';
            let overlay = false;
            for (let p = el; p && p !== document.body; p = p.parentElement) {{
                const style = getComputedStyle(p);
                if ((style.position === 'sticky' || style.position === 'fixed')
                    && (style.position === 'fixed' || parseInt(style.zIndex || '0', 10) > 0)) {{
                    overlay = true;
                    break;
                }}
            }}
            if (!overlay) return 'ok';
            if (!actual) return 'ok';
            if (wanted && (actual.includes(wanted) || wanted.includes(actual))) return 'ok';
            return '!overlay:' + actual;
        }})()"#,
        expected = serde_json::json!(expected)
    );
    let eval_rx = app.engine.evaluate(tab_id, &script);
    if let Ok(Ok(res)) = futures_lite::future::block_on(async { eval_rx.recv().await }) {
        if res == "!banner" {
            return Err("ref covered by banner. use: see".into());
        }
        if let Some(label) = res.strip_prefix("!overlay:") {
            return Err(format!(
                "ref covered by sticky/fixed overlay \"{label}\". use: see"
            ));
        }
    }
    Ok(())
}

pub(super) fn parse_chord_modifiers<'a>(chord: &'a str, mods: &mut gpui_kit::Modifiers) -> &'a str {
    let chord_trimmed = chord.trim();
    let lower = chord_trimmed.to_lowercase();
    if lower.contains("ctrl+")
        || lower.contains("control+")
        || lower.contains("ctrl-")
        || lower.contains("control-")
    {
        mods.control = true;
    }
    if lower.contains("alt+") || lower.contains("alt-") {
        mods.alt = true;
    }
    if lower.contains("shift+") || lower.contains("shift-") {
        mods.shift = true;
    }
    if lower.contains("cmd+")
        || lower.contains("meta+")
        || lower.contains("cmd-")
        || lower.contains("meta-")
        || lower.contains("super+")
    {
        mods.platform = true;
    }

    if let Some(pos) = chord_trimmed.rfind(['+', '-']) {
        chord_trimmed
            .get(pos.saturating_add(1)..)
            .unwrap_or(chord_trimmed)
            .trim()
    } else {
        chord_trimmed
    }
}

