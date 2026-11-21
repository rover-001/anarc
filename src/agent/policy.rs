use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationMode {
    SameSite,
    Free,
    Ask,
}

impl Default for NavigationMode {
    fn default() -> Self {
        Self::SameSite
    }
}

impl NavigationMode {
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("free") => Self::Free,
            Some("ask") => Self::Ask,
            _ => Self::SameSite,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PolicyConfig {
    pub space: Option<String>,
    pub navigation_mode: NavigationMode,
    pub confirm_intents: HashSet<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        let mut intents = HashSet::new();
        intents.insert("purchase".to_string());
        intents.insert("buy".to_string());
        intents.insert("pay".to_string());
        intents.insert("checkout".to_string());
        intents.insert("send".to_string());
        intents.insert("delete".to_string());
        intents.insert("remove".to_string());
        intents.insert("transfer".to_string());

        Self {
            space: Some("Agent".to_string()),
            navigation_mode: NavigationMode::SameSite,
            confirm_intents: intents,
        }
    }
}

pub enum NavigationCheck {
    Allowed,
    CrossSiteWarning(String),
    RequiresConfirmation(String),
}

/// Extract host/domain from a URL string
pub fn extract_host(url: &str) -> &str {
    let stripped = if let Some(idx) = url.find("://") {
        match url.get(idx.saturating_add(3)..) {
            Some(s) => s,
            None => url,
        }
    } else {
        url
    };

    let before_slash = stripped.split('/').next().unwrap_or(stripped);
    before_slash.split(':').next().unwrap_or(before_slash)
}

/// Check if navigation from `current_url` to `target_url` complies with policy
pub fn check_navigation_policy(
    current_url: &str,
    target_url: &str,
    mode: NavigationMode,
) -> NavigationCheck {
    if mode == NavigationMode::Free {
        return NavigationCheck::Allowed;
    }

    let current_host = extract_host(current_url);
    let target_host = extract_host(target_url);

    if current_host.is_empty() || current_host == "about" || current_host == "chrome" {
        return NavigationCheck::Allowed;
    }

    if current_host.eq_ignore_ascii_case(target_host) {
        return NavigationCheck::Allowed;
    }

    // Check if same registered domain (e.g. sub.example.com and example.com)
    let cur_parts: Vec<&str> = current_host.split('.').collect();
    let tgt_parts: Vec<&str> = target_host.split('.').collect();

    if cur_parts.len() >= 2 && tgt_parts.len() >= 2 {
        let cur_domain = (
            cur_parts.get(cur_parts.len().saturating_sub(2)).copied(),
            cur_parts.get(cur_parts.len().saturating_sub(1)).copied(),
        );
        let tgt_domain = (
            tgt_parts.get(tgt_parts.len().saturating_sub(2)).copied(),
            tgt_parts.get(tgt_parts.len().saturating_sub(1)).copied(),
        );

        if cur_domain == tgt_domain && cur_domain.0.is_some() && cur_domain.1.is_some() {
            return NavigationCheck::Allowed;
        }
    }

    let detail = format!("{current_host} -> {target_host}");
    match mode {
        NavigationMode::Ask => NavigationCheck::RequiresConfirmation(detail),
        NavigationMode::SameSite => NavigationCheck::CrossSiteWarning(detail),
        NavigationMode::Free => NavigationCheck::Allowed,
    }
}

/// Check if an action target, element name, or DSL op involves a sensitive intent
pub fn check_sensitive_intent(label: &str, config: &PolicyConfig) -> Option<String> {
    let lower = label.to_lowercase();
    for intent in &config.confirm_intents {
        if lower.contains(intent.as_str()) {
            return Some(format!("needs_confirm: approve {intent} in Anarc, then retry"));
        }
    }
    None
}

/// Check if page contents or title indicate a human handoff is required (CAPTCHA, 2FA, OTP, payment gateway)
pub fn check_human_handoff(title: &str, url: &str, raw_text: &str) -> Option<&'static str> {
    let combined = format!("{title} {url} {raw_text}").to_lowercase();

    if combined.contains("recaptcha")
        || combined.contains("hcaptcha")
        || combined.contains("cf-turnstile")
        || combined.contains("verify you are human")
        || combined.contains("security check")
        || combined.contains("bot detection")
        || combined.contains("please complete the security check")
    {
        return Some("needs_human: complete CAPTCHA in Anarc. use: see");
    }

    if combined.contains("two-factor")
        || combined.contains("two step")
        || combined.contains("2-step verification")
        || combined.contains("verification code")
        || combined.contains("enter security code")
        || combined.contains("authenticator app")
        || combined.contains("one-time password")
        || combined.contains("sms code")
    {
        return Some("needs_human: complete verification in Anarc. use: see");
    }

    if (combined.contains("credit card") || combined.contains("card number") || combined.contains("cvv"))
        && combined.contains("expiration")
    {
        return Some("needs_human: complete card entry in Anarc. use: see");
    }

    None
}

/// Resolve a vault secret by name
/// Resolution order:
/// 1. Environment variable `ANARC_SECRET_<NAME>`
/// 2. Local secrets file `$ANARC_DATA_DIR/agent/secrets.json`
pub fn resolve_secret(secret_name: &str) -> Option<String> {
    let env_key = format!("ANARC_SECRET_{}", secret_name.to_ascii_uppercase().replace('-', "_"));
    if let Ok(val) = std::env::var(&env_key) {
        if !val.is_empty() {
            return Some(val);
        }
    }

    let secrets_path = if let Ok(data_dir) = std::env::var("ANARC_DATA_DIR") {
        PathBuf::from(data_dir).join("agent").join("secrets.json")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share").join("anarc").join("agent").join("secrets.json")
    } else {
        return None;
    };

    if let Ok(content) = std::fs::read_to_string(&secrets_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(sec) = val.get(secret_name).and_then(|v| v.as_str()) {
                return Some(sec.to_string());
            }
        }
    }

    None
}

/// Redact secret values and passwords from audit logs and output strings
pub fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();

    while i < chars.len() {
        if let Some(&'@') = chars.get(i) {
            // Check for @secret:name
            let remaining: String = chars.get(i..).unwrap_or(&[]).iter().collect();
            if remaining.starts_with("@secret:") {
                let end = remaining
                    .find(|c: char| c.is_whitespace() || c == ';' || c == '"' || c == '\'')
                    .unwrap_or(remaining.len());
                let matched: String = chars.get(i..i.saturating_add(end)).unwrap_or(&[]).iter().collect();
                out.push_str(&matched);
                i = i.saturating_add(end);
                continue;
            }
        }
        if let Some(&c) = chars.get(i) {
            out.push(c);
        }
        i = i.saturating_add(1);
    }
    out
}

/// Audit log: append sanitized JSONL event to `$ANARC_DATA_DIR/agent/log.jsonl`
pub fn log_audit_event(op: &str, origin: &str, outcome: &str) {
    let log_path = if let Ok(data_dir) = std::env::var("ANARC_DATA_DIR") {
        PathBuf::from(data_dir).join("agent").join("log.jsonl")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share").join("anarc").join("agent").join("log.jsonl")
    } else {
        PathBuf::from("/tmp/anarc-agent-audit.jsonl")
    };

    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let sanitized_op = redact_secrets(op);
    let log_entry = serde_json::json!({
        "timestamp": timestamp,
        "origin": origin,
        "op": sanitized_op,
        "outcome": outcome
    });

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(file, "{}", serde_json::to_string(&log_entry).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_policy_samesite_and_subdomains() {
        let mode = NavigationMode::SameSite;
        let res = check_navigation_policy("https://github.com/login", "https://github.com/settings", mode);
        assert!(matches!(res, NavigationCheck::Allowed));

        let res2 = check_navigation_policy("https://sub.github.com/a", "https://github.com/b", mode);
        assert!(matches!(res2, NavigationCheck::Allowed));

        let res3 = check_navigation_policy("https://github.com/", "https://attacker.com/leak", mode);
        assert!(matches!(res3, NavigationCheck::CrossSiteWarning(_)));
    }

    #[test]
    fn test_sensitive_intent_confirm_gate() {
        let config = PolicyConfig::default();
        let res = check_sensitive_intent("Complete Purchase and Pay $50", &config);
        assert!(res.is_some());
        let msg = res.unwrap_or_default();
        assert!(msg.contains("needs_confirm"));

        let safe = check_sensitive_intent("Search repository documentation", &config);
        assert!(safe.is_none());
    }

    #[test]
    fn test_human_handoff_detection() {
        let res = check_human_handoff("Security Verification", "https://example.com/captcha", "Please solve this recaptcha");
        assert!(res.is_some());
        assert!(res.unwrap_or_default().contains("needs_human"));

        let res2 = check_human_handoff("Two-factor authentication", "https://example.com/2fa", "Enter your 6-digit sms code");
        assert!(res2.is_some());
        assert!(res2.unwrap_or_default().contains("needs_human"));

        let res3 = check_human_handoff("Dashboard", "https://example.com/dash", "Welcome back Rover");
        assert!(res3.is_none());
    }

    #[test]
    fn test_secret_resolution_and_redaction() {
        unsafe {
            std::env::set_var("ANARC_SECRET_GITHUB_TEST", "super_secret_pat_12345");
        }
        let secret = resolve_secret("github-test");
        assert_eq!(secret.as_deref(), Some("super_secret_pat_12345"));

        let redacted = redact_secrets("i2=@secret:github-test; key Enter");
        assert_eq!(redacted, "i2=@secret:github-test; key Enter");
    }
}
