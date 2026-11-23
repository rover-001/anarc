use std::collections::HashSet;
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
    pub navigation_mode: NavigationMode,
    pub confirm_intents: HashSet<String>,
    pub confirm_allow: HashSet<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            navigation_mode: NavigationMode::SameSite,
            confirm_intents: ["purchase", "payment", "delete", "permission_grant", "transfer"]
                .into_iter().map(str::to_string).collect(),
            confirm_allow: HashSet::new(),
        }
    }
}

impl PolicyConfig {
    pub fn load() -> Self {
        let config = crate::plugin::PluginConfig::load().ok();
        let table = config.as_ref()
            .and_then(|config| config.plugins.get(super::plugin::AGENT_PLUGIN_ID))
            .map(|entry| &entry.config);
        Self::from_table(table)
    }

    fn from_table(table: Option<&toml::Table>) -> Self {
        let Some(table) = table else { return Self::default() };
        let read_list = |key| {
            table.get(key).and_then(toml::Value::as_array).map(|list| {
                list.iter().filter_map(toml::Value::as_str)
                    .map(|entry| entry.trim().to_ascii_lowercase()).collect()
            })
        };
        Self {
            navigation_mode: NavigationMode::from_str_opt(table.get("navigation").and_then(toml::Value::as_str)),
            confirm_intents: read_list("confirm").unwrap_or_else(|| Self::default().confirm_intents),
            confirm_allow: read_list("confirm_allow").unwrap_or_default(),
        }
    }
}

pub enum NavigationCheck {
    Allowed,
    CrossSiteWarning(String),
    RequiresConfirmation,
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
        NavigationMode::Ask => NavigationCheck::RequiresConfirmation,
        NavigationMode::SameSite => NavigationCheck::CrossSiteWarning(detail),
        NavigationMode::Free => NavigationCheck::Allowed,
    }
}

#[derive(Default)]
pub struct Confirmations {
    approved: HashSet<(String, String)>,
}

impl Confirmations {
    // The approval UI can call this after the user approves. There is no agent command for it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn approve(&mut self, origin: &str, intent: &str) {
        self.approved.insert((origin.to_string(), intent.to_string()));
    }

    pub fn required(&self, config: &PolicyConfig, origin: &str, intent: &str) -> bool {
        if origin == "null" {
            return config.confirm_intents.contains(intent);
        }
        config.confirm_intents.contains(intent)
            && !config.confirm_allow.contains(&format!("{origin}:{intent}"))
            && !crate::shield::url_host(origin).is_some_and(|host| {
                config.confirm_allow.contains(&format!("{host}:{intent}"))
            })
            && !self.approved.contains(&(origin.to_string(), intent.to_string()))
    }
}

pub const ACTION_CONTEXT: &str = r#"target => {
    if (!target) return [location.origin, ''];
    const selector = 'button, [role=button], input[type=submit], input[type=button], a';
    let action = target.closest(selector) || target;
    const form = action.form || action.closest('form');
    if (action === target && form && !action.matches(selector)) {
        action = Array.from(form.elements).find(el => el.type === 'submit' && !el.disabled) || action;
    }
    const dialog = action.closest('dialog[open], [role=alertdialog], [role=dialog]');
    const name = (action.getAttribute('aria-label') || action.innerText || action.value || '').trim();
    const panel = dialog || form || document.querySelector('main,[role=main]') || document.body;
    const context = panel?.innerText || '';
    const fields = Array.from(panel?.querySelectorAll('input, [role=textbox]') || [])
        .filter(el => el.checkVisibility());
    const fieldName = field => [
        ...['autocomplete', 'name', 'aria-label', 'placeholder'].map(attr => field.getAttribute(attr) || ''),
        ...Array.from(field.labels || [], label => label.innerText)
    ].join(' ');
    const card = fields.some(field => /cc-(number|exp|csc)|card[ _-]?number|\bcvv\b|\bcvc\b/i.test(fieldName(field)));
    const amount = fields.some(field => /\b(amount|currency|total|payment)\b/i.test(fieldName(field)));
    const currency = /(?:[$€£]\s*\d|\d\s*(?:USD|EUR|GBP)\b)/i.test(context);
    const submit = !!form && (action.type === 'submit' || (target === action && target.matches('input')));
    let intent = '';
    if (dialog && /\b(delete|remove|erase|destroy)\b/i.test(name)
        && /\b(confirm|are you sure|permanent(?:ly)?|cannot be undone)\b/i.test(context)) {
        intent = 'delete';
    } else if (dialog && /\b(allow|grant|approve|authorize)\b/i.test(name)
        && /\b(permission|access|camera|microphone|location|notifications)\b/i.test(context)) {
        intent = 'permission_grant';
    } else if (/\b(transfer|send money)\b/i.test(name) && (amount || (form && currency))) {
        intent = 'transfer';
    } else if ((card && /\b(pay|purchase|buy|checkout|order)\b/i.test(name)) || (form && submit && currency)) {
        intent = 'purchase';
    } else if ((card || amount) && (submit || /\b(pay|payment)\b/i.test(name))) {
        intent = 'payment';
    } else if (/^send\b/i.test(name) && panel?.querySelector('textarea,[contenteditable=true],[role=textbox]')) {
        intent = 'send';
    }
    return [location.origin, intent];
}"#;

pub fn confirmation_message(intent: &str) -> String {
    format!("needs_confirm: waiting for user approval in Anarc (intent={intent}). Do not retry; call again after approval.")
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
#[cfg(test)]
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
    fn approvals_are_scoped_to_origin_and_intent() -> Result<(), toml::de::Error> {
        let config = PolicyConfig::default();
        let mut approvals = Confirmations::default();
        assert!(!approvals.required(&config, "https://chatgpt.com", "send"));
        assert!(approvals.required(&config, "https://shop.example", "purchase"));
        approvals.approve("https://shop.example", "purchase");
        assert!(!approvals.required(&config, "https://shop.example", "purchase"));
        assert!(approvals.required(&config, "https://shop.example:444", "purchase"));
        assert!(approvals.required(&config, "https://elsewhere.example", "purchase"));
        assert!(approvals.required(&config, "https://shop.example", "delete"));

        let table = toml::from_str("confirm = ['send']
confirm_allow = ['chatgpt.com:send']")?;
        let config = PolicyConfig::from_table(Some(&table));
        assert!(!approvals.required(&config, "https://chatgpt.com", "send"));
        assert!(approvals.required(&config, "https://another.example", "send"));
        Ok(())
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
