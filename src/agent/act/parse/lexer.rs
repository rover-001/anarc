//! Shell-like tokenization, statement splitting, quote stripping, and ref helpers.

pub(in crate::agent::act) fn is_ref_id(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && !chars.as_str().is_empty()
        && chars.all(|c| c.is_ascii_digit())
}

pub(in crate::agent::act) fn normalize_ref_token(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix('@')
        && is_ref_id(rest)
    {
        return rest;
    }
    s
}

pub(super) fn split_shell_args(input: &str) -> Vec<String> {
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

pub(super) fn split_statements(input: &str) -> Vec<String> {
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

pub(in crate::agent::act) fn unquote(s: &str) -> String {
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

pub(super) fn split_target_and_text(s: &str) -> (String, String) {
    let trimmed = s.trim();
    for prefix_len in [4usize, 5usize] {
        let prefix = if prefix_len == 4 { "css=" } else { "aria=" };
        if let Some(after_prefix) = trimmed.strip_prefix(prefix)
            && (after_prefix.starts_with('"') || after_prefix.starts_with('\''))
        {
            let quote_char = after_prefix.chars().next().unwrap_or('"');
            let quote_start = prefix_len + 1;
            if let Some(end_quote) = trimmed[quote_start..].find(quote_char) {
                let target = &trimmed[..quote_start + end_quote + 1];
                let text = trimmed[quote_start + end_quote + 1..].trim();
                return (target.to_string(), unquote(text));
            }
        }
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let target = parts.first().copied().unwrap_or("").to_string();
    let text_start = target.len();
    let text = trimmed.get(text_start..).unwrap_or("").trim();
    (target, unquote(text))
}
