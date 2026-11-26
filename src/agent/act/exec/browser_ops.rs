//! Browser-level DSL operations: navigation, tab, space, split, and wait.

use anarc_telemetry::schema::ErrorCode;

use super::super::dom::{evaluate_action, resolve_tab_target};
use super::super::note_error;
use super::super::types::{SplitAction, TabAction, WaitCondition};
use crate::agent::policy::{NavigationCheck, PolicyConfig, check_navigation_policy};

fn open_tab_with_url(app: &mut crate::app::Anarc, url: &str) -> String {
    app.add_tab();
    let id = app.active_tab_id().to_owned();
    app.engine.navigate(&id, url);
    if let Some(tab) = app.active_space_mut().tabs.iter_mut().find(|t| t.id == id) {
        tab.url = url.to_owned();
    }
    app.save_state();
    id
}

pub(super) fn execute_go(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    url: &str,
    policy_config: &PolicyConfig,
) -> Result<(), String> {
    let cur_url = app
        .spaces()
        .iter()
        .flat_map(|s| &s.tabs)
        .find(|t| t.id == tab_id)
        .map(|t| t.url.as_str())
        .unwrap_or("");
    match check_navigation_policy(cur_url, url, policy_config.navigation_mode) {
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

pub(super) fn execute_tab_action(
    app: &mut crate::app::Anarc,
    action: &TabAction,
) -> Result<(), String> {
    match action {
        TabAction::New { url } => {
            if let Some(url) = url {
                open_tab_with_url(app, url);
            } else {
                app.add_tab();
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
            // Wait for the execution context AND a hydrated DOM. A successful
            // eval alone can still report "loading", which caused `see` to
            // index about:blank on freshly woken tabs.
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

pub(super) fn execute_space_action(
    app: &mut crate::app::Anarc,
    target: &str,
) -> Result<(), String> {
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

pub(super) fn execute_split_action(
    app: &mut crate::app::Anarc,
    action: &SplitAction,
) -> Result<(), String> {
    match action {
        SplitAction::WithNext => {
            app.split_active_with_next();
        }
        SplitAction::Exit => {
            app.exit_split();
        }
        SplitAction::WithUrl(url) => {
            let first = app.active_tab_id().to_owned();
            let second = open_tab_with_url(app, url);
            if !first.is_empty() && first != second {
                let _ = app.split_tabs(&first, &second);
            }
        }
        SplitAction::Pair(a, b) => {
            let first = resolve_tab_target(app, Some(a))?;
            let second = if b.starts_with("http://") || b.starts_with("https://") {
                open_tab_with_url(app, b)
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

pub(super) fn execute_wait_action(
    app: &mut crate::app::Anarc,
    tab_id: &str,
    condition: &WaitCondition,
) -> Result<(), String> {
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
