#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SuggestionItem {
    pub icon_type: String,
    pub title: String,
    pub target: String,
}

pub(crate) fn compute_suggestions(query: &str) -> Vec<SuggestionItem> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }

    let mut list = Vec::new();

    // 1. Google search action
    list.push(SuggestionItem {
        icon_type: "search".to_string(),
        title: format!("Search Google for \"{q}\""),
        target: q.to_string(),
    });

    // 2. Direct domain navigation if not already a full URL
    if !q.contains("://") {
        let direct_domain = if q.contains('.') && !q.contains(' ') {
            format!("https://{q}")
        } else if !q.contains(' ') {
            format!("https://www.{q}.com")
        } else {
            String::new()
        };
        if !direct_domain.is_empty() {
            list.push(SuggestionItem {
                icon_type: "globe".to_string(),
                title: format!("Visit {direct_domain}"),
                target: direct_domain,
            });
        }
    }

    // 3. Matched quick sites and keywords
    let q_lower = q.to_lowercase();
    const POPULAR_SITES: &[(&str, &str, &str)] = &[
        ("youtube", "YouTube", "https://youtube.com"),
        ("github", "GitHub", "https://github.com"),
        ("google", "Google", "https://google.com"),
        ("gmail", "Gmail", "https://mail.google.com"),
        ("reddit", "Reddit", "https://reddit.com"),
        ("twitter", "Twitter / X", "https://x.com"),
        ("chatgpt", "ChatGPT", "https://chatgpt.com"),
        ("claude", "Claude AI", "https://claude.ai"),
        ("wikipedia", "Wikipedia", "https://wikipedia.org"),
        ("amazon", "Amazon", "https://amazon.com"),
        ("netflix", "Netflix", "https://netflix.com"),
        ("spotify", "Spotify", "https://open.spotify.com"),
        (
            "rust",
            "Rust Programming Language",
            "https://www.rust-lang.org",
        ),
        ("crates", "Crates.io", "https://crates.io"),
        (
            "stackoverflow",
            "Stack Overflow",
            "https://stackoverflow.com",
        ),
        ("discord", "Discord", "https://discord.com"),
        ("twitch", "Twitch", "https://twitch.tv"),
        ("notion", "Notion", "https://notion.so"),
        ("figma", "Figma", "https://figma.com"),
        ("news", "Google News", "https://news.google.com"),
        ("maps", "Google Maps", "https://maps.google.com"),
    ];

    for (slug, name, url) in POPULAR_SITES {
        if slug.starts_with(&q_lower) || name.to_lowercase().contains(&q_lower) {
            list.push(SuggestionItem {
                icon_type: "zap".to_string(),
                title: format!("{name} — {url}"),
                target: url.to_string(),
            });
        }
    }

    // 4. Common query completions
    if list.len() < 6 && !q.contains('.') {
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} official website"),
            target: format!("{q} official website"),
        });
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} login"),
            target: format!("{q} login"),
        });
        list.push(SuggestionItem {
            icon_type: "search".to_string(),
            title: format!("{q} documentation"),
            target: format!("{q} documentation"),
        });
    }

    list.truncate(6);
    list
}

pub(crate) fn move_suggestion_selection(
    current: Option<usize>,
    count: usize,
    down: bool,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, down) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(index), true) => (index + 1) % count,
        (Some(0), false) => count - 1,
        (Some(index), false) => index.saturating_sub(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_suggestions_produces_smart_search_and_completions() {
        // Empty query gives empty list
        assert!(compute_suggestions("").is_empty());
        assert!(compute_suggestions("   ").is_empty());

        // Typing "you" provides search, domain completion, and YouTube quick site
        let you_sugs = compute_suggestions("you");
        assert!(!you_sugs.is_empty());
        assert_eq!(you_sugs[0].title, "Search Google for \"you\"");
        assert!(
            you_sugs
                .iter()
                .any(|s| s.title.contains("https://www.you.com"))
        );
        assert!(
            you_sugs
                .iter()
                .any(|s| s.title.contains("YouTube") && s.target == "https://youtube.com")
        );

        // Typing "git" provides search and GitHub
        let git_sugs = compute_suggestions("git");
        assert!(
            git_sugs
                .iter()
                .any(|s| s.title.contains("GitHub") && s.target == "https://github.com")
        );

        // Direct domain query produces clean domain navigation
        let domain_sugs = compute_suggestions("example.com");
        assert!(
            domain_sugs
                .iter()
                .any(|s| s.target == "https://example.com")
        );
    }
}
