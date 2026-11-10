fn zoom_file() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".local/share/lumen/zoom.json")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchEngine {
    #[default]
    Google,
    Bing,
    DuckDuckGo,
}

impl SearchEngine {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Bing => "Bing",
            Self::DuckDuckGo => "DuckDuckGo",
        }
    }

    pub(crate) fn search_url(self, encoded_query: &str) -> String {
        let base = match self {
            Self::Google => "https://www.google.com/search?q=",
            Self::Bing => "https://www.bing.com/search?q=",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=",
        };
        format!("{base}{encoded_query}")
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct BrowserPreferences {
    pub(crate) default_zoom_factor: f64,
    pub(crate) search_engine: SearchEngine,
    #[serde(default = "default_ad_blocker_enabled")]
    pub(crate) ad_blocker_enabled: bool,
}

fn default_ad_blocker_enabled() -> bool {
    true
}

impl Default for BrowserPreferences {
    fn default() -> Self {
        Self {
            default_zoom_factor: crate::ui::visual::DEFAULT_PAGE_ZOOM_FACTOR,
            search_engine: SearchEngine::Google,
            ad_blocker_enabled: true,
        }
    }
}

pub(crate) fn load_preferences() -> BrowserPreferences {
    let path = crate::paths::data_dir().join("browser-preferences.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BrowserPreferences>(&bytes).ok())
        .map(|mut preferences| {
            if !preferences.default_zoom_factor.is_finite() {
                preferences.default_zoom_factor = crate::ui::visual::DEFAULT_PAGE_ZOOM_FACTOR;
            }
            preferences.default_zoom_factor = preferences.default_zoom_factor.clamp(0.25, 5.0);
            preferences
        })
        .unwrap_or_default()
}

pub(crate) fn save_preferences(preferences: &BrowserPreferences) {
    let path = crate::paths::data_dir().join("browser-preferences.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(preferences) {
        let _ = std::fs::write(path, bytes);
    }
}

pub(crate) fn load_zooms() -> std::collections::HashMap<String, f64> {
    std::fs::read_to_string(zoom_file())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub(crate) fn save_zooms(zooms: &std::collections::HashMap<String, f64>) {
    let path = zoom_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_vec_pretty(zooms) {
        let _ = std::fs::write(path, contents);
    }
}

pub(crate) fn url_host(url: &str) -> String {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return String::new();
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return String::new();
    }
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map(|end| &authority[..=end])
            .unwrap_or(authority)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if port.chars().all(|ch| ch.is_ascii_digit()) {
            host
        } else {
            authority
        }
    } else {
        authority
    };
    host.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_keyed_by_site_host() {
        assert_eq!(url_host("https://Example.com/path?q=1"), "example.com");
        assert_eq!(url_host("http://example.com:8080/path"), "example.com");
        assert_eq!(
            url_host("https://user:pass@example.com/path"),
            "example.com"
        );
        assert_eq!(url_host("about:blank"), "");
        assert_eq!(url_host("lumen://welcome"), "");
    }

    #[test]
    fn ad_blocker_is_enabled_by_default_and_old_preferences_keep_it_enabled() {
        assert!(BrowserPreferences::default().ad_blocker_enabled);
        let old =
            serde_json::from_str::<BrowserPreferences>(r#"{"search_engine":"google"}"#).unwrap();
        assert!(old.ad_blocker_enabled);
    }
}
