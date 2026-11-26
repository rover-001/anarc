//! DSL parser entry point and single-operation dispatcher.

mod file_args;
mod lexer;
mod ops;

use super::types::{ActionOp, ScrollDirection};
pub(in crate::agent::act) use lexer::{is_ref_id, normalize_ref_token, unquote};

use lexer::split_statements;

pub fn parse_dsl(input: &str) -> Result<Vec<ActionOp>, String> {
    parse_dsl_at_depth(input, 0)
}

fn parse_dsl_at_depth(input: &str, depth: usize) -> Result<Vec<ActionOp>, String> {
    if depth >= 16 {
        return Err("macro nesting limit. use: see".into());
    }
    let mut ops = Vec::new();
    for stmt in split_statements(input) {
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
            ops.extend(parse_dsl_at_depth(&expanded, depth + 1)?);
            continue;
        }

        ops.push(parse_single_op(trimmed)?);
    }

    Ok(ops)
}

fn parse_single_op(raw: &str) -> Result<ActionOp, String> {
    let s = raw.trim();

    if let Some(op) = ops::parse_key_op(s)? {
        return Ok(op);
    }

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

    match s {
        "submit" => return Ok(ActionOp::Submit),
        "back" => return Ok(ActionOp::Back),
        "fwd" | "forward" => return Ok(ActionOp::Fwd),
        "reload" => return Ok(ActionOp::Reload),
        _ => {}
    }

    if let Some(stripped) = s.strip_prefix("scroll ") {
        let direction = match stripped.trim() {
            "down" => ScrollDirection::Down,
            "up" => ScrollDirection::Up,
            "top" => ScrollDirection::Top,
            "end" => ScrollDirection::End,
            target => ScrollDirection::Ref(target.to_string()),
        };
        return Ok(ActionOp::Scroll { direction });
    }

    if let Some(stripped) = s.strip_prefix("hover ") {
        let ref_id = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        return Ok(ActionOp::Hover { ref_id });
    }

    if let Some(stripped) = s.strip_prefix("focus ") {
        let ref_id = normalize_ref_token(unquote(stripped.trim()).trim()).to_string();
        return Ok(ActionOp::Focus { ref_id });
    }

    if let Some(op) = ops::parse_dblclick_op(s)?
        .or(ops::parse_drag_op(s)?)
        .or(ops::parse_xy_op(s))
        .or(ops::parse_paste_op(s)?)
        .or(ops::parse_upload_op(s)?)
        .or(ops::parse_dialog_op(s))
        .or(ops::parse_tab_op(s)?)
        .or(ops::parse_top_level_pin_fav_op(s))
        .or(ops::parse_space_op(s)?)
        .or(ops::parse_split_op(s)?)
        .or(ops::parse_wait_op(s)?)
        .or(ops::parse_semantic_op(s)?)
        .or(ops::parse_set_op(s)?)
        .or(ops::parse_click_op(s)?)
        .or(ops::parse_fill_type_op(s)?)
        .or(ops::parse_assign_op(s)?)
    {
        return Ok(op);
    }

    if is_ref_id(s) {
        return Ok(ActionOp::Click {
            ref_id: s.to_string(),
        });
    }

    Err("unknown action. use: see".into())
}
