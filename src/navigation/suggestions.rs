use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SuggestionItem {
    pub icon_type: String,
    pub title: String,
    pub target: String,
}

struct PopularSite {
    slug: &'static str,
    name_lower: &'static str,
    url: &'static str,
    title: &'static str,
}

const POPULAR_SITES: &[PopularSite] = &[
    PopularSite {
        slug: "youtube",
        name_lower: "youtube",
        url: "https://youtube.com",
        title: "YouTube — https://youtube.com",
    },
    PopularSite {
        slug: "github",
        name_lower: "github",
        url: "https://github.com",
        title: "GitHub — https://github.com",
    },
    PopularSite {
        slug: "google",
        name_lower: "google",
        url: "https://google.com",
        title: "Google — https://google.com",
    },
    PopularSite {
        slug: "gmail",
        name_lower: "gmail",
        url: "https://mail.google.com",
        title: "Gmail — https://mail.google.com",
    },
    PopularSite {
        slug: "reddit",
        name_lower: "reddit",
        url: "https://reddit.com",
        title: "Reddit — https://reddit.com",
    },
    PopularSite {
        slug: "twitter",
        name_lower: "twitter / x",
        url: "https://x.com",
        title: "Twitter / X — https://x.com",
    },
    PopularSite {
        slug: "chatgpt",
        name_lower: "chatgpt",
        url: "https://chatgpt.com",
        title: "ChatGPT — https://chatgpt.com",
    },
    PopularSite {
        slug: "claude",
        name_lower: "claude ai",
        url: "https://claude.ai",
        title: "Claude AI — https://claude.ai",
    },
    PopularSite {
        slug: "wikipedia",
        name_lower: "wikipedia",
        url: "https://wikipedia.org",
        title: "Wikipedia — https://wikipedia.org",
    },
    PopularSite {
        slug: "amazon",
        name_lower: "amazon",
        url: "https://amazon.com",
        title: "Amazon — https://amazon.com",
    },
    PopularSite {
        slug: "netflix",
        name_lower: "netflix",
        url: "https://netflix.com",
        title: "Netflix — https://netflix.com",
    },
    PopularSite {
        slug: "spotify",
        name_lower: "spotify",
        url: "https://open.spotify.com",
        title: "Spotify — https://open.spotify.com",
    },
    PopularSite {
        slug: "rust",
        name_lower: "rust programming language",
        url: "https://www.rust-lang.org",
        title: "Rust Programming Language — https://www.rust-lang.org",
    },
    PopularSite {
        slug: "crates",
        name_lower: "crates.io",
        url: "https://crates.io",
        title: "Crates.io — https://crates.io",
    },
    PopularSite {
        slug: "stackoverflow",
        name_lower: "stack overflow",
        url: "https://stackoverflow.com",
        title: "Stack Overflow — https://stackoverflow.com",
    },
    PopularSite {
        slug: "discord",
        name_lower: "discord",
        url: "https://discord.com",
        title: "Discord — https://discord.com",
    },
    PopularSite {
        slug: "twitch",
        name_lower: "twitch",
        url: "https://twitch.tv",
        title: "Twitch — https://twitch.tv",
    },
    PopularSite {
        slug: "notion",
        name_lower: "notion",
        url: "https://notion.so",
        title: "Notion — https://notion.so",
    },
    PopularSite {
        slug: "figma",
        name_lower: "figma",
        url: "https://figma.com",
        title: "Figma — https://figma.com",
    },
    PopularSite {
        slug: "news",
        name_lower: "google news",
        url: "https://news.google.com",
        title: "Google News — https://news.google.com",
    },
    PopularSite {
        slug: "maps",
        name_lower: "google maps",
        url: "https://maps.google.com",
        title: "Google Maps — https://maps.google.com",
    },
];

static LAST_QUERY_CACHE: Mutex<Option<(String, Vec<SuggestionItem>)>> = Mutex::new(None);

pub(crate) fn compute_suggestions(query: &str) -> Vec<SuggestionItem> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }

    if let Ok(guard) = LAST_QUERY_CACHE.lock()
        && let Some((last_q, last_results)) = &*guard
        && last_q == q
    {
        return last_results.clone();
    }

    let mut list = Vec::with_capacity(6);

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

    // 3. Matched quick sites and keywords (zero allocations in loop)
    let q_lower = q.to_lowercase();
    for site in POPULAR_SITES {
        if site.slug.starts_with(&q_lower) || site.name_lower.contains(&q_lower) {
            list.push(SuggestionItem {
                icon_type: "zap".to_string(),
                title: site.title.to_string(),
                target: site.url.to_string(),
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

    if let Ok(mut guard) = LAST_QUERY_CACHE.lock() {
        *guard = Some((q.to_string(), list.clone()));
    }

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

        // Repeated/unchanged query returns equal result
        let domain_sugs_cached = compute_suggestions("example.com");
        assert_eq!(domain_sugs, domain_sugs_cached);
    }
}
