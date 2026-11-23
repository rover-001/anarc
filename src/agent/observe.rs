use super::refs::{ElementRef, RefRegistry, SnapshotHeader};

pub const INDEXER_JS: &str = include_str!("indexer.js");
pub const MAX_SEE_ELEMENTS: usize = 70;

pub fn format_see_output(
    header: &SnapshotHeader,
    elements: &[ElementRef],
    tabs: Option<&str>,
) -> String {
    format_see_output_paged(header, elements, tabs, 0).0
}

/// Format a page of elements starting at `start`. Returns the rendered text,
/// the index of the first element NOT consumed (the next page cursor), and
/// whether the page reached the end of the index. The cursor lets `see +`
/// advance instead of re-rendering page 1 forever.
pub fn format_see_output_paged(
    header: &SnapshotHeader,
    elements: &[ElementRef],
    tabs: Option<&str>,
    start: usize,
) -> (String, usize, bool) {
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
    let mut i = start.min(normals.len());
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

    (lines.join("\n"), i, i >= normals.len())
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

    scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));

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

/// Read a resolved ref's element text; fall back to whole-page extraction when
/// the DOM path no longer resolves (e.g. a SPA swapped the view).
pub fn build_read_ref_call_script(path: &str, mode: &str, max: usize, x: i32, y: i32) -> String {
    format!(
        "{INDEXER_JS} (function() {{ \
         var text = window.__anarc_read_ref({}, {max}, {x}, {y}); \
         return text || window.__anarc_read({}, {max}); }})();",
        serde_json::json!(path),
        serde_json::json!(mode)
    )
}

pub fn build_stable_call_script(ms: u64) -> String {
    format!("{} window.__anarc_wait_stable({ms});", build_indexer_injection_script())
}

pub fn build_stream_call_script(quiescent_ms: u64, max_timeout_ms: u64) -> String {
    format!(
        "{} window.__anarc_wait_stream({quiescent_ms}, {max_timeout_ms});",
        build_indexer_injection_script()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::refs::{RawElement, RawIndex};
    use std::process::Command;

    // Exercises the in-page extraction script with a minimal DOM stub. Skips
    // when Node is unavailable so the Rust suite stays self-contained.
    #[test]
    fn test_indexer_read_extracts_chat_and_preserves_articles() {
        if Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let harness = r#"
function FakeNode(opts) {
  this.nodeType = 1;
  this.tagName = opts.tag || 'DIV';
  this.innerText = opts.text || '';
  this.isContentEditable = !!opts.editable;
  this.parentElement = null;
  this.children = [];
  this.attrs = opts.attrs || {};
  this._matches = opts.matches || {};
  var cls = opts.cls || [];
  var self = this;
  this.classList = { length: cls.length, contains: function(c) { return cls.indexOf(c) >= 0; } };
  this.rect = opts.rect || { left: 0, top: 0, width: 100, height: 20, right: 100, bottom: 20 };
}
FakeNode.prototype.getBoundingClientRect = function() { return this.rect; };
FakeNode.prototype.getAttribute = function(k) { return this.attrs[k] === undefined ? null : this.attrs[k]; };
FakeNode.prototype.hasAttribute = function(k) { return this.attrs[k] !== undefined; };
FakeNode.prototype.closest = function() { return null; };
FakeNode.prototype.contains = function(n) {
  if (n === this) return true;
  for (var i = 0; i < this.children.length; i++) if (this.children[i].contains(n)) return true;
  return false;
};
FakeNode.prototype.querySelectorAll = function(sel) { return this._matches[sel] || []; };
FakeNode.prototype.cloneNode = function() {
  return new FakeNode({ tag: this.tagName, text: this.innerText, matches: {} });
};
var composer = new FakeNode({ editable: true, text: '' });
var bubble1 = new FakeNode({ text: 'Hello there' });
var bubble2 = new FakeNode({ text: 'Second message' });
var feed = new FakeNode({ tag: 'ARTICLE', text: 'yapster.club Ankit G. - Y Combinator' });
var scope = new FakeNode({ matches: { '[role="log"]': [bubble1, bubble2] } });
scope.children = [bubble1, bubble2, composer];
composer.parentElement = scope;
bubble1.parentElement = scope;
bubble2.parentElement = scope;
var composerQuery = '[contenteditable="true"], textarea, [role="textbox"]';
var articleQuery = 'article, [data-message-author-role="assistant"], [data-message-author-role="user"]';
var useChat = true;
global.window = {
  getComputedStyle: function() { return { display: 'block', visibility: 'visible', opacity: '1', cursor: 'auto' }; },
  innerHeight: 800, innerWidth: 1200, scrollY: 0
};
global.document = {
  querySelectorAll: function(sel) {
    if (sel === composerQuery) return useChat ? [composer] : [];
    if (sel === articleQuery) return [feed];
    return [];
  },
  querySelector: function() { return null; },
  body: scope
};
"#;
        let checks = r#"
var text = window.__anarc_read('text', 8000);
if (text !== 'Hello there\nSecond message') throw new Error('chat bubbles missed: ' + text);
var last = window.__anarc_read('last', 8000);
if (last !== 'Hello there\nSecond message') throw new Error('last missed chat: ' + last);
var trimmed = window.__anarc_read('text', 5);
if (trimmed.indexOf('trimmed to 5 chars') < 0) throw new Error('max ignored: ' + trimmed);
useChat = false;
var article = window.__anarc_read('text', 8000);
if (article.indexOf('yapster.club') < 0) throw new Error('article fallback broken: ' + article);
"#;
        let script = format!("{harness}\n{INDEXER_JS}\n{checks}");
        let output = match Command::new("node").arg("-e").arg(&script).output() {
            Ok(out) => out,
            Err(_) => return,
        };
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

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
    fn test_see_plus_pagination_advances() {
        let mut elements = Vec::new();
        for i in 0..160 {
            elements.push(ElementRef {
                id: format!("b{i}"),
                kind: 'b',
                name: format!("Distinct item {i}"),
                flags: Vec::new(),
                center: (0, i),
                bounds: (0, i, 10, 10),
                fingerprint: format!("b:item {i}:button.item{i}"),
                is_banner: false,
                is_dialog: false,
            });
        }
        let header = SnapshotHeader {
            generation: 1,
            display_url: "example.com".into(),
            title: "List".into(),
            scroll_remaining: 0,
        };

        let (first, next, exhausted) = format_see_output_paged(&header, &elements, None, 0);
        assert!(!exhausted);
        assert!(next > 0 && next < elements.len());
        assert!(first.contains("Distinct item 0"));
        assert!(first.contains("…+"));

        let (second, second_next, _) = format_see_output_paged(&header, &elements, None, next);
        assert_ne!(first, second, "see + must not repeat page 1");
        assert!(second.contains(&format!("Distinct item {next}")));
        assert!(!second.contains("Distinct item 0"));
        assert!(second_next > next);

        // Start past the end returns an empty page and reports exhaustion so
        // callers can wrap the cursor back to 0.
        let (empty, _, exhausted) =
            format_see_output_paged(&header, &elements, None, elements.len());
        assert!(exhausted);
        assert!(!empty.contains("Distinct item"));
    }

    #[test]
    fn test_read_ref_script_targets_path_and_falls_back() {
        let script = build_read_ref_call_script("div.msg>span", "text", 8000, 42, 99);
        assert!(script.contains("__anarc_read_ref"));
        assert!(script.contains("\"div.msg>span\""));
        assert!(script.contains("8000, 42, 99"));
        assert!(script.contains("__anarc_read(\"text\", 8000)"));
    }

    #[test]
    fn test_execute_find_reflects_reindexed_flags() {
        let make_raw = |disabled: bool| RawIndex {
            title: "Messages".into(),
            url: "https://linkedin.com/messaging".into(),
            scroll_remaining: 0,
            elements: vec![RawElement {
                kind: "b".into(),
                name: "Send".into(),
                flags: if disabled {
                    vec!["disabled".into(), "primary".into()]
                } else {
                    vec!["primary".into()]
                },
                x: 100,
                y: 200,
                box_bounds: None,
                path: "button.send".into(),
                is_banner: false,
                is_dialog: false,
            }],
        };

        let mut reg = RefRegistry::new();
        reg.update_from_raw(make_raw(true));
        assert!(execute_find(&reg, "send").contains("disabled"));

        // `find` re-indexes the live DOM before matching; previous flags must
        // not leak into the result.
        let (_, _, diff, _) = reg.update_from_raw(make_raw(false));
        assert_eq!(diff.changed.len(), 1);
        let fresh = execute_find(&reg, "send");
        assert!(!fresh.contains("disabled"));
        assert!(fresh.contains("primary"));
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
