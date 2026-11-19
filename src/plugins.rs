//! Toggleable built-in feature plugins.

use crate::plugin::{
    Manifest, Plugin, PluginConfig, PluginCtx, PluginDescriptor, PluginError, PluginHost,
    PluginKind,
};
use std::sync::atomic::{AtomicBool, Ordering};

static IMPORT_ENABLED: AtomicBool = AtomicBool::new(false);
static DOWNLOADS_ENABLED: AtomicBool = AtomicBool::new(false);
static SPLIT_VIEW_ENABLED: AtomicBool = AtomicBool::new(false);

struct BuiltinPlugin {
    manifest: Manifest,
    feature: BuiltinFeature,
}

#[derive(Clone, Copy)]
enum BuiltinFeature {
    Reader,
    Import,
    History,
    Lifecycle,
    Shield,
    Downloads,
    SplitView,
    SearchEngines,
}

impl BuiltinPlugin {
    fn new(id: &'static str, feature: BuiltinFeature) -> Self {
        Self {
            manifest: Manifest {
                id: id.to_owned(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: Vec::new(),
            },
            feature,
        }
    }

    fn set_enabled(feature: BuiltinFeature, enabled: bool) {
        match feature {
            BuiltinFeature::Reader => crate::reader::set_plugin_enabled(enabled),
            BuiltinFeature::Import => IMPORT_ENABLED.store(enabled, Ordering::Relaxed),
            BuiltinFeature::History => crate::history::set_plugin_enabled(enabled),
            BuiltinFeature::Lifecycle => crate::lifecycle::set_plugin_enabled(enabled),
            BuiltinFeature::Shield => crate::shield::set_plugin_enabled(enabled),
            BuiltinFeature::Downloads => DOWNLOADS_ENABLED.store(enabled, Ordering::Relaxed),
            BuiltinFeature::SplitView => SPLIT_VIEW_ENABLED.store(enabled, Ordering::Relaxed),
            BuiltinFeature::SearchEngines => {}
        }
    }

    fn register_search_engines() -> Result<(), PluginError> {
        let owner = "lumen.search-engines";
        let definitions = [
            ("google", "Google", "https://www.google.com/search?q=%s"),
            ("bing", "Bing", "https://www.bing.com/search?q=%s"),
            ("duck_duck_go", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
        ];
        let mut registered = Vec::new();
        for (id, label, url_template) in definitions {
            let definition = crate::settings::SearchEngineDefinition {
                id: id.to_owned(),
                label: label.to_owned(),
                url_template: url_template.to_owned(),
                keyword: None,
            };
            if let Err(error) = crate::settings::register_owned_search_engine(owner, definition) {
                crate::settings::unregister_owned_search_engines(owner);
                return Err(PluginError(error));
            }
            registered.push(id);
        }
        debug_assert_eq!(registered.len(), 3);
        Ok(())
    }
}

impl Plugin for BuiltinPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let feature = self.feature;
        if matches!(feature, BuiltinFeature::SearchEngines) {
            Self::register_search_engines()?;
        }
        Self::set_enabled(feature, true);
        ctx.effect(move || {
            if matches!(feature, BuiltinFeature::SearchEngines) {
                crate::settings::unregister_owned_search_engines("lumen.search-engines");
            }
            Self::set_enabled(feature, false);
        });
        Ok(())
    }
}

fn descriptor(id: &str, name: &str, description: &str) -> PluginDescriptor {
    PluginDescriptor {
        id: id.to_owned(),
        name: name.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        kind: PluginKind::Builtin,
        description: description.to_owned(),
        permissions: Vec::new(),
        enabled_by_default: true,
        can_toggle: true,
    }
}

/// Register implemented built-ins with the host.
pub(crate) fn register(host: &mut PluginHost) {
    host.register(
        descriptor("lumen.reader", "Reader mode", "Simplified article view"),
        || Box::new(BuiltinPlugin::new("lumen.reader", BuiltinFeature::Reader)),
    );
    host.register(
        descriptor(
            "lumen.import",
            "Browser import",
            "Import bookmarks and history",
        ),
        || Box::new(BuiltinPlugin::new("lumen.import", BuiltinFeature::Import)),
    );
    for (id, name, description, feature) in [
        (
            "lumen.history",
            "History",
            "Visited-page history and address suggestions",
            BuiltinFeature::History,
        ),
        (
            "lumen.lifecycle",
            "Tab lifecycle",
            "Discard inactive browser tabs after a period of inactivity",
            BuiltinFeature::Lifecycle,
        ),
        (
            "lumen.shield",
            "Site protection",
            "EasyList filtering and HTTPS-first navigation",
            BuiltinFeature::Shield,
        ),
        (
            "lumen.downloads",
            "Downloads page",
            "Show active and completed downloads",
            BuiltinFeature::Downloads,
        ),
        (
            "lumen.split-view",
            "Split view",
            "View two tabs side by side",
            BuiltinFeature::SplitView,
        ),
        (
            "lumen.search-engines",
            "Search engines",
            "Built-in search engine registry",
            BuiltinFeature::SearchEngines,
        ),
    ] {
        let plugin_id = id;
        host.register(descriptor(id, name, description), move || {
            Box::new(BuiltinPlugin::new(plugin_id, feature))
        });
    }
    let safe_mode = host.is_safe_mode();
    let config = host.config();
    for (id, feature) in [
        ("lumen.reader", BuiltinFeature::Reader),
        ("lumen.import", BuiltinFeature::Import),
        ("lumen.history", BuiltinFeature::History),
        ("lumen.lifecycle", BuiltinFeature::Lifecycle),
        ("lumen.shield", BuiltinFeature::Shield),
        ("lumen.downloads", BuiltinFeature::Downloads),
        ("lumen.split-view", BuiltinFeature::SplitView),
        ("lumen.search-engines", BuiltinFeature::SearchEngines),
    ] {
        BuiltinPlugin::set_enabled(feature, !safe_mode && enabled_in_config(config, id));
    }
}

/// The CLI import command runs before the app host is constructed, so consult
/// the persisted selection directly and retain today's enabled default.
pub(crate) fn import_enabled_from_config() -> bool {
    if std::env::args().any(|argument| argument == "--safe-mode") {
        return false;
    }
    let config = PluginConfig::load().unwrap_or_default();
    let history_enabled = enabled_in_config(&config, "lumen.history");
    crate::history::set_plugin_enabled(history_enabled);
    import_enabled(&config)
}

fn enabled_in_config(config: &PluginConfig, id: &str) -> bool {
    config
        .plugins
        .get(id)
        .and_then(|entry| entry.enabled)
        .unwrap_or(true)
}

fn import_enabled(config: &PluginConfig) -> bool {
    enabled_in_config(config, "lumen.import")
}

pub(crate) fn split_view_enabled() -> bool {
    SPLIT_VIEW_ENABLED.load(Ordering::Relaxed)
}

pub(crate) fn downloads_enabled() -> bool {
    DOWNLOADS_ENABLED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::Kernel;

    #[test]
    fn feature_effects_disable_and_reenable_with_plugin_lifecycle() {
        let mut kernel = Kernel::new();
        let builtins = [
            ("lumen.reader", BuiltinFeature::Reader),
            ("lumen.import", BuiltinFeature::Import),
            ("lumen.history", BuiltinFeature::History),
            ("lumen.lifecycle", BuiltinFeature::Lifecycle),
            ("lumen.shield", BuiltinFeature::Shield),
            ("lumen.downloads", BuiltinFeature::Downloads),
            ("lumen.split-view", BuiltinFeature::SplitView),
            ("lumen.search-engines", BuiltinFeature::SearchEngines),
        ];
        for (id, feature) in builtins {
            kernel
                .mount(Box::new(BuiltinPlugin::new(id, feature)))
                .unwrap();
        }

        assert!(crate::reader::is_plugin_enabled());
        assert!(IMPORT_ENABLED.load(Ordering::Relaxed));
        assert!(crate::history::plugin_enabled());
        assert!(downloads_enabled());
        assert!(split_view_enabled());
        assert!(crate::shield::upgrade_http_url("http://example.com").is_some());
        assert_eq!(
            crate::settings::custom_search_url("google", "query").as_deref(),
            Some("https://www.google.com/search?q=query")
        );

        for (id, _) in builtins {
            kernel.unmount(id);
        }
        assert!(!crate::reader::is_plugin_enabled());
        assert!(!IMPORT_ENABLED.load(Ordering::Relaxed));
        assert!(!crate::history::plugin_enabled());
        assert!(!downloads_enabled());
        assert!(!split_view_enabled());
        assert!(crate::shield::upgrade_http_url("http://example.com").is_none());
        let start = std::time::Instant::now();
        let mut lifecycle = crate::lifecycle::TabLifecycle::default();
        lifecycle.opened("inactive", start);
        assert!(
            lifecycle
                .discard_expired_many(start + crate::lifecycle::DISCARD_AFTER, &Vec::new())
                .is_empty()
        );
        assert_eq!(crate::settings::custom_search_url("google", "query"), None);
        assert_eq!(
            crate::settings::SearchEngine::Google.search_url("query"),
            "https://www.google.com/search?q=query"
        );

        for (id, feature) in builtins {
            kernel
                .mount(Box::new(BuiltinPlugin::new(id, feature)))
                .unwrap();
        }
        assert!(crate::reader::is_plugin_enabled());
        assert!(IMPORT_ENABLED.load(Ordering::Relaxed));
        assert!(crate::history::plugin_enabled());
        assert!(downloads_enabled());
        assert!(split_view_enabled());
        assert!(crate::shield::upgrade_http_url("http://example.com").is_some());
        assert_eq!(
            lifecycle.discard_expired_many(start + crate::lifecycle::DISCARD_AFTER, &Vec::new()),
            vec!["inactive"]
        );
    }

    #[test]
    fn import_default_is_on_but_persisted_disable_is_respected() {
        let mut config = PluginConfig::default();
        assert!(import_enabled(&config));
        config
            .plugins
            .entry("lumen.import".into())
            .or_default()
            .enabled = Some(false);
        assert!(!import_enabled(&config));
    }

    #[test]
    fn safe_mode_forces_builtin_feature_flags_off_before_mounting() {
        let mut host = PluginHost::new(PluginConfig::default());
        host.set_safe_mode(true);
        host.mount_core().unwrap();
        register(&mut host);
        host.mount_enabled().unwrap();

        assert!(!crate::reader::is_plugin_enabled());
        assert!(!crate::history::plugin_enabled());
        assert!(!downloads_enabled());
        assert!(!split_view_enabled());
        assert!(!crate::shield::upgrade_http_url("http://example.com").is_some());
        assert_eq!(crate::settings::custom_search_url("google", "query"), None);
        assert!(!host.is_active("lumen.import"));

        // The feature switches are process-wide because browser callbacks read
        // them on worker threads; restore the test process to its default set.
        for feature in [
            BuiltinFeature::Reader,
            BuiltinFeature::Import,
            BuiltinFeature::History,
            BuiltinFeature::Lifecycle,
            BuiltinFeature::Shield,
            BuiltinFeature::Downloads,
            BuiltinFeature::SplitView,
            BuiltinFeature::SearchEngines,
        ] {
            BuiltinPlugin::set_enabled(feature, true);
        }
        BuiltinPlugin::register_search_engines().unwrap();
    }
}
