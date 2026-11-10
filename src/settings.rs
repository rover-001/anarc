fn zoom_file() -> std::path::PathBuf {
    crate::paths::data_dir().join("zoom.json")
}

use std::{
    collections::BTreeMap,
    sync::{OnceLock, RwLock},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchEngine {
    #[default]
    Google,
    Bing,
    DuckDuckGo,
}

impl SearchEngine {
    fn id(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Bing => "bing",
            Self::DuckDuckGo => "duck_duck_go",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Bing => "Bing",
            Self::DuckDuckGo => "DuckDuckGo",
        }
    }

    pub(crate) fn search_url(self, encoded_query: &str) -> String {
        search_engine_registry()
            .read()
            .expect("search engine registry lock poisoned")
            .url(self.id(), encoded_query)
            .unwrap_or_else(|| builtin_search_url(self.id(), encoded_query))
    }
}

fn builtin_search_url(id: &str, encoded_query: &str) -> String {
    let template = match id {
        "google" => "https://www.google.com/search?q=%s",
        "bing" => "https://www.bing.com/search?q=%s",
        "duck_duck_go" => "https://duckduckgo.com/?q=%s",
        _ => "https://www.google.com/search?q=%s",
    };
    template.replace("%s", encoded_query)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchEngineDefinition {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) url_template: String,
    pub(crate) keyword: Option<String>,
}

/// Search-engine definitions are registry entries; the persisted enum remains
/// as the stable id for existing browser-preferences.json files.
#[derive(Clone, Debug, Default)]
pub(crate) struct SearchEngineRegistry {
    entries: BTreeMap<String, SearchEngineDefinition>,
    plugin_owners: BTreeMap<String, String>,
}

impl SearchEngineRegistry {
    #[cfg(test)]
    pub(crate) fn builtins() -> Self {
        let mut registry = Self::default();
        for definition in [
            SearchEngineDefinition {
                id: "google".into(),
                label: "Google".into(),
                url_template: "https://www.google.com/search?q=%s".into(),
                keyword: None,
            },
            SearchEngineDefinition {
                id: "bing".into(),
                label: "Bing".into(),
                url_template: "https://www.bing.com/search?q=%s".into(),
                keyword: None,
            },
            SearchEngineDefinition {
                id: "duck_duck_go".into(),
                label: "DuckDuckGo".into(),
                url_template: "https://duckduckgo.com/?q=%s".into(),
                keyword: None,
            },
        ] {
            registry.register(definition);
        }
        registry
    }

    #[cfg(test)]
    pub(crate) fn register(&mut self, definition: SearchEngineDefinition) {
        self.entries.insert(definition.id.clone(), definition);
    }

    pub(crate) fn register_owned(
        &mut self,
        owner: &str,
        definition: SearchEngineDefinition,
    ) -> Result<(), String> {
        if self.entries.contains_key(&definition.id) {
            return Err(format!(
                "search engine id is already registered: {}",
                definition.id
            ));
        }
        if definition.keyword.as_deref().is_some_and(|keyword| {
            let keyword = normalize_engine_keyword(keyword);
            self.entries.values().any(|entry| {
                entry.keyword.as_deref().is_some_and(|existing| {
                    normalize_engine_keyword(existing).eq_ignore_ascii_case(keyword)
                })
            })
        }) {
            return Err("search engine keyword is already registered".into());
        }
        self.plugin_owners
            .insert(definition.id.clone(), owner.to_owned());
        self.entries.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub(crate) fn unregister_owner(&mut self, owner: &str) -> usize {
        let ids = self
            .plugin_owners
            .iter()
            .filter(|(_, registered)| registered.as_str() == owner)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in &ids {
            self.plugin_owners.remove(id);
            self.entries.remove(id);
        }
        ids.len()
    }

    pub(crate) fn get(&self, id: &str) -> Option<&SearchEngineDefinition> {
        self.entries.get(id)
    }

    pub(crate) fn url(&self, id: &str, encoded_query: &str) -> Option<String> {
        let definition = self.get(id)?;
        Some(definition.url_template.replace("%s", encoded_query))
    }

    pub(crate) fn url_for_keyword(&self, keyword: &str, encoded_query: &str) -> Option<String> {
        let keyword = normalize_engine_keyword(keyword);
        let definition = self.entries.values().find(|entry| {
            entry.keyword.as_deref().is_some_and(|registered| {
                normalize_engine_keyword(registered).eq_ignore_ascii_case(keyword)
            })
        })?;
        Some(definition.url_template.replace("%s", encoded_query))
    }
}

fn normalize_engine_keyword(keyword: &str) -> &str {
    keyword.strip_prefix('!').unwrap_or(keyword)
}

fn search_engine_registry() -> &'static RwLock<SearchEngineRegistry> {
    static REGISTRY: OnceLock<RwLock<SearchEngineRegistry>> = OnceLock::new();
    // Built-in definitions are owned by `lumen.search-engines` so they can be
    // removed on disable. `SearchEngine::search_url` retains static fallbacks
    // for old preference values when that plugin is disabled.
    REGISTRY.get_or_init(|| RwLock::new(SearchEngineRegistry::default()))
}

#[cfg(test)]
pub(crate) fn custom_search_url(id: &str, encoded_query: &str) -> Option<String> {
    search_engine_registry().read().ok()?.url(id, encoded_query)
}

pub(crate) fn custom_search_url_for_keyword(keyword: &str, encoded_query: &str) -> Option<String> {
    search_engine_registry()
        .read()
        .ok()?
        .url_for_keyword(keyword, encoded_query)
}

pub(crate) fn register_owned_search_engine(
    owner: &str,
    definition: SearchEngineDefinition,
) -> Result<(), String> {
    search_engine_registry()
        .write()
        .map_err(|_| "search engine registry lock poisoned".to_string())?
        .register_owned(owner, definition)
}

pub(crate) fn unregister_owned_search_engines(owner: &str) -> usize {
    search_engine_registry()
        .write()
        .map(|mut registry| registry.unregister_owner(owner))
        .unwrap_or_default()
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

    #[test]
    fn legacy_search_engine_values_resolve_through_the_builtin_registry() {
        let registry = SearchEngineRegistry::builtins();
        for (engine, serialized, label, expected_url) in [
            (
                SearchEngine::Google,
                "google",
                "Google",
                "https://www.google.com/search?q=rust",
            ),
            (
                SearchEngine::Bing,
                "bing",
                "Bing",
                "https://www.bing.com/search?q=rust",
            ),
            (
                SearchEngine::DuckDuckGo,
                "duck_duck_go",
                "DuckDuckGo",
                "https://duckduckgo.com/?q=rust",
            ),
        ] {
            assert_eq!(
                serde_json::to_string(&engine).unwrap(),
                format!("\"{serialized}\"")
            );
            let definition = registry.get(serialized).unwrap();
            assert_eq!(definition.label, label);
            assert_eq!(engine.search_url("rust"), expected_url);
            let saved = format!(r#"{{"search_engine":"{serialized}"}}"#);
            assert_eq!(
                serde_json::from_str::<BrowserPreferences>(&saved)
                    .unwrap()
                    .search_engine,
                engine
            );
        }
    }

    #[test]
    fn registry_accepts_additional_template_entries() {
        let mut registry = SearchEngineRegistry::builtins();
        registry.register(SearchEngineDefinition {
            id: "github".into(),
            label: "GitHub".into(),
            url_template: "https://github.com/search?q=%s".into(),
            keyword: Some("!gh".into()),
        });
        assert_eq!(
            registry.url("github", "tabs%20rust").as_deref(),
            Some("https://github.com/search?q=tabs%20rust")
        );
        assert_eq!(
            registry.get("github").unwrap().keyword.as_deref(),
            Some("!gh")
        );
    }

    #[test]
    fn pack_search_engine_registration_is_owner_scoped_and_disposable() {
        let id = format!("pack-test-{}", std::process::id());
        let owner = "example.test-pack";
        register_owned_search_engine(
            owner,
            SearchEngineDefinition {
                id: id.clone(),
                label: "Test Pack Search".into(),
                url_template: "https://example.test/?q=%s".into(),
                keyword: Some("!test".into()),
            },
        )
        .unwrap();
        assert_eq!(
            custom_search_url(&id, "hello").as_deref(),
            Some("https://example.test/?q=hello")
        );
        assert_eq!(unregister_owned_search_engines("another.pack"), 0);
        assert!(custom_search_url(&id, "hello").is_some());
        assert_eq!(unregister_owned_search_engines(owner), 1);
        assert!(custom_search_url(&id, "hello").is_none());
    }
}
