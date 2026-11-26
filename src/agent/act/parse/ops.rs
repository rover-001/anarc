//! Per-command DSL parsers for `parse_single_op`.

use super::file_args::{expand_file_arg, is_file_path_arg, read_file_arg};
use super::lexer::{
    is_ref_id, normalize_ref_token, split_shell_args, split_target_and_text, unquote,
};
use crate::agent::act::types::{
    ActionOp, DblClickTarget, DragTarget, SplitAction, TabAction, WaitCondition,
};

fn parse_chord_cmd(s: &str, cmd: &str, usage: &str) -> Result<Option<String>, String> {
    let Some(stripped) = s
        .strip_prefix(&format!("{cmd} "))
        .or_else(|| (s == cmd).then_some(""))
    else {
        return Ok(None);
    };
    let chord = unquote(stripped.trim()).to_string();
    if chord.is_empty() {
        return Err(usage.into());
    }
    Ok(Some(chord))
}

pub(super) fn parse_key_op(s: &str) -> Result<Option<ActionOp>, String> {
    if let Some(chord) = parse_chord_cmd(s, "key", "missing key. use: do \"key Enter\"")? {
        return Ok(Some(ActionOp::Key { chord }));
    }
    if let Some(chord) = parse_chord_cmd(s, "keydown", "missing key. use: do \"keydown Shift\"")? {
        return Ok(Some(ActionOp::KeyDown { chord }));
    }
    if let Some(chord) = parse_chord_cmd(s, "keyup", "missing key. use: do \"keyup Shift\"")? {
        return Ok(Some(ActionOp::KeyUp { chord }));
    }
    Ok(None)
}

pub(super) fn parse_dblclick_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s
        .strip_prefix("dblclick ")
        .or_else(|| (s == "dblclick").then_some(""))
    else {
        return Ok(None);
    };
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
            return Ok(Some(ActionOp::DblClick {
                target: DblClickTarget::Coords(x, y),
            }));
        }
        return Err("invalid dblclick xy coords. use: do \"dblclick xy 100 200\"".into());
    }
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if let [first, second] = parts.as_slice()
        && let (Ok(x), Ok(y)) = (first.parse::<i32>(), second.parse::<i32>())
    {
        return Ok(Some(ActionOp::DblClick {
            target: DblClickTarget::Coords(x, y),
        }));
    }
    let target = normalize_ref_token(unquote(rest).trim()).to_string();
    Ok(Some(ActionOp::DblClick {
        target: DblClickTarget::Target(target),
    }))
}

pub(super) fn parse_drag_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s.strip_prefix("drag ") else {
        return Ok(None);
    };
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
                return Err(
                    "invalid drag arguments. use: do \"drag b1 b2\" or \"drag 10 20 30 40\"".into(),
                );
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
    Ok(Some(ActionOp::Drag {
        from: drag.0,
        to: drag.1,
    }))
}

pub(super) fn parse_xy_op(s: &str) -> Option<ActionOp> {
    let stripped = s.strip_prefix("xy ")?;
    let parts: Vec<&str> = stripped.split_whitespace().collect();
    let x: i32 = parts.first().and_then(|v| v.parse().ok()).unwrap_or(0);
    let y: i32 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
    Some(ActionOp::XY { x, y })
}

pub(super) fn parse_paste_op(s: &str) -> Result<Option<ActionOp>, String> {
    if s != "paste" && !s.starts_with("paste ") {
        return Ok(None);
    }
    let rest = s.strip_prefix("paste").unwrap_or("").trim();
    if rest.is_empty() {
        return Ok(Some(ActionOp::Paste {
            ref_id: None,
            text: None,
        }));
    }
    if let Some(file_path) = rest.strip_prefix("@file:") {
        let content = read_file_arg(file_path)?;
        return Ok(Some(ActionOp::Paste {
            ref_id: None,
            text: Some(content),
        }));
    }
    let first = rest.split_whitespace().next().unwrap_or("");
    if is_ref_id(first) {
        let ref_id = first.to_string();
        let remainder = rest.get(first.len()..).unwrap_or("").trim();
        let text = if remainder.is_empty() {
            None
        } else if let Some(file_path) = remainder.strip_prefix("@file:") {
            Some(read_file_arg(file_path)?)
        } else {
            Some(unquote(remainder))
        };
        Ok(Some(ActionOp::Paste {
            ref_id: Some(ref_id),
            text,
        }))
    } else {
        Ok(Some(ActionOp::Paste {
            ref_id: None,
            text: Some(unquote(rest)),
        }))
    }
}

pub(super) fn parse_upload_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s.strip_prefix("upload ").or_else(|| s.strip_prefix("up ")) else {
        return Ok(None);
    };
    let args = split_shell_args(stripped);
    let Some(first) = args.first().map(String::as_str) else {
        return Err("missing upload ref and path. use: do \"upload i1 /path/file\"".into());
    };
    let (ref_id, raw_paths) = if is_file_path_arg(first) {
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
        file_paths.extend(expand_file_arg(raw_path));
    }
    if file_paths.is_empty() {
        return Err("no matching files found for upload. use: see".into());
    }
    Ok(Some(ActionOp::Upload { ref_id, file_paths }))
}

pub(super) fn parse_dialog_op(s: &str) -> Option<ActionOp> {
    let stripped = s.strip_prefix("dialog ")?;
    let d = stripped.trim();
    Some(ActionOp::Dialog {
        accept: d == "ok" || d == "accept" || d == "true",
    })
}

fn parse_optional_target(rest: &str) -> Option<String> {
    let t = rest.trim();
    (!t.is_empty()).then(|| t.to_string())
}

pub(super) fn parse_tab_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s.strip_prefix("tab ") else {
        return Ok(None);
    };
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
            target: parse_optional_target(target),
        }
    } else if rest == "unpin" {
        TabAction::Unpin { target: None }
    } else if let Some(target) = rest.strip_prefix("unpin ") {
        TabAction::Unpin {
            target: parse_optional_target(target),
        }
    } else if rest == "fav" || rest == "favorite" {
        TabAction::Favorite { target: None }
    } else if let Some(target) = rest
        .strip_prefix("fav ")
        .or_else(|| rest.strip_prefix("favorite "))
    {
        TabAction::Favorite {
            target: parse_optional_target(target),
        }
    } else if rest == "unfav" || rest == "unfavorite" {
        TabAction::Unfavorite { target: None }
    } else if let Some(target) = rest
        .strip_prefix("unfav ")
        .or_else(|| rest.strip_prefix("unfavorite "))
    {
        TabAction::Unfavorite {
            target: parse_optional_target(target),
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
    Ok(Some(ActionOp::Tab { action }))
}

fn parse_pin_fav_target(rest: &str) -> Option<String> {
    let trimmed = rest.trim();
    let target = trimmed.strip_prefix("tab ").unwrap_or(trimmed).trim();
    (!target.is_empty()).then(|| target.to_string())
}

pub(super) fn parse_top_level_pin_fav_op(s: &str) -> Option<ActionOp> {
    let action = if s == "pin" {
        TabAction::Pin { target: None }
    } else if let Some(rest) = s.strip_prefix("pin ") {
        TabAction::Pin {
            target: parse_pin_fav_target(rest),
        }
    } else if s == "unpin" {
        TabAction::Unpin { target: None }
    } else if let Some(rest) = s.strip_prefix("unpin ") {
        TabAction::Unpin {
            target: parse_pin_fav_target(rest),
        }
    } else if s == "fav" || s == "favorite" {
        TabAction::Favorite { target: None }
    } else if let Some(rest) = s
        .strip_prefix("fav ")
        .or_else(|| s.strip_prefix("favorite "))
    {
        TabAction::Favorite {
            target: parse_pin_fav_target(rest),
        }
    } else if s == "unfav" || s == "unfavorite" {
        TabAction::Unfavorite { target: None }
    } else {
        let rest = s
            .strip_prefix("unfav ")
            .or_else(|| s.strip_prefix("unfavorite "))?;
        TabAction::Unfavorite {
            target: parse_pin_fav_target(rest),
        }
    };
    Some(ActionOp::Tab { action })
}

pub(super) fn parse_space_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s
        .strip_prefix("space ")
        .or_else(|| (s == "space").then_some(""))
    else {
        return Ok(None);
    };
    let target = unquote(stripped.trim()).trim().to_string();
    if target.is_empty() {
        return Err("missing space name or id. use: do \"space <name>\"".into());
    }
    Ok(Some(ActionOp::Space { target }))
}

pub(super) fn parse_split_op(s: &str) -> Result<Option<ActionOp>, String> {
    if s == "split" {
        return Ok(Some(ActionOp::Split {
            action: SplitAction::WithNext,
        }));
    }
    let Some(stripped) = s.strip_prefix("split ") else {
        return Ok(None);
    };
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
            _ => return Err("invalid split args. use: do \"split\" or \"split <url>\"".into()),
        }
    };
    Ok(Some(ActionOp::Split { action }))
}

pub(super) fn parse_wait_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s
        .strip_prefix("wait ")
        .or_else(|| s.strip_prefix("wait_for "))
    else {
        return Ok(None);
    };
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
        WaitCondition::Text(unquote(w))
    };
    Ok(Some(ActionOp::Wait { condition }))
}

pub(super) fn parse_semantic_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(colon_idx) = s.find(':') else {
        return Ok(None);
    };
    let kind_str = s.get(0..colon_idx).unwrap_or("");
    if kind_str.len() != 1 {
        return Ok(None);
    }
    let kind = kind_str.chars().next().unwrap_or('x');
    let rest = s.get(colon_idx.saturating_add(1)..).unwrap_or("");
    if let Some(eq_idx) = rest.find('=') {
        let label_part = rest.get(0..eq_idx).unwrap_or("");
        let text_part = rest.get(eq_idx.saturating_add(1)..).unwrap_or("");
        let text = if let Some(file_path) = text_part.trim().strip_prefix("@file:") {
            read_file_arg(file_path)?
        } else {
            unquote(text_part)
        };
        Ok(Some(ActionOp::SemanticType {
            kind,
            label: unquote(label_part),
            text,
        }))
    } else {
        Ok(Some(ActionOp::SemanticClick {
            kind,
            label: unquote(rest),
        }))
    }
}

pub(super) fn parse_set_op(s: &str) -> Result<Option<ActionOp>, String> {
    let Some(stripped) = s
        .strip_prefix("set ")
        .or_else(|| (s == "set").then_some(""))
    else {
        return Ok(None);
    };
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
        _ => return Err("invalid set syntax. use: do \"set @i10 \\\"750\\\"\"".into()),
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
    Ok(Some(ActionOp::SetValue {
        target,
        value,
        enter,
    }))
}

pub(super) fn parse_click_op(s: &str) -> Result<Option<ActionOp>, String> {
    if let Some(stripped) = s
        .strip_prefix("clickjs ")
        .or_else(|| s.strip_prefix("domclick "))
    {
        let target = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"clickjs b1\"".into());
        }
        return Ok(Some(ActionOp::DomClick { ref_id: target }));
    }
    if let Some(stripped) = s.strip_prefix("click! ") {
        let target = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"click! b1\"".into());
        }
        return Ok(Some(ActionOp::ForceClick { ref_id: target }));
    }
    if let Some(stripped) = s.strip_prefix("click ") {
        let rest = stripped.trim();
        if let Some(xy_rest) = rest.strip_prefix("xy ") {
            let parts: Vec<&str> = xy_rest.split_whitespace().collect();
            if let (Some(Ok(x)), Some(Ok(y))) = (
                parts.first().map(|v| v.parse::<i32>()),
                parts.get(1).map(|v| v.parse::<i32>()),
            ) {
                return Ok(Some(ActionOp::XY { x, y }));
            }
            return Err("invalid click xy coords. use: do \"click xy 100 200\"".into());
        }
        let target = normalize_ref_token(unquote(rest).trim()).to_string();
        if target.is_empty() {
            return Err("missing click target. use: do \"click b1\"".into());
        }
        return Ok(Some(ActionOp::Click { ref_id: target }));
    }
    if s.starts_with("css=")
        || s.starts_with("aria=")
        || s.starts_with("css:")
        || s.starts_with("aria:")
    {
        return Ok(Some(ActionOp::Click {
            ref_id: s.to_string(),
        }));
    }
    Ok(None)
}

pub(super) fn parse_fill_type_op(s: &str) -> Result<Option<ActionOp>, String> {
    if let Some(stripped) = s.strip_prefix("fill ") {
        let (target, text) = split_target_and_text(stripped.trim());
        if target.is_empty() {
            return Err("missing fill target. use: do \"fill i1 text\"".into());
        }
        return Ok(Some(ActionOp::ClearAndType {
            ref_id: target,
            text,
        }));
    }
    let Some(stripped) = s.strip_prefix("type ") else {
        return Ok(None);
    };
    let rest = stripped.trim();
    let is_quoted_single = ((rest.starts_with('"') && rest.ends_with('"'))
        || (rest.starts_with('\'') && rest.ends_with('\''))
        || (rest.starts_with('`') && rest.ends_with('`')))
        && rest.len() >= 2;
    if is_quoted_single && !rest.starts_with("css=") && !rest.starts_with("aria=") {
        let args = split_shell_args(rest);
        if args.len() == 1 {
            return Ok(Some(ActionOp::TypeActive {
                text: args.into_iter().next().unwrap_or_default(),
            }));
        }
    }
    if let Some(file_path) = rest.strip_prefix("@file:") {
        let content = read_file_arg(file_path)?;
        return Ok(Some(ActionOp::TypeActive { text: content }));
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
        Ok(Some(ActionOp::AppendText {
            ref_id: target,
            text,
        }))
    } else {
        Ok(Some(ActionOp::ClearAndType {
            ref_id: target,
            text,
        }))
    }
}

pub(super) fn parse_assign_op(s: &str) -> Result<Option<ActionOp>, String> {
    if let Some(eq_idx) = s.find('=') {
        let ref_id = s.get(0..eq_idx).unwrap_or("").trim().to_string();
        let val_part = s.get(eq_idx.saturating_add(1)..).unwrap_or("").trim();
        if let Some(secret_name) = val_part.strip_prefix("@secret:") {
            return Ok(Some(ActionOp::TypeSecret {
                ref_id,
                secret_name: secret_name.trim().to_string(),
            }));
        }
        let text = if let Some(file_path) = val_part.strip_prefix("@file:") {
            read_file_arg(file_path)?
        } else {
            unquote(val_part)
        };
        return Ok(Some(ActionOp::ClearAndType { ref_id, text }));
    }
    if let Some(plus_idx) = s.find('+') {
        let ref_id = s.get(0..plus_idx).unwrap_or("").trim().to_string();
        let val_part = s.get(plus_idx.saturating_add(1)..).unwrap_or("").trim();
        let text = if let Some(file_path) = val_part.strip_prefix("@file:") {
            read_file_arg(file_path)?
        } else {
            unquote(val_part)
        };
        return Ok(Some(ActionOp::AppendText { ref_id, text }));
    }
    Ok(None)
}
