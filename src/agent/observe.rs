use super::refs::{ElementRef, RefRegistry, SnapshotHeader};

pub const INDEXER_JS: &str = include_str!("indexer.js");
pub const MAX_SEE_ELEMENTS: usize = 70;
pub const HARD_TOKEN_BUDGET: usize = 4000;

pub fn format_see_output(
    header: &SnapshotHeader,
    elements: &[ElementRef],
    tabs: Option<&str>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "g{} {} \"{}\" ↓{}",
        header.generation, header.display_url, header.title, header.scroll_remaining
    ));

    if let Some(tabs) = tabs {
        lines.push(format!("tabs: {tabs}"));
    }

    // Split into alerts/dialogs/banners first, then regular items
    let mut alerts = Vec::new();
    let mut normals = Vec::new();

    for el in elements {
        if el.is_dialog {
            alerts.push(format_element_line(el, "!dialog"));
        } else if el.is_banner {
            alerts.push(format_element_line(el, "!banner"));
        } else {
            normals.push(el);
        }
    }

    for alert in alerts {
        lines.push(alert);
    }

    // Collapse repeated structures in normals
    let mut i = 0;
    let mut count = 0;
    while i < normals.len() {
        if count >= MAX_SEE_ELEMENTS {
            let remaining = normals.len().saturating_sub(i);
            lines.push(format!("…+{remaining} more (see +)"));
            break;
        }

        let cur = match normals.get(i) {
            Some(c) => *c,
            None => break,
        };

        // Check if there is a consecutive run of similar items (e.g. links with same kind and empty/similar context)
        let mut run_end = i + 1;
        while run_end < normals.len() {
            let next_el = match normals.get(run_end) {
                Some(n) => *n,
                None => break,
            };
            if next_el.kind == cur.kind && next_el.flags == cur.flags && is_similar_repeated(cur, next_el) {
                run_end += 1;
            } else {
                break;
            }
        }

        let run_len = run_end.saturating_sub(i);
        if run_len >= 5 {
            let last_el = normals.get(run_end.saturating_sub(1)).copied().unwrap_or(cur);
            lines.push(format!("{}-{} {} ×{}", cur.id, last_el.id, cur.name, run_len));
            i = run_end;
        } else {
            lines.push(format_element_line(cur, ""));
            i += 1;
        }
        count += 1;
    }

    lines.join("\n")
}

fn is_similar_repeated(a: &ElementRef, b: &ElementRef) -> bool {
    a.kind == 'l' && b.kind == 'l' && a.bounds.1 == b.bounds.1
}

fn format_element_line(el: &ElementRef, prefix: &str) -> String {
    let flags_str = if el.flags.is_empty() {
        String::new()
    } else {
        format!(" {}", el.flags.join(" "))
    };

    let pfx = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} ")
    };

    format!("{pfx}{} \"{}\"{flags_str}", el.id, el.name)
}

pub fn execute_find(
    registry: &RefRegistry,
    query: &str,
) -> String {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return "find: empty query".to_string();
    }
    let terms: Vec<&str> = q.split_whitespace().collect();

    let mut scored: Vec<(&ElementRef, usize)> = Vec::new();
    for el in registry.current_elements.values() {
        let name_lower = el.name.to_lowercase();
        let id_lower = el.id.to_lowercase();
        let path_lower = el.fingerprint.to_lowercase();

        let mut score = 0;
        for t in &terms {
            if id_lower == *t {
                score += 100;
            }
            if name_lower.contains(t) {
                score += 50;
            }
            for f in &el.flags {
                if f.to_lowercase().contains(t) {
                    score += 20;
                }
            }
            if path_lower.contains(t) {
                score += 10;
            }
        }

        if score > 0 {
            scored.push((el, score));
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));

    if scored.is_empty() {
        return format!("find: no elements match \"{query}\"");
    }

    let top = scored.into_iter().take(5);
    let mut lines = Vec::new();
    for (el, _) in top {
        let flags_str = if el.flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", el.flags.join(", "))
        };
        lines.push(format!("{} \"{}\"{flags_str} · center: ({}, {})", el.id, el.name, el.center.0, el.center.1));
    }
    lines.join("\n")
}

pub fn build_indexer_injection_script() -> &'static str {
    INDEXER_JS
}

pub fn build_see_call_script(scope: &str) -> String {
    format!(
        "if (typeof window.__anarc_index !== 'function') {{ {INDEXER_JS} }} window.__anarc_index({});",
        serde_json::json!(scope)
    )
}

pub fn build_read_call_script(mode: &str, max: usize) -> String {
    format!("{INDEXER_JS} window.__anarc_read({}, {max});", serde_json::json!(mode))
}

pub fn build_stable_call_script(ms: u64) -> String {
    format!("{} window.__anarc_wait_stable({ms});", build_indexer_injection_script())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::refs::{RawElement, RawIndex};

    #[test]
    fn test_format_see_output() {
        let mut reg = RefRegistry::new();
        let raw = RawIndex {
            title: "Sign in to GitHub".into(),
            url: "https://github.com/login".into(),
            scroll_remaining: 0,
            elements: vec![
                RawElement {
                    kind: "i".into(),
                    name: "Username or email".into(),
                    flags: vec![],
                    x: 100,
                    y: 100,
                    box_bounds: Some([50, 80, 150, 30]),
                    path: "input#login".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "i".into(),
                    name: "Password".into(),
                    flags: vec!["secret".into()],
                    x: 100,
                    y: 150,
                    box_bounds: Some([50, 130, 150, 30]),
                    path: "input#password".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "b".into(),
                    name: "Sign in".into(),
                    flags: vec!["primary".into()],
                    x: 100,
                    y: 200,
                    box_bounds: Some([50, 180, 100, 35]),
                    path: "button#submit".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "b".into(),
                    name: "Accept cookies".into(),
                    flags: vec![],
                    x: 50,
                    y: 500,
                    box_bounds: Some([0, 480, 200, 40]),
                    path: "div.cookie-banner>button".into(),
                    is_banner: true,
                    is_dialog: false,
                },
            ],
        };

        let (header, els, _, _) = reg.update_from_raw(raw);
        let output = format_see_output(&header, &els, None);

        // Header check
        assert!(output.starts_with("g1 github.com/login \"Sign in to GitHub\" ↓0"));
        // Banner surfaced with !banner prefix
        assert!(output.contains("!banner b2 \"Accept cookies\""));
        // Input and secret
        assert!(output.contains("i1 \"Username or email\""));
        assert!(output.contains("i2 \"Password\" secret"));
        assert!(output.contains("b1 \"Sign in\" primary"));
    }

    #[test]
    fn test_execute_find() {
        let mut reg = RefRegistry::new();
        let raw = RawIndex {
            title: "Store".into(),
            url: "https://shop.test/".into(),
            scroll_remaining: 100,
            elements: vec![
                RawElement {
                    kind: "b".into(),
                    name: "Add to cart".into(),
                    flags: vec!["primary".into()],
                    x: 20,
                    y: 40,
                    box_bounds: None,
                    path: "button.cart".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "i".into(),
                    name: "Search products".into(),
                    flags: vec![],
                    x: 50,
                    y: 10,
                    box_bounds: None,
                    path: "input.search".into(),
                    is_banner: false,
                    is_dialog: false,
                },
            ],
        };

        reg.update_from_raw(raw);
        let res = execute_find(&reg, "cart");
        assert!(res.contains("b1"));
        assert!(res.contains("Add to cart"));

        let not_found = execute_find(&reg, "nonexistent");
        assert!(not_found.contains("no elements match"));
    }
}
