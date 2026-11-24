use super::*;

impl Anarc {
    /// Publish the focused tab to Discord. Called from the existing UI loop so
    /// activation and navigation changes are picked up without extra timers.
    pub(crate) fn update_discord_presence(&self) {
        let Some(service) = self.plugins.service(crate::presence::PRESENCE.clone()) else {
            return;
        };
        let tab_id = self.focused_tab_id();
        let Some(tab) = self.active_space().tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let details = one_line(&tab.title);
        let details = if details.is_empty() {
            "Browsing in Anarc".to_owned()
        } else {
            truncate(details)
        };
        let host = crate::settings::url_host(&tab.url);
        let state = if host.is_empty() {
            "In Anarc".to_owned()
        } else {
            truncate(host)
        };
        service.update(crate::presence::PresenceUpdate { details, state });
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(value: String) -> String {
    if value.chars().count() <= 128 {
        return value;
    }
    let mut short: String = value.chars().take(125).collect();
    short.push_str("...");
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_text_is_single_line_and_bounded() {
        assert_eq!(one_line("  Hello\n world\t"), "Hello world");
        assert_eq!(truncate("x".repeat(200)).chars().count(), 128);
        assert_eq!(truncate("short".into()), "short");
    }
}
