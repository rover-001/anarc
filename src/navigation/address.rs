use crate::model::{Space, Tab, TabState};
use crate::settings::SearchEngine;

fn encode_search_query(query: &str) -> String {
    let mut encoded = String::new();
    for byte in query.bytes() {
        match byte {
            b' ' => encoded.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(crate) fn normalize_address_with_engine(input: &str, search_engine: SearchEngine) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if input.starts_with("http://") {
        return crate::shield::upgrade_http_url(input).unwrap_or_else(|| input.to_owned());
    }
    if input.contains("://") || input.starts_with("about:") {
        return input.to_owned();
    }
    if let Some(url) = keyword_search_url(input) {
        return url;
    }
    if input == "localhost"
        || input.starts_with("localhost:")
        || (!input.contains(char::is_whitespace) && input.contains('.'))
    {
        return format!("https://{input}");
    }

    let lower = input.to_lowercase();
    match lower.as_str() {
        "youtube" => "https://youtube.com".to_string(),
        "google" => "https://google.com".to_string(),
        "github" => "https://github.com".to_string(),
        "reddit" => "https://reddit.com".to_string(),
        "twitter" | "x" => "https://x.com".to_string(),
        "wikipedia" => "https://wikipedia.org".to_string(),
        "amazon" => "https://amazon.com".to_string(),
        "netflix" => "https://netflix.com".to_string(),
        "spotify" => "https://open.spotify.com".to_string(),
        "chatgpt" => "https://chatgpt.com".to_string(),
        "claude" => "https://claude.ai".to_string(),
        "twitch" => "https://twitch.tv".to_string(),
        "discord" => "https://discord.com".to_string(),
        "figma" => "https://figma.com".to_string(),
        "notion" => "https://notion.so".to_string(),
        "stackoverflow" => "https://stackoverflow.com".to_string(),
        _ => search_engine.search_url(&encode_search_query(input)),
    }
}

fn keyword_search_url(input: &str) -> Option<String> {
    let input = input.trim();
    let mut parts = input.splitn(2, char::is_whitespace);
    let keyword = parts.next()?.strip_prefix('!')?;
    let query = parts.next()?.trim();
    if keyword.is_empty() || query.is_empty() {
        return None;
    }
    crate::settings::custom_search_url_for_keyword(keyword, &encode_search_query(query))
}

pub(crate) fn submit_address_with_engine(
    space: &mut Space,
    input: &str,
    search_engine: SearchEngine,
) -> Option<(String, String, bool)> {
    let http_first = input.trim().starts_with("http://")
        && crate::shield::upgrade_http_url(input.trim()).is_some();
    let url = normalize_address_with_engine(input, search_engine);
    let url = if url.is_empty() {
        "https://www.google.com".to_owned()
    } else {
        url
    };

    if space.active.is_none() {
        let id = format!("tab-{}", space.tabs.len() + 1);
        space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
        space.active = Some(id);
    }
    let id = space.active.clone()?;
    let tab = space.tabs.iter_mut().find(|tab| tab.id == id)?;
    let is_custom_keyword_search = keyword_search_url(input).is_some();
    let title = if is_custom_keyword_search
        || url.starts_with("https://www.google.com/search?q=")
        || url.starts_with("https://google.com/search?q=")
        || url.starts_with("https://www.bing.com/search?q=")
        || url.starts_with("https://duckduckgo.com/?q=")
    {
        format!("Search: {}", input.trim())
    } else if let Some(domain) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let domain = domain.split('/').next().unwrap_or(domain);
        let domain = domain.strip_prefix("www.").unwrap_or(domain);
        domain.to_string()
    } else {
        url.clone()
    };
    tab.url = url.clone();
    tab.title = title;
    tab.state = TabState::Live;
    Some((id, url, http_first))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize_address(input: &str) -> String {
        normalize_address_with_engine(input, SearchEngine::default())
    }

    fn submit_address(space: &mut Space, input: &str) -> Option<(String, String, bool)> {
        submit_address_with_engine(space, input, SearchEngine::default())
    }
    use crate::model::{Space, Tab, TabState};

    #[test]
    fn address_bar_routes_urls_and_searches_to_the_active_tab() {
        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let tab = Tab::new("existing", "Welcome", "lumen://welcome");
        space.active = Some(tab.id.clone());
        space.tabs.push(tab);

        // Smart domain navigation for top keywords
        let (id, url, _) = submit_address(&mut space, "youtube").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://youtube.com");
        assert_eq!(space.tabs[0].url, url);
        assert_eq!(space.tabs[0].title, "youtube.com");
        assert_eq!(space.tabs[0].state, TabState::Live);

        // General search query routes to Google search with clean title
        let (id, url, _) = submit_address(&mut space, "rust compiler docs").unwrap();
        assert_eq!(id, "existing");
        assert_eq!(url, "https://www.google.com/search?q=rust+compiler+docs");
        assert_eq!(space.tabs[0].title, "Search: rust compiler docs");
    }

    #[test]
    fn empty_address_opens_search_home_and_creates_a_tab_if_needed() {
        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let (id, url, _) = submit_address(&mut space, "").unwrap();
        assert_eq!(id, "tab-1");
        assert_eq!(url, "https://www.google.com");
        assert_eq!(space.active.as_deref(), Some("tab-1"));
        assert_eq!(space.tabs[0].url, url);
    }

    #[test]
    fn address_normalization_handles_urls_domains_and_search_queries() {
        crate::shield::set_plugin_enabled(true);
        assert_eq!(normalize_address(""), "");
        assert_eq!(
            normalize_address("https://news.ycombinator.com"),
            "https://news.ycombinator.com"
        );
        assert_eq!(normalize_address("about:blank"), "about:blank");
        assert_eq!(normalize_address("github.com"), "https://github.com");
        assert_eq!(
            normalize_address("http://example.com/x"),
            "https://example.com/x"
        );
        assert_eq!(
            normalize_address("http://localhost:8080/x"),
            "http://localhost:8080/x"
        );
        assert_eq!(
            normalize_address("localhost:8080"),
            "https://localhost:8080"
        );
        assert_eq!(
            normalize_address("what is the airspeed velocity of a swallow"),
            "https://www.google.com/search?q=what+is+the+airspeed+velocity+of+a+swallow"
        );
        assert_eq!(
            normalize_address("cats & dogs"),
            "https://www.google.com/search?q=cats+%26+dogs"
        );
    }

    #[test]
    fn registered_custom_search_keywords_route_queries_and_unmount_cleanly() {
        let keyword = format!("lumenphase3{}", std::process::id());
        let owner = "example.address-pack";
        crate::settings::register_owned_search_engine(
            owner,
            crate::settings::SearchEngineDefinition {
                id: format!("test-search-{}", std::process::id()),
                label: "Address Test".into(),
                url_template: "https://search.example/?q=%s".into(),
                keyword: Some(format!("!{keyword}")),
            },
        )
        .unwrap();

        let mut space = Space::new("test", "Test", "compass", 0x123456);
        let (_, url, _) = submit_address_with_engine(
            &mut space,
            &format!("!{keyword} two words & more"),
            SearchEngine::default(),
        )
        .unwrap();
        assert_eq!(url, "https://search.example/?q=two+words+%26+more");
        assert_eq!(
            space.tabs[0].title,
            format!("Search: !{keyword} two words & more")
        );

        assert_eq!(crate::settings::unregister_owned_search_engines(owner), 1);
        assert_eq!(
            normalize_address_with_engine(
                &format!("!{keyword} two words"),
                SearchEngine::default()
            ),
            format!("https://www.google.com/search?q=%21{keyword}+two+words")
        );
    }
}
