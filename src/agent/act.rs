use super::observe::{build_see_call_script, build_stable_call_script, format_see_output};
use super::policy::{
    ACTION_CONTEXT, NavigationCheck, PolicyConfig, check_navigation_policy, confirmation_message,
    resolve_secret,
};
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
    New { url: Option<String> },
    Select(String),
    Close { target: Option<String> },
    Pin { target: Option<String> },
    Unpin { target: Option<String> },
    Favorite { target: Option<String> },
    Unfavorite { target: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaitCondition {
    Idle,
    Stable(u64),
    Stream { quiescent_ms: u64, timeout_ms: u64 },
    Millis(u64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionOp {
    Click {
        ref_id: String,
    },
    SemanticClick {
        kind: char,
        label: String,
    },
    ClearAndType {
        ref_id: String,
        text: String,
    },
    AppendText {
        ref_id: String,
        text: String,
    },
    TypeSecret {
        ref_id: String,
        secret_name: String,
    },
    SemanticType {
        kind: char,
        label: String,
        text: String,
    },
    Paste {
        ref_id: Option<String>,
        text: Option<String>,
    },
    Key {
        chord: String,
    },
    KeyDown {
        chord: String,
    },
    KeyUp {
        chord: String,
    },
    Submit,
    Go {
        url: String,
    },
    Back,
    Fwd,
    Reload,
    Scroll {
        direction: ScrollDirection,
    },
    Hover {
        ref_id: String,
    },
    Focus {
        ref_id: String,
    },
    Drag {
        from_ref: String,
        to_ref: String,
    },
    XY {
        x: i32,
        y: i32,
    },
    Upload {
        ref_id: String,
        file_paths: Vec<String>,
    },
    Dialog {
        accept: bool,
    },
    Tab {
        action: TabAction,
    },
    Wait {
        condition: WaitCondition,
    },
}

static PENDING_UPLOADS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

pub fn set_pending_upload_files(files: Vec<String>) {
    if let Ok(mut guard) = PENDING_UPLOADS.lock() {
        *guard = Some(files);
    }
}

pub fn take_pending_upload_files() -> Option<Vec<String>> {
    PENDING_UPLOADS.lock().ok()?.take()
}

fn read_file_arg(file_path: &str) -> Result<String, String> {
    let unquoted = unquote(file_path);
    let p = std::path::Path::new(unquoted.trim());
    std::fs::read_to_string(p).map_err(|e| format!("cannot read file '{file_path}': {e}. use: see"))
}

fn expand_file_arg(arg: &str) -> Vec<String> {
    if arg.contains('*') || arg.contains('?') {
        let p = std::path::Path::new(arg);
        let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
        let file_pattern = p.file_name().and_then(|f| f.to_str()).unwrap_or("*");
        let mut matched = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if matches_simple_glob(&name_str, file_pattern) {
                    matched.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
        matched.sort();
        if !matched.is_empty() {
            return matched;
        }
    }
    vec![arg.to_string()]
}

fn matches_simple_glob(name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    name == pattern
}

fn split_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in input.chars() {
        if ch == '\r' {
            continue;
        }
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            current.push(ch);
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            current.push(ch);
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            current.push(ch);
            continue;
        }

        if (ch == ';' || ch == '\n') && !in_single && !in_double {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
    statements
}

pub fn parse_dsl(input: &str) -> Result<Vec<ActionOp>, String> {
    parse_dsl_at_depth(input, 0)
}

fn parse_dsl_at_depth(input: &str, depth: usize) -> Result<Vec<ActionOp>, String> {
    if depth >= 16 {
        return Err("macro nesting limit. use: see".into());
    }
    let mut ops = Vec::new();
    let statements = split_statements(input);

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
                let raw_args: Vec<String> = parts
                    .get(1..)
                    .unwrap_or(&[])
                    .iter()
                    .map(|a| unquote(a))
                    .collect();
                let playbook = super::playbook::get_or_init_registry();
                let expanded = playbook.expand_macro(macro_name, &raw_args)?;
                let expanded_ops = parse_dsl_at_depth(&expanded, depth + 1)?;
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

    // Key chord: "key Enter", "key ctrl+a", "key \"Ctrl+v\""
    if let Some(stripped) = s
        .strip_prefix("key ")
        .or_else(|| (s == "key").then_some(""))
    {
        let chord = unquote(stripped.trim()).to_string();
        if chord.is_empty() {
            return Err("missing key. use: do \"key Enter\"".into());
        }
        return Ok(ActionOp::Key { chord });
    }

    if let Some(stripped) = s
        .strip_prefix("keydown ")
        .or_else(|| (s == "keydown").then_some(""))
    {
        let chord = unquote(stripped.trim()).to_string();
        if chord.is_empty() {
            return Err("missing key. use: do \"keydown Shift\"".into());
        }
        return Ok(ActionOp::KeyDown { chord });
    }

    if let Some(stripped) = s
        .strip_prefix("keyup ")
        .or_else(|| (s == "keyup").then_some(""))
    {
        let chord = unquote(stripped.trim()).to_string();
        if chord.is_empty() {
            return Err("missing key. use: do \"keyup Shift\"".into());
        }
        return Ok(ActionOp::KeyUp { chord });
    }

    // Go / Navigate url: "go https://...", "navigate https://..."
    if let Some(stripped) = s
        .strip_prefix("navigate ")
        .or_else(|| s.strip_prefix("go "))
        .or_else(|| (s == "go" || s == "navigate").then_some(""))
    {
        let url = unquote(stripped.trim()).to_string();
        if url.is_empty() {
            return Err("missing URL. use: do \"navigate https://example.com\"".into());
        }
        return Ok(ActionOp::Go { url });
    }

    if s == "submit" {
        return Ok(ActionOp::Submit);
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
        return Ok(ActionOp::Drag {
            from_ref: from,
            to_ref: to,
        });
    }

    // XY: "xy 100 200"
    if let Some(stripped) = s.strip_prefix("xy ") {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let x: i32 = parts.get(0).and_then(|v| v.parse().ok()).unwrap_or(0);
        let y: i32 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        return Ok(ActionOp::XY { x, y });
    }

    // Paste: "paste", "paste b1", "paste b1 text", "paste b1 @file:...", "paste @file:..."
    if s == "paste" || s.starts_with("paste ") {
        let rest = s.strip_prefix("paste").unwrap_or("").trim();
        if rest.is_empty() {
            return Ok(ActionOp::Paste {
                ref_id: None,
                text: None,
            });
        }
        if let Some(file_path) = rest.strip_prefix("@file:") {
            let content = read_file_arg(file_path)?;
            return Ok(ActionOp::Paste {
                ref_id: None,
                text: Some(content),
            });
        }
        let parts: Vec<&str> = rest.split_whitespace().collect();
        let first = parts.get(0).copied().unwrap_or("");
        let mut first_chars = first.chars();
        let is_ref = first_chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && !first_chars.as_str().is_empty()
            && first_chars.all(|c| c.is_ascii_digit());

        if is_ref {
            let ref_id = first.to_string();
            let remainder = rest.get(first.len()..).unwrap_or("").trim();
            if remainder.is_empty() {
                return Ok(ActionOp::Paste {
                    ref_id: Some(ref_id),
                    text: None,
                });
            }
            if let Some(file_path) = remainder.strip_prefix("@file:") {
                let content = read_file_arg(file_path)?;
                return Ok(ActionOp::Paste {
                    ref_id: Some(ref_id),
                    text: Some(content),
                });
            }
            return Ok(ActionOp::Paste {
                ref_id: Some(ref_id),
                text: Some(unquote(remainder)),
            });
        } else {
            return Ok(ActionOp::Paste {
                ref_id: None,
                text: Some(unquote(rest)),
            });
        }
    }

    // Upload: "up ref /path/file...", "upload ref /path/file...", "upload /path/file..."
    if let Some(stripped) = s.strip_prefix("upload ").or_else(|| s.strip_prefix("up ")) {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        if parts.is_empty() {
            return Err("missing upload ref and path. use: do \"upload i1 /path/file\"".into());
        }
        let first = parts[0];
        let (r, raw_paths) = if first.starts_with('/')
            || first.starts_with("./")
            || first.starts_with('~')
            || std::path::Path::new(first).exists()
        {
            (String::new(), &parts[..])
        } else {
            (first.to_string(), parts.get(1..).unwrap_or(&[]))
        };
        if raw_paths.is_empty() {
            return Err("missing upload file path. use: do \"upload i1 /path/file\"".into());
        }
        let mut file_paths = Vec::new();
        for &raw_path in raw_paths {
            let unq = unquote(raw_path);
            let expanded = expand_file_arg(&unq);
            file_paths.extend(expanded);
        }
        if file_paths.is_empty() {
            return Err("no matching files found for upload. use: see".into());
        }
        return Ok(ActionOp::Upload {
            ref_id: r,
            file_paths,
        });
    }

    // Dialog: "dialog ok", "dialog cancel"
    if let Some(stripped) = s.strip_prefix("dialog ") {
        let d = stripped.trim();
        let accept = d == "ok" || d == "accept" || d == "true";
        return Ok(ActionOp::Dialog { accept });
    }

    // Tab: "tab new", "tab 2", "tab close", "tab pin", "tab fav", "tab select personal-tab-3", etc.
    if let Some(stripped) = s.strip_prefix("tab ") {
        let rest = stripped.trim();
        let action = if rest == "new" {
            TabAction::New { url: None }
        } else if let Some(url) = rest.strip_prefix("new ") {
            TabAction::New {
                url: Some(unquote(url).trim().to_string()),
            }
        } else if rest == "close" || rest == "remove" {
            TabAction::Close { target: None }
        } else if let Some(target) = rest
            .strip_prefix("close ")
            .or_else(|| rest.strip_prefix("remove "))
        {
            TabAction::Close {
                target: Some(target.trim().to_string()),
            }
        } else if rest == "pin" {
            TabAction::Pin { target: None }
        } else if let Some(target) = rest.strip_prefix("pin ") {
            TabAction::Pin {
                target: Some(target.trim().to_string()),
            }
        } else if rest == "unpin" {
            TabAction::Unpin { target: None }
        } else if let Some(target) = rest.strip_prefix("unpin ") {
            TabAction::Unpin {
                target: Some(target.trim().to_string()),
            }
        } else if rest == "fav" || rest == "favorite" {
            TabAction::Favorite { target: None }
        } else if let Some(target) = rest
            .strip_prefix("fav ")
            .or_else(|| rest.strip_prefix("favorite "))
        {
            TabAction::Favorite {
                target: Some(target.trim().to_string()),
            }
        } else if rest == "unfav" || rest == "unfavorite" {
            TabAction::Unfavorite { target: None }
        } else if let Some(target) = rest
            .strip_prefix("unfav ")
            .or_else(|| rest.strip_prefix("unfavorite "))
        {
            TabAction::Unfavorite {
                target: Some(target.trim().to_string()),
            }
        } else if let Some(target) = rest
            .strip_prefix("select ")
            .or_else(|| rest.strip_prefix("switch "))
        {
            TabAction::Select(target.trim().to_string())
        } else if rest.chars().all(|c| c.is_ascii_digit())
            || rest.contains("-tab-")
            || rest.starts_with("tab-")
            || rest.starts_with("personal-")
            || rest.starts_with("work-")
        {
            TabAction::Select(rest.to_string())
        } else {
            return Err("unknown tab action. use: tab pin|fav|close|new|N".into());
        };
        return Ok(ActionOp::Tab { action });
    }

    if let Some(stripped) = s.strip_prefix("pin ") {
        let rest = stripped.trim();
        let target = rest.strip_prefix("tab ").unwrap_or(rest).trim();
        let target = if target.is_empty() {
            None
        } else {
            Some(target.to_string())
        };
        return Ok(ActionOp::Tab {
            action: TabAction::Pin { target },
        });
    }
    if s == "pin" {
        return Ok(ActionOp::Tab {
            action: TabAction::Pin { target: None },
        });
    }
    if let Some(stripped) = s.strip_prefix("unpin ") {
        let rest = stripped.trim();
        let target = rest.strip_prefix("tab ").unwrap_or(rest).trim();
        let target = if target.is_empty() {
            None
        } else {
            Some(target.to_string())
        };
        return Ok(ActionOp::Tab {
            action: TabAction::Unpin { target },
        });
    }
    if s == "unpin" {
        return Ok(ActionOp::Tab {
            action: TabAction::Unpin { target: None },
        });
    }
    if let Some(stripped) = s
        .strip_prefix("fav ")
        .or_else(|| s.strip_prefix("favorite "))
    {
        let rest = stripped.trim();
        let target = rest.strip_prefix("tab ").unwrap_or(rest).trim();
        let target = if target.is_empty() {
            None
        } else {
            Some(target.to_string())
        };
        return Ok(ActionOp::Tab {
            action: TabAction::Favorite { target },
        });
    }
    if s == "fav" || s == "favorite" {
        return Ok(ActionOp::Tab {
            action: TabAction::Favorite { target: None },
        });
    }
    if let Some(stripped) = s
        .strip_prefix("unfav ")
        .or_else(|| s.strip_prefix("unfavorite "))
    {
        let rest = stripped.trim();
        let target = rest.strip_prefix("tab ").unwrap_or(rest).trim();
        let target = if target.is_empty() {
            None
        } else {
            Some(target.to_string())
        };
        return Ok(ActionOp::Tab {
            action: TabAction::Unfavorite { target },
        });
    }
    if s == "unfav" || s == "unfavorite" {
        return Ok(ActionOp::Tab {
            action: TabAction::Unfavorite { target: None },
        });
    }

    // Wait: "wait idle", "wait 500", "wait stream", "wait stream 15000", "wait text-quiescent 1000", "wait \"Text\""
    if let Some(stripped) = s.strip_prefix("wait ") {
        let w = stripped.trim();
        let condition = if w == "idle" {
            WaitCondition::Idle
        } else if w == "stable" {
            WaitCondition::Stable(1500)
        } else if let Some(interval) = w.strip_prefix("stable ") {
            let ms = interval
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|ms| (1..=30000).contains(ms))
                .ok_or("invalid stable wait. use: do \"wait stable 1500\"")?;
            WaitCondition::Stable(ms)
        } else if w == "stream" {
            WaitCondition::Stream {
                quiescent_ms: 1000,
                timeout_ms: 30000,
            }
        } else if let Some(timeout) = w.strip_prefix("stream ") {
            let ms = timeout
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|ms| (1..=60000).contains(ms))
                .ok_or("invalid stream wait. use: do \"wait stream 15000\"")?;
            WaitCondition::Stream {
                quiescent_ms: 1000,
                timeout_ms: ms,
            }
        } else if let Some(quiescent) = w.strip_prefix("text-quiescent ") {
            let parts: Vec<&str> = quiescent.split_whitespace().collect();
            let q_ms = parts
                .get(0)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1000);
            let t_ms = parts
                .get(1)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30000);
            WaitCondition::Stream {
                quiescent_ms: q_ms,
                timeout_ms: t_ms,
            }
        } else if let Ok(ms) = w.parse::<u64>() {
            WaitCondition::Millis(ms)
        } else {
            let unquoted = unquote(w);
            WaitCondition::Text(unquoted)
        };
        return Ok(ActionOp::Wait { condition });
    }

    // Semantic selectors: b:"Sign in" or i:"Email"="alice" or i:"Prompt"=@file:...
    if let Some(colon_idx) = s.find(':') {
        let kind_str = s.get(0..colon_idx).unwrap_or("");
        if kind_str.len() == 1 {
            let kind_char = kind_str.chars().next().unwrap_or('x');
            let rest = s.get(colon_idx.saturating_add(1)..).unwrap_or("");

            if let Some(eq_idx) = rest.find('=') {
                let label_part = rest.get(0..eq_idx).unwrap_or("");
                let text_part = rest.get(eq_idx.saturating_add(1)..).unwrap_or("");
                let text = if let Some(file_path) = text_part.trim().strip_prefix("@file:") {
                    read_file_arg(file_path)?
                } else {
                    unquote(text_part)
                };
                return Ok(ActionOp::SemanticType {
                    kind: kind_char,
                    label: unquote(label_part),
                    text,
                });
            } else {
                return Ok(ActionOp::SemanticClick {
                    kind: kind_char,
                    label: unquote(rest),
                });
            }
        }
    }

    // Click command: "click b1", "click css=...", "click aria=..."
    if let Some(stripped) = s.strip_prefix("click ") {
        let target = unquote(stripped.trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"click b1\"".into());
        }
        return Ok(ActionOp::Click { ref_id: target });
    }

    // Fill command: "fill css=... text", "fill aria=... text", "fill i1 text"
    if let Some(stripped) = s.strip_prefix("fill ") {
        let rest = stripped.trim();
        let (target, text) = split_target_and_text(rest);
        if target.is_empty() {
            return Err("missing fill target. use: do \"fill i1 text\"".into());
        }
        return Ok(ActionOp::ClearAndType {
            ref_id: target,
            text,
        });
    }

    // Type command: "type css=... text", "type aria=... text", "type i1 text", "type append i1 text"
    if let Some(stripped) = s.strip_prefix("type ") {
        let rest = stripped.trim();
        let (is_append, remainder) = if let Some(r) = rest.strip_prefix("append ") {
            (true, r.trim())
        } else {
            (false, rest)
        };
        let (target, mut text) = split_target_and_text(remainder);
        if target.is_empty() {
            return Err("missing type target. use: do \"type i1 text\"".into());
        }
        let append_flag = text.ends_with("append=true") || is_append;
        if text.ends_with("append=true") {
            text = text.trim_end_matches("append=true").trim().to_string();
        }
        if append_flag {
            return Ok(ActionOp::AppendText {
                ref_id: target,
                text,
            });
        } else {
            return Ok(ActionOp::ClearAndType {
                ref_id: target,
                text,
            });
        }
    }

    // Direct CSS or ARIA selector click: "css=button.send", "aria=Send"
    if s.starts_with("css=")
        || s.starts_with("aria=")
        || s.starts_with("css:")
        || s.starts_with("aria:")
    {
        return Ok(ActionOp::Click {
            ref_id: s.to_string(),
        });
    }

    // Assignment: ref=text or ref+text or ref=@secret:name or ref=@file:path
    if let Some(eq_idx) = s.find('=') {
        let ref_id = s.get(0..eq_idx).unwrap_or("").trim().to_string();
        let val_part = s.get(eq_idx.saturating_add(1)..).unwrap_or("").trim();

        if let Some(secret_name) = val_part.strip_prefix("@secret:") {
            return Ok(ActionOp::TypeSecret {
                ref_id,
                secret_name: secret_name.trim().to_string(),
            });
        }
        if let Some(file_path) = val_part.strip_prefix("@file:") {
            let content = read_file_arg(file_path)?;
            return Ok(ActionOp::ClearAndType {
                ref_id,
                text: content,
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

        if let Some(file_path) = val_part.strip_prefix("@file:") {
            let content = read_file_arg(file_path)?;
            return Ok(ActionOp::AppendText {
                ref_id,
                text: content,
            });
        }
        return Ok(ActionOp::AppendText {
            ref_id,
            text: unquote(val_part),
        });
    }

    // Default: simple click on ref
    let mut characters = s.chars();
    if characters
        .next()
        .is_some_and(|kind| kind.is_ascii_lowercase())
        && !characters.as_str().is_empty()
        && characters.all(|character| character.is_ascii_digit())
    {
        return Ok(ActionOp::Click {
            ref_id: s.to_string(),
        });
    }

    Err("unknown action. use: see".into())
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        if t.len() >= 2 {
            t.get(1..t.len().saturating_sub(1))
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        }
    } else {
        t.to_string()
    }
}

fn split_target_and_text(s: &str) -> (String, String) {
    let trimmed = s.trim();
    if trimmed.starts_with("css=\"") || trimmed.starts_with("css='") {
        let quote_char = trimmed.chars().nth(4).unwrap_or('"');
        if let Some(end_quote) = trimmed[5..].find(quote_char) {
            let target = &trimmed[..5 + end_quote + 1];
            let text = trimmed[5 + end_quote + 1..].trim();
            return (target.to_string(), unquote(text));
        }
    } else if trimmed.starts_with("aria=\"") || trimmed.starts_with("aria='") {
        let quote_char = trimmed.chars().nth(5).unwrap_or('"');
        if let Some(end_quote) = trimmed[6..].find(quote_char) {
            let target = &trimmed[..6 + end_quote + 1];
            let text = trimmed[6 + end_quote + 1..].trim();
            return (target.to_string(), unquote(text));
        }
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let target = parts.get(0).copied().unwrap_or("").to_string();
    let text_start = target.len();
    let text = trimmed.get(text_start..).unwrap_or("").trim();
    (target, unquote(text))
}

#[derive(Clone, Copy, Debug)]
enum WaitMode {
    Stable(u64),
    Stream { quiescent_ms: u64, timeout_ms: u64 },
}

pub fn execute_dsl(
    app: &mut crate::app::Lumen,
    tab_id: &str,
    ops: &[ActionOp],
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
        if matches!(op, ActionOp::Tab { .. }) {
            target = app.active_tab_id().to_owned();
        }
    }
    let count = ops.len();
    let summary = if submissions.is_empty() {
        format!("ok {count}")
    } else {
        format!("ok {count} · {}", submissions.join(" · "))
    };
    let tabs = super::plugin::compact_tabs(app);
    let script = match wait_mode {
        Some(WaitMode::Stable(ms)) => build_stable_call_script(ms),
        Some(WaitMode::Stream {
            quiescent_ms,
            timeout_ms,
        }) => super::observe::build_stream_call_script(quiescent_ms, timeout_ms),
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
                    let _ = response.try_send(AgentResponse::err("wait interrupted. use: see"));
                    return;
                }
            }
            let raw = result.and_then(|json| serde_json::from_str::<RawIndex>(&json).ok());
            let reply = if let (Some(raw), Ok(mut state)) = (raw, agent_state.lock()) {
                let registry = state.registries.entry(target).or_default();
                let (header, elements, diff, gen_changed) = registry.update_from_raw(raw);
                let mut reply = format!("{summary} · g{}", header.generation);
                if gen_changed {
                    reply.push('\n');
                    reply.push_str(&format_see_output(&header, &elements, Some(&tabs)));
                } else if !diff.is_empty() {
                    reply.push('\n');
                    reply.push_str(&diff.format_diff());
                }
                reply
            } else {
                summary
            };
            let _ = response.try_send(AgentResponse::success(reply));
        });
}

fn execute_single_op(
    app: &mut crate::app::Lumen,
    tab_id: &str,
    op: &ActionOp,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
    policy_config: &PolicyConfig,
    submissions: &mut Vec<&'static str>,
) -> Result<(), String> {
    match op {
        ActionOp::Click { ref_id } => {
            let (cx, cy) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, ref_id)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            super::playbook::record_action(ref_id);
            Ok(())
        }
        ActionOp::ClearAndType { ref_id, text } | ActionOp::AppendText { ref_id, text } => {
            let (x, y) = resolve_target_coords(app, ref_id, tab_id, state_lock)?;
            check_occlusion(app, tab_id, x, y, ref_id)?;
            let replace = matches!(op, ActionOp::ClearAndType { .. });
            type_at(app, tab_id, x, y, text, replace)?;
            let operator = if replace { "=" } else { "+" };
            super::playbook::record_action(&format!("{ref_id}{operator}\"{text}\""));
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
            check_occlusion(app, tab_id, x, y, ref_id)?;
            type_at(app, tab_id, x, y, &secret, true)?;
            super::playbook::record_action(&format!("{ref_id}=@secret:{secret_name}"));
            Ok(())
        }
        ActionOp::SemanticClick { kind, label } => {
            let (cx, cy, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, cx, cy, &id)?;
            click_at(app, tab_id, cx, cy, policy_config)?;
            super::playbook::record_action(&format!("{kind}:\"{label}\""));
            Ok(())
        }
        ActionOp::SemanticType { kind, label, text } => {
            let (x, y, id) = resolve_semantic_coords(*kind, label, tab_id, state_lock)?;
            check_occlusion(app, tab_id, x, y, &id)?;
            type_at(app, tab_id, x, y, text, true)?;
            super::playbook::record_action(&format!("{kind}:\"{label}\"=\"{text}\""));
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
                    super::playbook::record_action(&format!("key {chord}"));
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
            super::playbook::record_action(&format!("key {chord}"));
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
            super::playbook::record_action(&format!("keydown {chord}"));
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
            super::playbook::record_action(&format!("keyup {chord}"));
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
        ActionOp::Drag { from_ref, to_ref } => {
            let (x1, y1) = resolve_target_coords(app, from_ref, tab_id, state_lock)?;
            let (x2, y2) = resolve_target_coords(app, to_ref, tab_id, state_lock)?;
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
            // Intermediate drag steps
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
            }
            app.engine
                .send_mouse_click(tab_id, x2, y2, cef::MouseButtonType::LEFT, true, 1, &mods);
            Ok(())
        }
        ActionOp::Paste { ref_id, text } => {
            if let Some(r) = ref_id {
                let (cx, cy) = resolve_target_coords(app, r, tab_id, state_lock)?;
                check_occlusion(app, tab_id, cx, cy, r)?;
                click_at(app, tab_id, cx, cy, policy_config)?;
            }
            let paste_content = if let Some(t) = text {
                crate::engine::clipboard::write_system_clipboard(t);
                t.clone()
            } else {
                crate::engine::clipboard::read_system_clipboard().unwrap_or_default()
            };
            app.engine.set_focus(tab_id, true);
            let script = format!(
                r#"(function() {{
                    const text = {text_json};
                    const active = document.activeElement;
                    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {{
                        const start = active.selectionStart ?? active.value.length;
                        const end = active.selectionEnd ?? active.value.length;
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
            super::playbook::record_action(&op_str);
            Ok(())
        }
        ActionOp::XY { x, y } => click_at(app, tab_id, *x, *y, policy_config),
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
                if path_str.contains("/.ssh")
                    || path_str.contains("/.gnupg")
                    || path_str.starts_with("/etc")
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
                {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&obj_res) {
                        if let Some(object_id) =
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
                    }
                }
            }

            // Click to trigger native file chooser if necessary (which LumenDialogHandler intercepts)
            if let Some((cx, cy)) = coords {
                let _ = click_at(app, tab_id, cx, cy, policy_config);
            }

            let change_script = "(function() { var el = document.activeElement; if (el && el.type === 'file') { el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true })); } })()";
            app.engine.execute_script(tab_id, change_script);

            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = take_pending_upload_files();
            super::playbook::record_action(&format!("upload {ref_id}"));
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
                    // Wait for execution context / frame to mount
                    for _ in 0..10 {
                        if app.engine.has_browser(&id) {
                            if let Ok(Ok(_)) = app
                                .engine
                                .evaluate(&id, "document.readyState")
                                .recv_blocking()
                            {
                                break;
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
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
                WaitCondition::Text(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
            Ok(())
        }
    }
}

fn resolve_tab_target(app: &crate::app::Lumen, target: Option<&str>) -> Result<String, String> {
    if let Some(t) = target {
        let t = t.trim();
        if let Ok(n) = t.parse::<usize>() {
            n.checked_sub(1)
                .and_then(|idx| app.active_space().tabs.get(idx))
                .map(|tab| tab.id.clone())
                .ok_or_else(|| super::plugin::no_such_tab(&super::plugin::compact_tabs(app)))
        } else if app.active_space().tabs.iter().any(|tab| tab.id == t) {
            Ok(t.to_string())
        } else if let Some(tab) =
            app.active_space().tabs.iter().find(|tab| {
                tab.url.contains(t) || tab.title.to_lowercase().contains(&t.to_lowercase())
            })
        {
            Ok(tab.id.clone())
        } else {
            Err(super::plugin::no_such_tab(&super::plugin::compact_tabs(
                app,
            )))
        }
    } else {
        let active = app.active_tab_id();
        if active.is_empty() {
            Err("no active tab".into())
        } else {
            Ok(active.to_string())
        }
    }
}

fn check_click_policy(
    app: &crate::app::Lumen,
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

fn check_action_policy(
    app: &crate::app::Lumen,
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

fn click_at(
    app: &crate::app::Lumen,
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

fn press_key(app: &crate::app::Lumen, tab: &str, key: &str, mods: &gpui_kit::Modifiers) {
    app.engine.set_focus(tab, true);
    app.engine.input_key(tab, key, mods, true);
    app.engine.input_key(tab, key, mods, false);
}

fn type_at(
    app: &crate::app::Lumen,
    tab: &str,
    x: i32,
    y: i32,
    text: &str,
    replace: bool,
) -> Result<(), String> {
    app.engine.set_focus(tab, true);
    let script = format!(
        r#"(() => {{
        const target = document.elementFromPoint({x}, {y})
            ?.closest('input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]');
        if (!target || target.disabled || target.readOnly || target.matches('input[type=submit], input[type=button], input[type=file], input[type=reset], input[type=checkbox], input[type=radio]')) return 'missing';
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
    match evaluate_action(app, tab, &script)?.as_str() {
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
        _ => return Err("field unavailable. use: see".into()),
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
        if let Ok(res) = evaluate_action(app, tab, &fast_paste_script) {
            if res == "ok" {
                return Ok(());
            }
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

fn evaluate_action(app: &crate::app::Lumen, tab: &str, script: &str) -> Result<String, String> {
    app.engine
        .evaluate(tab, script)
        .recv_blocking()
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| "page unavailable. use: see".into())
}

fn submit(
    app: &crate::app::Lumen,
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
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
) -> Result<(i32, i32), String> {
    let guard = state_lock
        .lock()
        .map_err(|_| "state unavailable. use: status")?;
    let registry = guard
        .registries
        .get(tab_id)
        .ok_or_else(|| "index missing. use: see")?;
    let el = registry
        .resolve_ref_with_fallback(ref_id)
        .ok_or("stale ref. use: see")?;
    Ok(el.center)
}

/// Find a live interactive element by the name stored in the ref index.
/// Walks shadow roots because Discord-style editors often live inside them.
fn live_coords_for_name(app: &crate::app::Lumen, tab_id: &str, name: &str) -> Option<(i32, i32)> {
    if name.is_empty() {
        return None;
    }
    let script = format!(
        r#"(() => {{
            const name = {n};
            const all = [];
            const walk = root => {{
                for (const el of root.querySelectorAll('input, textarea, select, button, [contenteditable]:not([contenteditable=false]), [role=textbox], a[href]')) all.push(el);
                for (const host of root.querySelectorAll('*')) if (host.shadowRoot) walk(host.shadowRoot);
            }};
            walk(document);
            const el = all.find(el =>
                (el.getAttribute('aria-label') || el.placeholder || el.innerText || '').toLowerCase().includes(name)
            );
            if (!el) return null;
            el.scrollIntoView({{ block: 'nearest', inline: 'nearest' }});
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
fn coords_still_valid(app: &crate::app::Lumen, tab_id: &str, (x, y): (i32, i32)) -> bool {
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

fn resolve_target_coords(
    app: &crate::app::Lumen,
    target: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
) -> Result<(i32, i32), String> {
    let target = target.trim();
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

fn resolve_semantic_coords(
    kind: char,
    label: &str,
    tab_id: &str,
    state_lock: &std::sync::Arc<std::sync::Mutex<super::plugin::AgentState>>,
) -> Result<(i32, i32, String), String> {
    let guard = state_lock
        .lock()
        .map_err(|_| "state unavailable. use: status")?;
    let registry = guard
        .registries
        .get(tab_id)
        .ok_or_else(|| "index missing. use: see")?;

    let label_lower = label.to_lowercase();
    for el in registry.current_elements.values() {
        if el.kind == kind && el.name.to_lowercase().contains(&label_lower) {
            return Ok((el.center.0, el.center.1, el.id.clone()));
        }
    }
    for el in registry.previous_elements.values() {
        if el.kind == kind && el.name.to_lowercase().contains(&label_lower) {
            if let Some(cur) = registry.resolve_ref_with_fallback(&el.id) {
                return Ok((cur.center.0, cur.center.1, cur.id.clone()));
            }
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

    if let Some(pos) = chord_trimmed.rfind(|c| c == '+' || c == '-') {
        chord_trimmed
            .get(pos.saturating_add(1)..)
            .unwrap_or(chord_trimmed)
            .trim()
    } else {
        chord_trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_errors_give_next_commands() {
        for input in ["key", "go", "drag b1", "tab invalid", "bogus", "@missing"] {
            let error = parse_dsl(input).err().unwrap_or_default();
            assert!(
                error.contains("use:") || error.contains("retry"),
                "{input}: {error}"
            );
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
            paste b1;
            wait stream;
        "#;

        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 10);
        assert_eq!(
            ops.get(0),
            Some(&ActionOp::Click {
                ref_id: "b1".into()
            })
        );
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
        assert_eq!(
            ops.get(4),
            Some(&ActionOp::Key {
                chord: "Enter".into()
            })
        );
        assert_eq!(
            ops.get(5),
            Some(&ActionOp::Go {
                url: "https://example.com".into()
            })
        );
        assert_eq!(
            ops.get(6),
            Some(&ActionOp::Scroll {
                direction: ScrollDirection::Down
            })
        );
        assert_eq!(
            ops.get(7),
            Some(&ActionOp::Wait {
                condition: WaitCondition::Idle
            })
        );
        assert_eq!(
            ops.get(8),
            Some(&ActionOp::Paste {
                ref_id: Some("b1".into()),
                text: None
            })
        );
        assert_eq!(
            ops.get(9),
            Some(&ActionOp::Wait {
                condition: WaitCondition::Stream {
                    quiescent_ms: 1000,
                    timeout_ms: 30000
                }
            })
        );
    }

    #[test]
    fn test_quote_aware_multiline_statement_splitting() {
        let script = "i1=\"line 1\nline 2; still line 2\"; b1";
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops.get(0),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "line 1\nline 2; still line 2".into()
            })
        );
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::Click {
                ref_id: "b1".into()
            })
        );
    }

    #[test]
    fn test_upload_parsing_multiple_files() {
        let script = "upload b1 /tmp/test1.png /tmp/test2.png";
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops.get(0),
            Some(&ActionOp::Upload {
                ref_id: "b1".into(),
                file_paths: vec!["/tmp/test1.png".into(), "/tmp/test2.png".into()]
            })
        );
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

    #[test]
    fn test_enhanced_dsl_features() {
        let script = r#"
            navigate https://example.com;
            keydown Shift;
            key ArrowLeft;
            keyup Shift;
            key "Ctrl+v";
            click css=button[type=submit];
            click aria="Send";
            type i1 "hello human";
            type append i2 " world";
            fill css=input[name=q] "search text";
            tab select personal-tab-3;
            tab personal-tab-4;
        "#;
        let ops = parse_dsl(script).unwrap();
        assert_eq!(ops.len(), 12);
        assert_eq!(
            ops[0],
            ActionOp::Go {
                url: "https://example.com".into()
            }
        );
        assert_eq!(
            ops[1],
            ActionOp::KeyDown {
                chord: "Shift".into()
            }
        );
        assert_eq!(
            ops[2],
            ActionOp::Key {
                chord: "ArrowLeft".into()
            }
        );
        assert_eq!(
            ops[3],
            ActionOp::KeyUp {
                chord: "Shift".into()
            }
        );
        assert_eq!(
            ops[4],
            ActionOp::Key {
                chord: "Ctrl+v".into()
            }
        );
        assert_eq!(
            ops[5],
            ActionOp::Click {
                ref_id: "css=button[type=submit]".into()
            }
        );
        assert_eq!(
            ops[6],
            ActionOp::Click {
                ref_id: "aria=\"Send\"".into()
            }
        );
        assert_eq!(
            ops[7],
            ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "hello human".into()
            }
        );
        assert_eq!(
            ops[8],
            ActionOp::AppendText {
                ref_id: "i2".into(),
                text: " world".into()
            }
        );
        assert_eq!(
            ops[9],
            ActionOp::ClearAndType {
                ref_id: "css=input[name=q]".into(),
                text: "search text".into()
            }
        );
        assert_eq!(
            ops[10],
            ActionOp::Tab {
                action: TabAction::Select("personal-tab-3".into())
            }
        );
        assert_eq!(
            ops[11],
            ActionOp::Tab {
                action: TabAction::Select("personal-tab-4".into())
            }
        );
    }
}
