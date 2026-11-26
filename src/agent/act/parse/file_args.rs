//! `@file:` reading and glob expansion helpers for DSL arguments.

use super::lexer::{is_ref_id, normalize_ref_token, unquote};

pub(super) fn is_file_path_arg(arg: &str) -> bool {
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

pub(super) fn read_file_arg(file_path: &str) -> Result<String, String> {
    let unquoted = unquote(file_path);
    let p = std::path::Path::new(unquoted.trim());
    std::fs::read_to_string(p).map_err(|e| format!("cannot read file '{file_path}': {e}. use: see"))
}

pub(super) fn expand_file_arg(arg: &str) -> Vec<String> {
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
