use super::types::{
    ActionOp, DblClickTarget, DragTarget, ScrollDirection, SplitAction, TabAction, WaitCondition,
};

pub(super) fn is_ref_id(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && !chars.as_str().is_empty()
        && chars.all(|c| c.is_ascii_digit())
}

pub(super) fn normalize_ref_token(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('@')
        && is_ref_id(rest)
    {
        return rest;
    }
    s
}

fn is_file_path_arg(arg: &str) -> bool {
    if arg.starts_with('/')
        || arg.starts_with("./")
        || arg.starts_with("../")
        || arg.starts_with('~')
    {
        return true;
    }
    let bytes = arg.as_bytes();
    if let [drive, b':', sep, ..] = bytes
        && drive.is_ascii_alphabetic()
        && (*sep == b'\\' || *sep == b'/')
    {
        return true;
    }
    !is_ref_id(normalize_ref_token(arg))
        && !arg.starts_with("css=")
        && !arg.starts_with("css:")
        && !arg.starts_with("aria=")
        && !arg.starts_with("aria:")
        && std::path::Path::new(arg).exists()
}

fn split_shell_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escaped = false;
    let mut has_token = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            has_token = true;
            continue;
        }
        if ch == '\\' && !in_single {
            escaped = true;
            has_token = true;
            continue;
        }
        if ch == '"' && !in_single && !in_backtick {
            in_double = !in_double;
            has_token = true;
            continue;
        }
        if ch == '\'' && !in_double && !in_backtick {
            in_single = !in_single;
            has_token = true;
            continue;
        }
        if ch == '`' && !in_single && !in_double {
            in_backtick = !in_backtick;
            has_token = true;
            continue;
        }
        if ch.is_whitespace() && !in_single && !in_double && !in_backtick {
            if has_token {
                args.push(std::mem::take(&mut current));
                has_token = false;
            }
        } else {
            current.push(ch);
            has_token = true;
        }
    }
    if has_token {
        args.push(current);
    }
    args
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
    let mut in_backtick = false;
    let mut in_triple = false;
    let mut escaped = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let Some(ch) = chars.get(i).copied() else {
            break;
        };
        if ch == '\r' {
            i += 1;
            continue;
        }
        if escaped {
            current.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            current.push(ch);
            i += 1;
            continue;
        }

        // Check triple quotes
        let has_triple_quote = chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"');
        if ch == '"' && has_triple_quote && !in_single && !in_backtick {
            in_triple = !in_triple;
            current.push_str("\"\"\"");
            i += 3;
            continue;
        }

        if ch == '"' && !in_single && !in_backtick && !in_triple {
            in_double = !in_double;
            current.push(ch);
            i += 1;
            continue;
        }
        if ch == '\'' && !in_double && !in_backtick && !in_triple {
            in_single = !in_single;
            current.push(ch);
            i += 1;
            continue;
        }
        if ch == '`' && !in_single && !in_double && !in_triple {
            in_backtick = !in_backtick;
            current.push(ch);
            i += 1;
            continue;
        }

        if (ch == ';' || ch == '\n') && !in_single && !in_double && !in_backtick && !in_triple {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
        i += 1;
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
        if let Some(stripped) = trimmed.strip_prefix('@')
            && !stripped.starts_with("secret:")
        {
            let parts: Vec<&str> = stripped.split_whitespace().collect();
            let macro_name = parts.first().copied().unwrap_or("");
            let raw_args: Vec<String> = parts
                .get(1..)
                .unwrap_or(&[])
                .iter()
                .map(|a| unquote(a))
                .collect();
            let playbook = crate::agent::playbook::get_or_init_registry();
            let expanded = playbook.expand_macro(macro_name, &raw_args)?;
            let expanded_ops = parse_dsl_at_depth(&expanded, depth + 1)?;
            ops.extend(expanded_ops);
            continue;
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
        let r = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        return Ok(ActionOp::Hover { ref_id: r });
    }

    // Focus
    if let Some(stripped) = s.strip_prefix("focus ") {
        let r = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        return Ok(ActionOp::Focus { ref_id: r });
    }

    // Double-click: "dblclick @b1", "dblclick xy 100 200", "dblclick 100 200", "dblclick \"Text\""
    if let Some(stripped) = s
        .strip_prefix("dblclick ")
        .or_else(|| (s == "dblclick").then_some(""))
    {
        let rest = stripped.trim();
        if rest.is_empty() {
            return Err("missing dblclick target. use: do \"dblclick @b1\"".into());
        }
        if let Some(xy_rest) = rest.strip_prefix("xy ") {
            let parts: Vec<&str> = xy_rest.split_whitespace().collect();
            if let (Some(Ok(x)), Some(Ok(y))) = (
                parts.first().map(|v| v.parse::<i32>()),
                parts.get(1).map(|v| v.parse::<i32>()),
            ) {
                return Ok(ActionOp::DblClick {
                    target: DblClickTarget::Coords(x, y),
                });
            }
            return Err("invalid dblclick xy coords. use: do \"dblclick xy 100 200\"".into());
        }
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if let [first, second] = parts.as_slice()
            && let (Ok(x), Ok(y)) = (first.parse::<i32>(), second.parse::<i32>())
        {
            return Ok(ActionOp::DblClick {
                target: DblClickTarget::Coords(x, y),
            });
        }
        let target = normalize_ref_token(unquote(rest).trim()).to_string();
        return Ok(ActionOp::DblClick {
            target: DblClickTarget::Target(target),
        });
    }

    // Drag: "drag b1 b2", "drag x1 y1 x2 y2", "drag xy x1 y1 x2 y2", "drag b1 x2 y2", "drag x1 y1 b2"
    if let Some(stripped) = s.strip_prefix("drag ") {
        let rest = stripped.trim().strip_prefix("xy ").unwrap_or(stripped.trim());
        let args = split_shell_args(rest);
        let drag = match args.as_slice() {
            [a, b] => DragTarget::Ref(normalize_ref_token(a).to_string())
                .into_pair(DragTarget::Ref(normalize_ref_token(b).to_string())),
            [a, b, c] => {
                if let (Ok(x2), Ok(y2)) = (b.parse::<i32>(), c.parse::<i32>()) {
                    (
                        DragTarget::Ref(normalize_ref_token(a).to_string()),
                        DragTarget::Coords(x2, y2),
                    )
                } else if let (Ok(x1), Ok(y1)) = (a.parse::<i32>(), b.parse::<i32>()) {
                    (
                        DragTarget::Coords(x1, y1),
                        DragTarget::Ref(normalize_ref_token(c).to_string()),
                    )
                } else {
                    return Err("invalid drag arguments. use: do \"drag b1 b2\" or \"drag 10 20 30 40\"".into());
                }
            }
            [a, b, c, d] => {
                let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (
                    a.parse::<i32>(),
                    b.parse::<i32>(),
                    c.parse::<i32>(),
                    d.parse::<i32>(),
                ) else {
                    return Err("invalid drag coordinates. use: do \"drag 10 20 30 40\"".into());
                };
                (DragTarget::Coords(x1, y1), DragTarget::Coords(x2, y2))
            }
            _ => return Err("missing drag refs. use: see".into()),
        };
        return Ok(ActionOp::Drag {
            from: drag.0,
            to: drag.1,
        });
    }

    // XY: "xy 100 200"
    if let Some(stripped) = s.strip_prefix("xy ") {
        let parts: Vec<&str> = stripped.split_whitespace().collect();
        let x: i32 = parts.first().and_then(|v| v.parse().ok()).unwrap_or(0);
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
        let first = parts.first().copied().unwrap_or("");
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
        let args = split_shell_args(stripped);
        let first = match args.first() {
            Some(first) => first.as_str(),
            None => {
                return Err("missing upload ref and path. use: do \"upload i1 /path/file\"".into());
            }
        };
        let (r, raw_paths) = if is_file_path_arg(first) {
            (String::new(), &args[..])
        } else {
            (
                normalize_ref_token(first).to_string(),
                args.get(1..).unwrap_or(&[]),
            )
        };
        if raw_paths.is_empty() {
            return Err("missing upload file path. use: do \"upload i1 /path/file\"".into());
        }
        let mut file_paths = Vec::new();
        for raw_path in raw_paths {
            let expanded = expand_file_arg(raw_path);
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

    // Space: "space <id|name|index>"
    if let Some(stripped) = s
        .strip_prefix("space ")
        .or_else(|| (s == "space").then_some(""))
    {
        let target = unquote(stripped.trim()).trim().to_string();
        if target.is_empty() {
            return Err("missing space name or id. use: do \"space <name>\"".into());
        }
        return Ok(ActionOp::Space { target });
    }

    // Split: "split", "split off", "split <url>", "split <tab1> <tab2>"
    if s == "split" {
        return Ok(ActionOp::Split {
            action: SplitAction::WithNext,
        });
    }
    if let Some(stripped) = s.strip_prefix("split ") {
        let rest = stripped.trim();
        let action = if matches!(rest, "off" | "close" | "exit" | "none") {
            SplitAction::Exit
        } else {
            let args = split_shell_args(rest);
            match args.as_slice() {
                [single] if single.starts_with("http://") || single.starts_with("https://") => {
                    SplitAction::WithUrl(single.clone())
                }
                [a, b] => SplitAction::Pair(a.clone(), b.clone()),
                _ => {
                    return Err("invalid split args. use: do \"split\" or \"split <url>\"".into());
                }
            }
        };
        return Ok(ActionOp::Split { action });
    }

    // Wait: "wait idle", "wait 500", "wait stream", "wait stream 15000", "wait text-quiescent 1000", "wait \"Text\"", "wait_for \"Text\""
    if let Some(stripped) = s
        .strip_prefix("wait ")
        .or_else(|| s.strip_prefix("wait_for "))
    {
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
                .first()
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

    // Batch set_value: "set @i10 \"750\"", "set @i8 \"80\" enter"
    if let Some(stripped) = s
        .strip_prefix("set ")
        .or_else(|| (s == "set").then_some(""))
    {
        let args = split_shell_args(stripped);
        let (raw_target, raw_val, enter) = match args.as_slice() {
            [single] if single.contains('=') => {
                let (t, v) = single.split_once('=').unwrap_or(("", ""));
                (t.trim(), v.trim().to_string(), false)
            }
            [target, val] => (target.as_str(), val.clone(), false),
            [target, val, flag]
                if flag.eq_ignore_ascii_case("enter") || flag.eq_ignore_ascii_case("commit") =>
            {
                (target.as_str(), val.clone(), true)
            }
            _ => {
                return Err("invalid set syntax. use: do \"set @i10 \\\"750\\\"\"".into());
            }
        };
        let target = normalize_ref_token(raw_target).to_string();
        if target.is_empty() {
            return Err("missing set target. use: do \"set @i10 \\\"750\\\"\"".into());
        }
        let value = if let Some(file_path) = raw_val.trim().strip_prefix("@file:") {
            read_file_arg(file_path)?
        } else {
            raw_val
        };
        return Ok(ActionOp::SetValue {
            target,
            value,
            enter,
        });
    }

    // Direct DOM click: "clickjs b1", "domclick b1". Dispatches element.click()
    // over CDP, which SPAs with synthetic event delegation do not swallow.
    if let Some(stripped) = s
        .strip_prefix("clickjs ")
        .or_else(|| s.strip_prefix("domclick "))
    {
        let target = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"clickjs b1\"".into());
        }
        return Ok(ActionOp::DomClick { ref_id: target });
    }

    // Force click: "click! b1", "click! \"Rename\"" (bypasses occlusion check)
    if let Some(stripped) = s.strip_prefix("click! ") {
        let target = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"click! b1\"".into());
        }
        return Ok(ActionOp::ForceClick { ref_id: target });
    }

    // Click command: "click b1", "click xy 100 200", "click css=...", "click aria=...", "click \"Add a new color\""
    if let Some(stripped) = s.strip_prefix("click ") {
        let rest = stripped.trim();
        if let Some(xy_rest) = rest.strip_prefix("xy ") {
            let parts: Vec<&str> = xy_rest.split_whitespace().collect();
            if let (Some(Ok(x)), Some(Ok(y))) = (
                parts.first().map(|v| v.parse::<i32>()),
                parts.get(1).map(|v| v.parse::<i32>()),
            ) {
                return Ok(ActionOp::XY { x, y });
            }
            return Err("invalid click xy coords. use: do \"click xy 100 200\"".into());
        }
        let target = normalize_ref_token(unquote(rest).trim()).to_string();
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

    // Type command: "type \"Rank\"" (into activeElement), "type css=... text", "type aria=... text", "type i1 text", "type append i1 text"
    if let Some(stripped) = s.strip_prefix("type ") {
        let rest = stripped.trim();
        let is_quoted_single = ((rest.starts_with('"') && rest.ends_with('"'))
            || (rest.starts_with('\'') && rest.ends_with('\''))
            || (rest.starts_with('`') && rest.ends_with('`')))
            && rest.len() >= 2;
        if is_quoted_single && !rest.starts_with("css=") && !rest.starts_with("aria=") {
            let args = split_shell_args(rest);
            if args.len() == 1 {
                return Ok(ActionOp::TypeActive {
                    text: args.into_iter().next().unwrap_or_default(),
                });
            }
        }
        if let Some(file_path) = rest.strip_prefix("@file:") {
            let content = read_file_arg(file_path)?;
            return Ok(ActionOp::TypeActive { text: content });
        }
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

pub(super) fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.starts_with("\"\"\"") && t.ends_with("\"\"\"") && t.len() >= 6 {
        return t[3..t.len() - 3].to_string();
    }
    if (t.starts_with('"') && t.ends_with('"'))
        || (t.starts_with('\'') && t.ends_with('\''))
        || (t.starts_with('`') && t.ends_with('`'))
    {
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
    let target = parts.first().copied().unwrap_or("").to_string();
    let text_start = target.len();
    let text = trimmed.get(text_start..).unwrap_or("").trim();
    (target, unquote(text))
}
