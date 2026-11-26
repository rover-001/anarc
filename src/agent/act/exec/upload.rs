//! File upload execution via CDP `DOM.setFileInputFiles` and dialog interception.

use std::sync::{Arc, Mutex};

use super::super::dom::{click_at, evaluate_action, resolve_target_coords};
use crate::agent::plugin::AgentState;
use crate::agent::policy::PolicyConfig;

static PENDING_UPLOADS: Mutex<Option<Vec<String>>> = Mutex::new(None);

pub fn set_pending_upload_files(files: Vec<String>) {
    if let Ok(mut guard) = PENDING_UPLOADS.lock() {
        *guard = Some(files);
    }
}

pub fn take_pending_upload_files() -> Option<Vec<String>> {
    PENDING_UPLOADS.lock().ok()?.take()
}

fn validate_upload_paths(file_paths: &[String]) -> Result<Vec<String>, String> {
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
    Ok(valid_paths)
}

fn set_files_via_cdp_object(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    coords: Option<(i32, i32)>,
    valid_paths: &[String],
) {
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
        && let Some(object_id) = parsed.pointer("/result/objectId").and_then(|v| v.as_str())
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
}

fn set_files_in_flattened_dom(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    valid_paths: &[String],
) {
    // Fallback for cross-origin iframes (e.g. Google Picker): pierce all
    // frames via CDP DOM.getFlattenedDocument and set files by backendNodeId.
    let flat_params = serde_json::json!({ "depth": -1, "pierce": true });
    let Ok(Ok(flat_res)) = app
        .engine
        .devtools(
            tab_id,
            "DOM.getFlattenedDocument",
            Some(&flat_params.to_string()),
        )
        .recv_blocking()
    else {
        return;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&flat_res) else {
        return;
    };
    let Some(nodes) = parsed.get("nodes").and_then(|v| v.as_array()) else {
        return;
    };
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
        has_file_type
            .then(|| node.get("backendNodeId").and_then(|v| v.as_i64()))
            .flatten()
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

pub(super) fn execute_upload(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    ref_id: &str,
    file_paths: &[String],
    state_lock: &Arc<Mutex<AgentState>>,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
    let valid_paths = validate_upload_paths(file_paths)?;
    set_pending_upload_files(valid_paths.clone());

    let coords = if ref_id.is_empty() {
        None
    } else {
        resolve_target_coords(app, ref_id, tab_id, state_lock).ok()
    };

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
        set_files_via_cdp_object(app, tab_id, coords, &valid_paths);
    } else {
        set_files_in_flattened_dom(app, tab_id, &valid_paths);
    }

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
