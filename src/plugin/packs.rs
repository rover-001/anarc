//! Data-only support for declarative plugin packs.
//!
//! This module deliberately performs no registration or code execution. The host
//! can validate a pack, safely resolve its assets, and then apply each section
//! through the corresponding service.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    net::IpAddr,
    path::{Component, Path, PathBuf},
};

const MANIFEST_FILE: &str = "lumen-plugin.toml";
const API_VERSION: &str = "1";
const KNOWN_THEME_TOKENS: &[&str] = &[
    "base",
    "surface",
    "surface_hover",
    "border",
    "accent",
    "text",
    "muted",
    "card_radius",
    "ui_scale",
    "font_family",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct PackManifest {
    pub(crate) plugin: PackMetadata,
    #[serde(default)]
    pub(crate) pack: Pack,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct PackMetadata {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) api: String,
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) description: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct Pack {
    #[serde(default)]
    pub(crate) theme: Option<ThemePack>,
    #[serde(default)]
    pub(crate) userstyle: Vec<Userstyle>,
    #[serde(default)]
    pub(crate) userscript: Vec<Userscript>,
    #[serde(default)]
    pub(crate) keymap: BTreeMap<String, String>,
    #[serde(default, rename = "search_engine")]
    pub(crate) search_engines: Vec<SearchEnginePack>,
    #[serde(default, rename = "filter_list")]
    pub(crate) filter_lists: Vec<FilterListPack>,
    #[serde(default)]
    pub(crate) layout: Option<LayoutPack>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct ThemePack {
    #[serde(default)]
    pub(crate) mode: Option<String>,
    #[serde(default)]
    pub(crate) tokens: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub(crate) motion: BTreeMap<String, toml::Value>,
}

impl ThemePack {
    /// Token keys unknown to this host version are ignored with a warning by
    /// the caller, preserving forward compatibility for newer packs.
    pub(crate) fn unknown_tokens(&self) -> Vec<&str> {
        self.tokens
            .keys()
            .filter(|key| !KNOWN_THEME_TOKENS.contains(&key.as_str()))
            .map(String::as_str)
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Userstyle {
    #[serde(rename = "match")]
    pub(crate) matches: Vec<String>,
    #[serde(default)]
    pub(crate) exclude: Vec<String>,
    pub(crate) css: String,
    #[serde(default)]
    pub(crate) run_at: RunAt,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Userscript {
    #[serde(rename = "match")]
    pub(crate) matches: Vec<String>,
    #[serde(default)]
    pub(crate) exclude: Vec<String>,
    pub(crate) js: String,
    #[serde(default)]
    pub(crate) run_at: RunAt,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum RunAt {
    DocumentStart,
    #[default]
    DocumentIdle,
    DocumentEnd,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SearchEnginePack {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) keyword: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct FilterListPack {
    #[serde(default)]
    pub(crate) file: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) refresh_interval: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct LayoutPack {
    #[serde(default)]
    pub(crate) sidebar_side: Option<String>,
    #[serde(default)]
    pub(crate) sidebar_collapsed: Option<bool>,
    #[serde(default)]
    pub(crate) topbar: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoadedPack {
    pub(crate) root: PathBuf,
    pub(crate) manifest: PackManifest,
}

impl LoadedPack {
    /// Load and validate `<root>/lumen-plugin.toml` and every referenced local
    /// asset. No pack code is executed and no network request is made.
    pub(crate) fn load(root: &Path) -> Result<Self, PackError> {
        let root = root
            .canonicalize()
            .map_err(|error| PackError::io(root, error))?;
        if !root.is_dir() {
            return Err(PackError::new("pack root is not a directory"));
        }
        let manifest_path = if root.join("anarc-plugin.toml").is_file() {
            root.join("anarc-plugin.toml")
        } else {
            root.join(MANIFEST_FILE)
        };
        let source = fs::read_to_string(&manifest_path)
            .map_err(|error| PackError::io(&manifest_path, error))?;
        let manifest: PackManifest = toml::from_str(&source)
            .map_err(|error| PackError::new(format!("{}: {error}", manifest_path.display())))?;
        manifest.validate()?;
        for style in &manifest.pack.userstyle {
            resolve_pack_asset(&root, &style.css)?;
        }
        for script in &manifest.pack.userscript {
            resolve_pack_asset(&root, &script.js)?;
        }
        for list in &manifest.pack.filter_lists {
            if let Some(file) = &list.file {
                resolve_pack_asset(&root, file)?;
            }
        }
        Ok(Self { root, manifest })
    }

    pub(crate) fn asset_path(&self, relative: &str) -> Result<PathBuf, PackError> {
        resolve_pack_asset(&self.root, relative)
    }
}

/// Scan `$ANARC_DATA_DIR/plugins` without letting one malformed pack hide the
/// others. Each child directory gets its own `Result` for the manager UI.
pub(crate) fn scan_pack_root(root: &Path) -> Result<Vec<Result<LoadedPack, PackError>>, PackError> {
    let mut directories = fs::read_dir(root)
        .map_err(|error| PackError::io(root, error))?
        .map(|entry| entry.map_err(|error| PackError::io(root, error)))
        .collect::<Result<Vec<_>, _>>()?;
    directories.sort_by_key(|entry| entry.file_name());
    Ok(directories
        .into_iter()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| LoadedPack::load(&entry.path()))
        .collect())
}

impl PackManifest {
    pub(crate) fn parse(source: &str) -> Result<Self, PackError> {
        let manifest: Self =
            toml::from_str(source).map_err(|error| PackError::new(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn validate(&self) -> Result<(), PackError> {
        if self.plugin.kind != "pack" {
            return Err(PackError::new("plugin.kind must be 'pack'"));
        }
        if self.plugin.api != API_VERSION {
            return Err(PackError::new(format!(
                "unsupported plugin API {}; this host supports {API_VERSION}",
                self.plugin.api
            )));
        }
        validate_reverse_dns(&self.plugin.id)?;
        if self.plugin.name.trim().is_empty() {
            return Err(PackError::new("plugin.name cannot be empty"));
        }
        validate_version(&self.plugin.version)?;
        if let Some(theme) = &self.pack.theme
            && let Some(mode) = theme.mode.as_deref()
            && !matches!(mode, "dark" | "light" | "system")
        {
            return Err(PackError::new(
                "pack.theme.mode must be dark, light, or system",
            ));
        }
        for style in &self.pack.userstyle {
            validate_rules(&style.matches, &style.exclude)?;
            validate_asset_reference(&style.css)?;
        }
        for script in &self.pack.userscript {
            validate_rules(&script.matches, &script.exclude)?;
            validate_asset_reference(&script.js)?;
        }
        for engine in &self.pack.search_engines {
            if engine.id.trim().is_empty() || engine.name.trim().is_empty() {
                return Err(PackError::new("search engine id and name cannot be empty"));
            }
            if !engine.url.contains("%s") {
                return Err(PackError::new(format!(
                    "search engine '{}' URL must contain %s",
                    engine.id
                )));
            }
            validate_remote_url(&engine.url.replace("%s", "lumen-query"), false).map_err(
                |message| {
                    PackError::new(format!(
                        "search engine '{}' has an invalid URL: {message}",
                        engine.id
                    ))
                },
            )?;
        }
        for filter in &self.pack.filter_lists {
            if filter.file.is_some() == filter.url.is_some() {
                return Err(PackError::new(
                    "each filter_list must specify exactly one of file or url",
                ));
            }
            if let Some(file) = &filter.file {
                validate_asset_reference(file)?;
            }
            if let Some(url) = &filter.url {
                validate_remote_url(url, true)
                    .map_err(|message| PackError::new(format!("filter_list URL: {message}")))?;
            }
        }
        for (key, command) in &self.pack.keymap {
            if key.trim().is_empty() || command.trim().is_empty() {
                return Err(PackError::new("keymap keys and commands cannot be empty"));
            }
        }
        if let Some(layout) = &self.pack.layout {
            if layout
                .sidebar_side
                .as_deref()
                .is_some_and(|side| !matches!(side, "left" | "right"))
            {
                return Err(PackError::new("layout.sidebar_side must be left or right"));
            }
            if layout
                .topbar
                .as_deref()
                .is_some_and(|topbar| !matches!(topbar, "default" | "compact" | "hidden"))
            {
                return Err(PackError::new(
                    "layout.topbar must be default, compact, or hidden",
                ));
            }
        }
        Ok(())
    }

    /// Whether a URL is selected by a style or script rule. `*` matches any
    /// run of characters; matching is case-sensitive and deliberately avoids
    /// parsing or executing arbitrary regular expressions.
    pub(crate) fn matches_url(rules: &[String], excludes: &[String], url: &str) -> bool {
        rules.iter().any(|rule| wildcard_match(rule, url))
            && !excludes.iter().any(|rule| wildcard_match(rule, url))
    }
}

fn validate_rules(matches: &[String], excludes: &[String]) -> Result<(), PackError> {
    if matches.is_empty() {
        return Err(PackError::new(
            "match must contain at least one URL pattern",
        ));
    }
    for rule in matches.iter().chain(excludes) {
        if !(rule.starts_with("http://") || rule.starts_with("https://")) {
            return Err(PackError::new(format!(
                "URL pattern must begin with http:// or https://: {rule}"
            )));
        }
        if rule.chars().any(char::is_whitespace) {
            return Err(PackError::new("URL patterns cannot contain whitespace"));
        }
    }
    Ok(())
}

fn validate_asset_reference(path: &str) -> Result<(), PackError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PackError::new(
            "asset path must stay inside the pack directory",
        ));
    }
    Ok(())
}

fn validate_remote_url(url: &str, https_only: bool) -> Result<(), String> {
    if url.chars().any(char::is_whitespace) {
        return Err("URL cannot contain whitespace".into());
    }
    let (scheme, remainder) = url
        .split_once("://")
        .ok_or_else(|| "URL must include an http(s) scheme".to_string())?;
    if scheme != "https" && (https_only || scheme != "http") {
        return Err(if https_only {
            "remote filter URLs must use https".into()
        } else {
            "URL must use http or https".into()
        });
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err("URL must have a host and cannot contain credentials".into());
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .split_once(']')
            .map(|(host, _)| host)
            .ok_or_else(|| "invalid bracketed host".to_string())?
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    if host.is_empty() {
        return Err("URL host cannot be empty".into());
    }
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".internal")
    {
        return Err("loopback and local hostnames are not allowed".into());
    }
    if let Ok(address) = host.parse::<IpAddr>()
        && is_local_ip(address)
    {
        return Err("loopback and private-network IP addresses are not allowed".into());
    }
    Ok(())
}

fn is_local_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_broadcast()
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.is_unspecified()
        }
    }
}

fn resolve_pack_asset(root: &Path, relative: &str) -> Result<PathBuf, PackError> {
    validate_asset_reference(relative)?;
    let candidate = root.join(relative);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| PackError::io(&candidate, error))?;
    if !canonical.starts_with(root) {
        return Err(PackError::new("asset resolves outside the pack directory"));
    }
    if !canonical.is_file() {
        return Err(PackError::new("pack asset is not a regular file"));
    }
    Ok(canonical)
}

fn validate_reverse_dns(id: &str) -> Result<(), PackError> {
    let labels = id.split('.').collect::<Vec<_>>();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || label.starts_with('-')
                || label.ends_with('-')
        })
    {
        return Err(PackError::new(
            "plugin.id must be a lowercase reverse-DNS identifier",
        ));
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), PackError> {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.parse::<u64>().is_err()) {
        return Err(PackError::new(
            "plugin.version must use major.minor.patch form",
        ));
    }
    Ok(())
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return false;
    };
    if !value.starts_with(first) {
        return false;
    }
    let mut remainder = &value[first.len()..];
    let rest = parts.collect::<Vec<_>>();
    for (index, part) in rest.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = remainder.find(part) else {
            return false;
        };
        if index == rest.len() - 1 && !pattern.ends_with('*') {
            return remainder[found..].ends_with(part) && found + part.len() == remainder.len();
        }
        remainder = &remainder[found + part.len()..];
    }
    pattern.ends_with('*') || remainder.is_empty()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackError(String);

impl PackError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    fn io(path: &Path, error: std::io::Error) -> Self {
        Self(format!("{}: {error}", path.display()))
    }

    pub(crate) fn message(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r##"
[plugin]
id = "me.rover.midnight"
name = "Midnight"
version = "1.0.0"
api = "1"
kind = "pack"

[pack.theme]
mode = "dark"
[pack.theme.tokens]
accent = "#9aa6ff"
new_token = "future-compatible"

[[pack.userstyle]]
match = ["https://www.youtube.com/*"]
css = "styles/youtube.css"
run_at = "document_start"

[[pack.userscript]]
match = ["https://github.com/*"]
exclude = ["https://github.com/login*"]
js = "scripts/gh.js"
run_at = "document_idle"

[[pack.search_engine]]
id = "gh"
name = "GitHub"
url = "https://github.com/search?q=%s"
keyword = "!gh"

[[pack.filter_list]]
file = "filters/custom.txt"

[pack.keymap]
"ctrl-j" = "tabs.next"

[pack.layout]
sidebar_side = "left"
sidebar_collapsed = false
topbar = "default"
"##;

    #[test]
    fn manifest_parses_all_declarative_sections_and_unknown_tokens() {
        let manifest = PackManifest::parse(MANIFEST).unwrap();
        assert_eq!(manifest.pack.userstyle[0].run_at, RunAt::DocumentStart);
        assert_eq!(manifest.pack.userscript[0].run_at, RunAt::DocumentIdle);
        assert_eq!(manifest.pack.search_engines[0].id, "gh");
        assert_eq!(manifest.pack.keymap["ctrl-j"], "tabs.next");
        assert_eq!(
            manifest.pack.theme.as_ref().unwrap().unknown_tokens(),
            ["new_token"]
        );
    }

    #[test]
    fn invalid_kind_api_search_templates_remote_urls_and_paths_are_rejected() {
        let wrong_kind = MANIFEST.replace("kind = \"pack\"", "kind = \"wasm\"");
        assert!(PackManifest::parse(&wrong_kind).is_err());
        let wrong_api = MANIFEST.replace("api = \"1\"", "api = \"2\"");
        assert!(PackManifest::parse(&wrong_api).is_err());
        let bad_search = MANIFEST.replace("q=%s", "q=query");
        assert!(PackManifest::parse(&bad_search).is_err());
        for local_host in [
            "http://localhost/search?q=%s",
            "https://127.0.0.1/search?q=%s",
            "https://192.168.1.2/search?q=%s",
        ] {
            let bad_search = MANIFEST.replace("https://github.com/search?q=%s", local_host);
            assert!(PackManifest::parse(&bad_search).is_err());
        }
        let insecure_filter = MANIFEST.replace(
            "file = \"filters/custom.txt\"",
            "url = \"http://filters.example/list.txt\"",
        );
        assert!(PackManifest::parse(&insecure_filter).is_err());
        let local_filter = MANIFEST.replace(
            "file = \"filters/custom.txt\"",
            "url = \"https://localhost/list.txt\"",
        );
        assert!(PackManifest::parse(&local_filter).is_err());
        assert!(validate_asset_reference("../escape.css").is_err());
        assert!(validate_asset_reference("/tmp/escape.css").is_err());
    }

    #[test]
    fn wildcard_rules_respect_exclusions() {
        let matches = vec!["https://*.example.com/*".to_string()];
        let excludes = vec!["https://private.example.com/*".to_string()];
        assert!(PackManifest::matches_url(
            &matches,
            &excludes,
            "https://www.example.com/watch?v=1"
        ));
        assert!(!PackManifest::matches_url(
            &matches,
            &excludes,
            "https://private.example.com/account"
        ));
        assert!(!PackManifest::matches_url(
            &matches,
            &excludes,
            "https://example.net/"
        ));
    }

    #[test]
    fn local_assets_must_exist_and_symlinks_cannot_escape_pack() {
        let base = std::env::temp_dir().join(format!(
            "lumen-pack-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("worker")
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("pack");
        fs::create_dir_all(root.join("styles")).unwrap();
        fs::create_dir_all(&base).unwrap();
        fs::write(root.join(MANIFEST_FILE), MANIFEST).unwrap();
        fs::write(root.join("styles/youtube.css"), "body { color: red; }").unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/gh.js"), "void 0;").unwrap();
        fs::create_dir_all(root.join("filters")).unwrap();
        fs::write(root.join("filters/custom.txt"), "||ads.example^").unwrap();
        let loaded = LoadedPack::load(&root).unwrap();
        assert!(loaded.asset_path("styles/youtube.css").unwrap().is_file());
        assert!(loaded.asset_path("../outside").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::write(base.join("outside.css"), "body {}").unwrap();
            symlink(base.join("outside.css"), root.join("styles/outside.css")).unwrap();
            assert!(loaded.asset_path("styles/outside.css").is_err());
        }
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn scan_keeps_bad_pack_errors_isolated() {
        let root = std::env::temp_dir().join(format!(
            "lumen-pack-scan-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("worker")
        ));
        let _ = fs::remove_dir_all(&root);
        let good = root.join("good");
        let bad = root.join("bad");
        fs::create_dir_all(&good).unwrap();
        fs::create_dir_all(&bad).unwrap();
        fs::write(
            good.join(MANIFEST_FILE),
            "[plugin]\nid='example.good'\nname='Good'\nversion='1.0.0'\napi='1'\nkind='pack'\n",
        )
        .unwrap();
        fs::write(bad.join(MANIFEST_FILE), "[plugin]\nkind='pack'").unwrap();
        let results = scan_pack_root(&root).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_ok());
        let _ = fs::remove_dir_all(root);
    }
}
