//! Kernel adapter for safe, declarative packs.

use super::packs::{LayoutPack, LoadedPack, RunAt, ThemePack};
use super::{
    Manifest, Order, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind,
};
use crate::settings::SearchEngineDefinition;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MatchedPageAssetKind {
    Userstyle,
    Userscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchedPageAsset {
    pub(crate) plugin_id: String,
    pub(crate) kind: MatchedPageAssetKind,
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) run_at: RunAt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveLocalFilterList {
    pub(crate) plugin_id: String,
    pub(crate) path: PathBuf,
    pub(crate) source: String,
}

fn active_packs() -> &'static RwLock<BTreeMap<String, LoadedPack>> {
    static ACTIVE: OnceLock<RwLock<BTreeMap<String, LoadedPack>>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

/// Return script/style source for active packs whose rules match this URL.
/// Called by the GPUI-thread `page.load_end` dispatcher; this does no JS
/// execution itself.
pub(crate) fn matched_page_assets(url: &str) -> Vec<MatchedPageAsset> {
    let Ok(active) = active_packs().read() else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for (plugin_id, loaded) in active.iter() {
        for style in &loaded.manifest.pack.userstyle {
            if super::packs::PackManifest::matches_url(&style.matches, &style.exclude, url)
                && let Ok(path) = loaded.asset_path(&style.css)
                && let Ok(source) = fs::read_to_string(&path)
            {
                matches.push(MatchedPageAsset {
                    plugin_id: plugin_id.clone(),
                    kind: MatchedPageAssetKind::Userstyle,
                    path,
                    source,
                    run_at: style.run_at,
                });
            }
        }
        for script in &loaded.manifest.pack.userscript {
            if super::packs::PackManifest::matches_url(&script.matches, &script.exclude, url)
                && let Ok(path) = loaded.asset_path(&script.js)
                && let Ok(source) = fs::read_to_string(&path)
            {
                matches.push(MatchedPageAsset {
                    plugin_id: plugin_id.clone(),
                    kind: MatchedPageAssetKind::Userscript,
                    path,
                    source,
                    run_at: script.run_at,
                });
            }
        }
    }
    matches
}

/// Snapshot local filter lists from currently active packs for the shield
/// worker. Remote URLs are intentionally excluded; no network fetch occurs.
pub(crate) fn active_local_filter_lists() -> Vec<ActiveLocalFilterList> {
    let Ok(active) = active_packs().read() else {
        return Vec::new();
    };
    active
        .iter()
        .flat_map(|(plugin_id, loaded)| {
            loaded
                .manifest
                .pack
                .filter_lists
                .iter()
                .filter_map(|list| {
                    let relative = list.file.as_deref()?;
                    let path = loaded.asset_path(relative).ok()?;
                    let source = fs::read_to_string(&path).ok()?;
                    Some(ActiveLocalFilterList {
                        plugin_id: plugin_id.clone(),
                        path,
                        source,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Theme definitions from enabled packs, in stable plugin-id order. Later
/// packs override only tokens they define.
pub(crate) fn active_theme_overrides() -> Vec<ThemePack> {
    active_packs()
        .read()
        .map(|active| {
            active
                .values()
                .filter_map(|pack| pack.manifest.pack.theme.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Merge layout fields from enabled packs in stable plugin-id order. This is a
/// data-only getter; the app decides how to apply the preset to its shell.
pub(crate) fn active_layout_preset() -> Option<LayoutPack> {
    let active = active_packs().read().ok()?;
    let mut result = LayoutPack {
        sidebar_side: None,
        sidebar_collapsed: None,
        topbar: None,
    };
    let mut found = false;
    for pack in active.values() {
        let Some(layout) = &pack.manifest.pack.layout else {
            continue;
        };
        found = true;
        if layout.sidebar_side.is_some() {
            result.sidebar_side = layout.sidebar_side.clone();
        }
        if layout.sidebar_collapsed.is_some() {
            result.sidebar_collapsed = layout.sidebar_collapsed;
        }
        if layout.topbar.is_some() {
            result.topbar = layout.topbar.clone();
        }
    }
    found.then_some(result)
}

fn register_active_pack(pack: LoadedPack) -> Result<(), PluginError> {
    let id = pack.manifest.plugin.id.clone();
    let mut active = active_packs()
        .write()
        .map_err(|_| PluginError("active pack registry lock poisoned".into()))?;
    if active.contains_key(&id) {
        return Err(PluginError(format!("pack is already active: {id}")));
    }
    active.insert(id, pack);
    Ok(())
}

fn unregister_active_pack(id: &str) {
    if let Ok(mut active) = active_packs().write() {
        active.remove(id);
    }
}

fn refresh_shield_filter_lists() {
    let lists = active_local_filter_lists()
        .into_iter()
        .map(|list| list.source)
        .collect();
    crate::shield::replace_active_pack_filter_lists(lists);
}

pub(crate) struct PackRegistrationError {
    pub(crate) directory: PathBuf,
    pub(crate) plugin_id: String,
    pub(crate) message: String,
}

/// Validate a folder before the manager copies it into the plugin data root.
pub(crate) fn validate_pack_directory(directory: &Path) -> Result<(), String> {
    LoadedPack::load(directory)
        .map(|_| ())
        .map_err(|error| error.message().to_owned())
}

/// Scan and register each pack directory. Invalid manifests stay visible as
/// non-enabled manager rows with their validation message in the description.
/// Missing roots are treated as an empty install; directory I/O failures are
/// returned to the caller.
pub(crate) fn register_packs(
    host: &mut PluginHost,
    root: &Path,
) -> Result<Vec<PackRegistrationError>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", root.display())),
    };
    let mut directories = entries
        .map(|entry| entry.map_err(|error| format!("{}: {error}", root.display())))
        .collect::<Result<Vec<_>, _>>()?;
    directories.sort_by_key(|entry| entry.file_name());

    let mut seen_ids = HashSet::new();
    let mut errors = Vec::new();
    for entry in directories {
        if !entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let directory = entry.path();
        // Other plugin kinds share the same install root. Their own scanner
        // owns validation and manager registration, so don't surface them as
        // broken declarative packs.
        if let Ok(source) = fs::read_to_string(directory.join("lumen-plugin.toml"))
            && let Ok(value) = toml::from_str::<toml::Value>(&source)
            && let Some(kind) = value
                .get("plugin")
                .and_then(|plugin| plugin.get("kind"))
                .and_then(toml::Value::as_str)
            && kind != "pack"
        {
            continue;
        }
        match LoadedPack::load(&directory) {
            Ok(pack) => {
                let id = pack.manifest.plugin.id.clone();
                let validation_error = if id.starts_with("lumen.") {
                    Some("pack IDs under the reserved lumen.* namespace are not allowed".into())
                } else if !seen_ids.insert(id.clone()) {
                    Some(format!("duplicate pack ID: {id}"))
                } else {
                    None
                };
                if let Some(message) = validation_error {
                    let fallback_id = register_invalid(host, &directory, message.clone());
                    errors.push(PackRegistrationError {
                        directory,
                        plugin_id: fallback_id,
                        message,
                    });
                    continue;
                }
                let descriptor = descriptor_for_pack(&pack);
                host.register(descriptor, move || Box::new(PackPlugin::new(pack.clone())));
            }
            Err(error) => {
                let message = error.message().to_owned();
                let plugin_id = register_invalid(host, &directory, message.clone());
                errors.push(PackRegistrationError {
                    directory,
                    plugin_id,
                    message,
                });
            }
        }
    }
    Ok(errors)
}

fn descriptor_for_pack(pack: &LoadedPack) -> PluginDescriptor {
    PluginDescriptor {
        id: pack.manifest.plugin.id.clone(),
        name: pack.manifest.plugin.name.clone(),
        version: pack.manifest.plugin.version.clone(),
        kind: PluginKind::Pack,
        description: pack.manifest.plugin.description.clone(),
        permissions: pack
            .manifest
            .pack
            .userscript
            .is_empty()
            .then(Vec::new)
            .unwrap_or_else(|| vec!["userscripts are disabled by default".into()]),
        enabled_by_default: false,
        can_toggle: true,
    }
}

fn register_invalid(host: &mut PluginHost, directory: &Path, message: String) -> String {
    let fallback_id = format!("invalid.pack.{}", path_slug(directory));
    let descriptor = PluginDescriptor {
        id: fallback_id.clone(),
        name: directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Invalid pack")
            .to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        kind: PluginKind::Pack,
        description: format!("Invalid pack: {message}"),
        permissions: Vec::new(),
        enabled_by_default: false,
        can_toggle: true,
    };
    let plugin = InvalidPackPlugin::new(fallback_id.clone(), message);
    host.register(descriptor, move || Box::new(plugin.clone()));
    fallback_id
}

fn path_slug(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let slug = name
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "unknown".into()
    } else {
        slug
    }
}

#[derive(Clone)]
struct InvalidPackPlugin {
    manifest: Manifest,
    message: String,
}

impl InvalidPackPlugin {
    fn new(id: String, message: String) -> Self {
        Self {
            manifest: Manifest {
                id,
                required: Vec::new(),
                optional: Vec::new(),
                provides: Vec::new(),
            },
            message,
        }
    }
}

impl Plugin for InvalidPackPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, _: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        Err(PluginError(self.message.clone()))
    }
}

#[derive(Clone)]
pub(crate) struct PackPlugin {
    manifest: Manifest,
    pack: LoadedPack,
}

impl PackPlugin {
    pub(crate) fn new(pack: LoadedPack) -> Self {
        Self {
            manifest: Manifest {
                id: pack.manifest.plugin.id.clone(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: Vec::new(),
            },
            pack,
        }
    }
}

impl Plugin for PackPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let owner = self.manifest.id.clone();
        for (key, command) in &self.pack.manifest.pack.keymap {
            ctx.contribute(
                "keymap.binding",
                format!("{key}={}", canonical_command(command)),
                Order::default(),
            );
        }
        register_active_pack(self.pack.clone())?;
        let active_owner = owner.clone();
        ctx.effect(move || {
            unregister_active_pack(&active_owner);
            refresh_shield_filter_lists();
        });
        refresh_shield_filter_lists();
        let cleanup_owner = owner.clone();
        ctx.effect(move || {
            crate::settings::unregister_owned_search_engines(&cleanup_owner);
        });
        for engine in &self.pack.manifest.pack.search_engines {
            let definition = SearchEngineDefinition {
                id: engine.id.clone(),
                label: engine.name.clone(),
                url_template: engine.url.clone(),
                keyword: engine.keyword.clone(),
            };
            crate::settings::register_owned_search_engine(&owner, definition)
                .map_err(PluginError)?;
            ctx.contribute("search.engine", engine.id.clone(), Order::default());
        }

        // Scripts are deliberately data-only and are never executed by this
        // adapter. The plugin manager exposes this as an explicit default-off
        // capability until a permission-gated executor is integrated.
        Ok(())
    }
}

fn canonical_command(command: &str) -> &str {
    match command {
        "tabs.next" => "next-tab",
        "tabs.previous" | "tabs.prev" => "previous-tab",
        _ => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{CorePlugin, Kernel, PluginConfig};

    fn pack(id: &str, content: &str) -> LoadedPack {
        let base = std::env::temp_dir().join(format!(
            "lumen-pack-plugin-{}-{}",
            std::process::id(),
            id.replace('.', "-")
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let source = content.replace("example.test", id).replace(
            "test-search",
            &format!("test-search-{}", id.replace('.', "-")),
        );
        fs::write(base.join("lumen-plugin.toml"), source).unwrap();
        fs::write(base.join("script.js"), "void 0;").unwrap();
        fs::write(base.join("style.css"), "body { color: red; }").unwrap();
        fs::create_dir_all(base.join("filters")).unwrap();
        fs::write(base.join("filters/list.txt"), "||ads.example^").unwrap();
        LoadedPack::load(&base).unwrap()
    }

    const SIMPLE: &str = "[plugin]\nid='example.test'\nname='Test'\nversion='1.0.0'\napi='1'\nkind='pack'\n[pack.keymap]\n'ctrl-j'='tabs.next'\n[[pack.search_engine]]\nid='test-search'\nname='Test Search'\nurl='https://search.example/?q=%s'\n";

    #[test]
    fn keymaps_and_search_engines_are_removed_on_unmount_and_reenable() {
        let pack = pack("example.lifecycle", SIMPLE);
        let owner = pack.manifest.plugin.id.clone();
        let mut kernel = Kernel::new();
        kernel.mount(Box::new(CorePlugin::new())).unwrap();
        kernel
            .mount(Box::new(PackPlugin::new(pack.clone())))
            .unwrap();
        assert!(kernel.contributions().iter().any(|item| {
            item.plugin_id == owner
                && item.key == "keymap.binding"
                && item.value == "ctrl-j=next-tab"
        }));
        assert_eq!(
            crate::settings::custom_search_url("test-search-example-lifecycle", "query").as_deref(),
            Some("https://search.example/?q=query")
        );
        kernel.unmount(&owner);
        assert!(
            !kernel
                .contributions()
                .iter()
                .any(|item| item.plugin_id == owner)
        );
        assert!(
            crate::settings::custom_search_url("test-search-example-lifecycle", "query").is_none()
        );

        kernel.mount(Box::new(PackPlugin::new(pack))).unwrap();
        assert!(
            crate::settings::custom_search_url("test-search-example-lifecycle", "query").is_some()
        );
        kernel.unmount(&owner);
        let _ = fs::remove_dir_all(pack_root_for_test(&owner));
    }

    #[test]
    fn active_pack_exposes_only_matching_styles_and_local_filter_sources() {
        let content = "[plugin]\nid='example.test'\nname='Test'\nversion='1.0.0'\napi='1'\nkind='pack'\n[[pack.userstyle]]\nmatch=['https://example.com/*']\nexclude=['https://example.com/private*']\ncss='style.css'\n[[pack.filter_list]]\nfile='filters/list.txt'\n";
        let pack = pack("example.assets", content);
        let id = pack.manifest.plugin.id.clone();
        let root = pack.root.clone();
        let mut kernel = Kernel::new();
        kernel.mount(Box::new(PackPlugin::new(pack))).unwrap();

        let assets = matched_page_assets("https://example.com/public");
        assert!(assets.iter().any(|asset| {
            asset.plugin_id == id
                && asset.kind == MatchedPageAssetKind::Userstyle
                && asset.source.contains("color: red")
        }));
        assert!(
            !matched_page_assets("https://example.com/private/page")
                .iter()
                .any(|asset| asset.plugin_id == id)
        );
        let filters = active_local_filter_lists();
        assert!(
            filters.iter().any(|filter| {
                filter.plugin_id == id && filter.source.contains("||ads.example^")
            })
        );

        kernel.unmount(&id);
        assert!(
            !active_local_filter_lists()
                .iter()
                .any(|filter| filter.plugin_id == id)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn packs_and_userscripts_are_off_by_default_and_scripts_are_not_executed() {
        let pack = pack(
            "example.userscript",
            &format!(
                "{}\n[[pack.userscript]]\nmatch=['https://example.com/*']\njs='script.js'\n",
                SIMPLE.replace("[[pack.search_engine]]\nid='test-search'\nname='Test Search'\nurl='https://search.example/?q=%s'\n", "")
            ),
        );
        assert!(!descriptor_for_pack(&pack).enabled_by_default);
        let mut kernel = Kernel::new();
        kernel
            .mount(Box::new(PackPlugin::new(pack.clone())))
            .unwrap();
        assert!(
            !kernel
                .contributions()
                .iter()
                .any(|item| item.key.contains("userscript"))
        );
        assert!(
            matched_page_assets("https://example.com/page")
                .iter()
                .any(|asset| asset.kind == MatchedPageAssetKind::Userscript)
        );
        assert!(
            !active_local_filter_lists()
                .iter()
                .any(|filter| filter.plugin_id == pack.manifest.plugin.id)
        );
        kernel.unmount(&pack.manifest.plugin.id);
        assert!(
            !matched_page_assets("https://example.com/page")
                .iter()
                .any(|asset| asset.plugin_id == pack.manifest.plugin.id)
        );
        let _ = fs::remove_dir_all(pack.root);
    }

    fn pack_root_for_test(id: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lumen-pack-plugin-{}-{}",
            std::process::id(),
            id.replace('.', "-")
        ))
    }

    #[test]
    fn malformed_packs_are_retained_as_default_off_manager_rows() {
        let root = std::env::temp_dir().join(format!("lumen-invalid-packs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("broken-pack")).unwrap();
        fs::write(
            root.join("broken-pack/lumen-plugin.toml"),
            "not valid toml = [",
        )
        .unwrap();
        let mut host = PluginHost::new(PluginConfig::default());
        let errors = register_packs(&mut host, &root).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("TOML"));
        let row = host
            .rows()
            .into_iter()
            .find(|row| row.descriptor.id == errors[0].plugin_id)
            .unwrap();
        assert!(!row.enabled);
        assert!(row.descriptor.description.starts_with("Invalid pack:"));
        let _ = fs::remove_dir_all(root);
    }
}
