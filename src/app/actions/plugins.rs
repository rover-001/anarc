use super::super::*;
use crate::app::state::PendingPluginReview;
use crate::ui::plugin_manager::{InstallSource, PluginReview};
use std::{fs, path::Path};

impl Anarc {
    #[cfg(not(test))]
    pub(crate) fn inject_pack_assets(&self, tab_id: &str, url: &str) {
        for asset in crate::plugin::matched_page_assets(url) {
            let Ok(source) = serde_json::to_string(&asset.source) else {
                continue;
            };
            let script = match asset.kind {
                crate::plugin::MatchedPageAssetKind::Userstyle => {
                    let Ok(id) = serde_json::to_string(&format!(
                        "{}:{}",
                        asset.plugin_id,
                        asset.path.display()
                    )) else {
                        continue;
                    };
                    format!(
                        "(()=>{{let s=document.querySelector('style[data-lumen-plugin='+CSS.escape({id})+']');if(!s){{s=document.createElement('style');s.dataset.lumenPlugin={id};(document.head||document.documentElement).appendChild(s)}}s.textContent={source}}})();"
                    )
                }
                crate::plugin::MatchedPageAssetKind::Userscript => {
                    format!("(()=>{{(0,eval)({source})}})();")
                }
            };
            self.engine.execute_script(tab_id, &script);
        }
    }

    pub(crate) fn set_plugin_action_error(&mut self, error: Option<String>) {
        self.plugin_action_error = error;
    }

    /// Expand or collapse the details accordion for one plugin row.
    pub(crate) fn toggle_plugin_details(&mut self, id: String) {
        if !self.expanded_plugins.remove(&id) {
            self.expanded_plugins.insert(id);
        }
    }

    pub(crate) fn plugin_action_error(&self) -> Option<&str> {
        self.plugin_action_error.as_deref()
    }

    pub(crate) fn pending_plugin_review(&self) -> Option<PluginReview> {
        self.pending_plugin_review
            .as_ref()
            .map(|pending| pending.review.clone())
    }

    /// Inspect a selected source and stage it for the approved permission
    /// review. No files are copied and no plugin code is enabled here.
    pub(crate) fn install_plugin_path(
        &mut self,
        path: &Path,
        source: InstallSource,
    ) -> Result<(), String> {
        let (
            id,
            name,
            version,
            kind,
            permissions,
            signature_valid,
            signature_trusted,
            signer_key,
            manifest_toml,
        ) = match source {
            InstallSource::Package => {
                let info = crate::plugin::inspect_package(path, None).map_err(|error| error.0)?;
                if !info.signature_valid {
                    return Err("package signature is invalid".into());
                }
                (
                    info.id,
                    info.name,
                    info.version,
                    info.kind,
                    info.permission_summary,
                    info.signature_valid,
                    info.signature_trusted,
                    Some(info.signer_public_key),
                    info.manifest_toml,
                )
            }
            InstallSource::Folder => {
                let root = path
                    .canonicalize()
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                if !root.is_dir() {
                    return Err("selected plugin source is not a folder".into());
                }
                let manifest_path = if root.join("anarc-plugin.toml").is_file() {
                    root.join("anarc-plugin.toml")
                } else {
                    root.join("lumen-plugin.toml")
                };
                let manifest = fs::read_to_string(&manifest_path)
                    .map_err(|error| format!("{}: {error}", root.display()))?;
                let value: toml::Value = toml::from_str(&manifest)
                    .map_err(|error| format!("invalid plugin manifest: {error}"))?;
                let (id, name, version, kind) = validate_install_manifest(&value)?;
                validate_install_kind(&root, &value, &id, &kind)?;
                (
                    id,
                    name,
                    version,
                    kind,
                    permission_summary(&value),
                    false,
                    false,
                    None,
                    manifest,
                )
            }
        };
        if kind == "wasm" && !cfg!(feature = "wasm-plugins") {
            return Err("WASM plugins are unavailable in this build".into());
        }
        if self
            .plugins
            .rows()
            .iter()
            .any(|row| row.descriptor.id == id)
        {
            return Err(format!("plugin is already installed: {id}"));
        }
        self.pending_plugin_review = Some(PendingPluginReview {
            review: PluginReview {
                id,
                name,
                version,
                source,
                requested_permissions: permissions,
                signature_valid,
                signature_trusted,
                signer_fingerprint: signer_key.as_deref().map(fingerprint),
            },
            source_path: path.to_owned(),
            signer_public_key: signer_key,
            manifest_toml,
        });
        Ok(())
    }

    pub(crate) fn confirm_plugin_install(&mut self, approved: bool) -> Result<(), String> {
        let Some(pending) = self.pending_plugin_review.take() else {
            return Ok(());
        };
        if !approved {
            return Ok(());
        }
        let plugin_root = crate::paths::data_dir().join("plugins");
        let id = pending.review.id.clone();
        let result = match pending.review.source {
            InstallSource::Package => pending
                .signer_public_key
                .as_deref()
                .ok_or_else(|| "package signer key is missing".to_owned())
                .and_then(|key| {
                    crate::plugin::install_package(&pending.source_path, &plugin_root, key)
                        .map_err(|error| error.0)
                        .and_then(|installed| {
                            if installed.info.manifest_toml != pending.manifest_toml {
                                return Err(
                                    "package manifest changed after permission review".into()
                                );
                            }
                            let value: toml::Value = toml::from_str(&installed.info.manifest_toml)
                                .map_err(|error| error.to_string())?;
                            validate_install_kind(
                                &installed.path,
                                &value,
                                &installed.info.id,
                                &installed.info.kind,
                            )
                        })
                }),
            InstallSource::Folder => install_folder(
                &pending.source_path,
                &plugin_root,
                &id,
                &pending.manifest_toml,
            ),
        };
        let result = result.and_then(|()| {
            crate::plugin::register_packs(&mut self.plugins, &plugin_root)
                .map_err(|error| error.to_string())?;
            crate::plugin::register_panel_plugins(
                &mut self.plugins,
                &plugin_root,
                self.panel_registry.clone(),
            )?;
            #[cfg(feature = "wasm-plugins")]
            crate::plugin::register_wasm_plugins(&mut self.plugins, &plugin_root)?;
            // Installed plugins always start disabled, even if plugins.toml
            // retained an entry for this ID from an earlier installation.
            self.plugins.set_enabled(&id, false)
        });
        if let Err(error) = result {
            let installed = plugin_root.join(&id);
            let _ = fs::remove_dir_all(installed);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn remove_plugin(&mut self, id: &str) -> Result<(), String> {
        let row = self
            .plugins
            .rows()
            .into_iter()
            .find(|row| row.descriptor.id == id)
            .ok_or_else(|| format!("unknown plugin: {id}"))?;
        if matches!(
            row.descriptor.kind,
            crate::plugin::PluginKind::Builtin | crate::plugin::PluginKind::Core
        ) {
            return Err("built-in plugins cannot be removed".into());
        }
        let plugin_root = crate::paths::data_dir().join("plugins");
        let target = plugin_root.join(id);
        // An invalid plugin (for example a manifest-less leftover folder) gets a
        // fallback id that never matches an installed directory. Removal must
        // still unregister it so the stale row and config entry disappear.
        let canonical_target = match target.canonicalize() {
            Ok(path) => Some(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("{}: {error}", target.display())),
        };
        if let Some(path) = canonical_target.as_ref() {
            let canonical_root = plugin_root
                .canonicalize()
                .map_err(|error| format!("{}: {error}", plugin_root.display()))?;
            if !path.starts_with(&canonical_root) || !path.is_dir() {
                return Err("plugin directory is outside the install root".into());
            }
        }
        if row.enabled {
            self.set_plugin_enabled(id, false)?;
        }
        if let Some(path) = canonical_target {
            fs::remove_dir_all(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        self.plugins.remove(id)
    }

    pub(crate) fn open_plugin_folder(&self) -> Result<(), String> {
        let directory = crate::paths::data_dir().join("plugins");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        #[cfg(target_os = "linux")]
        let mut command = std::process::Command::new("xdg-open");
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("open");
        #[cfg(target_os = "windows")]
        let mut command = std::process::Command::new("explorer");
        command
            .arg(&directory)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("could not open {}: {error}", directory.display()))
    }

    pub(crate) fn reload_plugins(&mut self) -> Result<(), String> {
        self.plugins.unmount_external()?;
        let data_plugins = crate::paths::data_dir().join("plugins");
        let bundled_plugins = std::path::PathBuf::from("plugins");
        let exe_plugins = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("plugins")));
        for root in [&data_plugins, &bundled_plugins]
            .into_iter()
            .chain(exe_plugins.as_ref())
        {
            if root.is_dir() {
                let _ = crate::plugin::register_packs(&mut self.plugins, root);
                let _ = crate::plugin::register_panel_plugins(
                    &mut self.plugins,
                    root,
                    self.panel_registry.clone(),
                );
                #[cfg(feature = "wasm-plugins")]
                let _ = crate::plugin::register_wasm_plugins(&mut self.plugins, root);
            }
        }
        self.plugins.mount_enabled()
    }
}

fn install_folder(
    source: &Path,
    plugin_root: &Path,
    id: &str,
    reviewed_manifest: &str,
) -> Result<(), String> {
    let source = source
        .canonicalize()
        .map_err(|error| format!("{}: {error}", source.display()))?;
    let plugin_root = if plugin_root.exists() {
        plugin_root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", plugin_root.display()))?
    } else {
        fs::create_dir_all(plugin_root)
            .map_err(|error| format!("{}: {error}", plugin_root.display()))?;
        plugin_root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", plugin_root.display()))?
    };
    let destination = plugin_root.join(id);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(format!("plugin is already installed: {id}"));
    }
    let staging = plugin_root.join(format!(".{id}.install-{}", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir(&staging).map_err(|error| error.to_string())?;
    let copied = copy_plugin_tree(&source, &source, &staging, &mut 0usize, &mut 0u64);
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let validation = (|| {
        let copied_manifest = fs::read_to_string(staging.join("lumen-plugin.toml"))
            .map_err(|error| format!("{}: {error}", staging.display()))?;
        let parsed_manifest: toml::Value = toml::from_str(&copied_manifest)
            .map_err(|error| format!("invalid plugin manifest: {error}"))?;
        let (manifest_id, _, _, kind) = validate_install_manifest(&parsed_manifest)?;
        if manifest_id != id {
            return Err("plugin ID changed after permission review".into());
        }
        if copied_manifest != reviewed_manifest {
            return Err("plugin manifest changed after permission review".into());
        }
        validate_install_kind(&staging, &parsed_manifest, id, &kind)
    })();
    if let Err(error) = validation {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("{}: {error}", destination.display()));
    }
    Ok(())
}

fn copy_plugin_tree(
    root: &Path,
    current: &Path,
    destination: &Path,
    entries: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), String> {
    const MAX_FILES: usize = 10_000;
    const MAX_BYTES: u64 = 256 * 1024 * 1024;
    let mut children = fs::read_dir(current)
        .map_err(|error| format!("{}: {error}", current.display()))?
        .map(|entry| entry.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        *entries += 1;
        if *entries > MAX_FILES {
            return Err("plugin folder contains too many entries".into());
        }
        let metadata = fs::symlink_metadata(child.path())
            .map_err(|error| format!("{}: {error}", child.path().display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "plugin folders cannot contain symlinks: {}",
                child.path().display()
            ));
        }
        let relative = child
            .path()
            .strip_prefix(root)
            .map_err(|_| "plugin source escaped its root".to_owned())?
            .to_owned();
        let target = destination.join(relative);
        if metadata.is_dir() {
            fs::create_dir(&target).map_err(|error| format!("{}: {error}", target.display()))?;
            copy_plugin_tree(root, &child.path(), destination, entries, total_bytes)?;
        } else if metadata.is_file() {
            *total_bytes = total_bytes
                .checked_add(metadata.len())
                .filter(|size| *size <= MAX_BYTES)
                .ok_or_else(|| "plugin folder exceeds the 256 MiB limit".to_owned())?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(child.path(), &target)
                .map_err(|error| format!("{}: {error}", target.display()))?;
        } else {
            return Err(format!(
                "unsupported plugin file: {}",
                child.path().display()
            ));
        }
    }
    Ok(())
}

fn validate_install_manifest(
    value: &toml::Value,
) -> Result<(String, String, String, String), String> {
    let plugin = value
        .get("plugin")
        .and_then(toml::Value::as_table)
        .ok_or("manifest is missing [plugin]")?;
    let id = string_field(plugin, "id")?;
    let valid_id = id.len() <= 253
        && !id.starts_with("lumen.")
        && !id.starts_with("anarc.")
        && id.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        });
    if !valid_id {
        return Err(
            "plugin id must be reverse-DNS and outside the reserved anarc.* and lumen.* namespaces".into(),
        );
    }
    let version = string_field(plugin, "version")?;
    let kind = string_field(plugin, "kind")?;
    let api = string_field(plugin, "api")?;
    let name = string_field(plugin, "name")?;
    if version.trim().is_empty() || name.trim().is_empty() {
        return Err("plugin name and version cannot be empty".into());
    }
    if api != "1" || !matches!(kind.as_str(), "pack" | "panel" | "wasm") {
        return Err("unsupported plugin kind or API version".into());
    }
    Ok((id, name, version, kind))
}

fn validate_install_kind(
    root: &Path,
    value: &toml::Value,
    id: &str,
    kind: &str,
) -> Result<(), String> {
    match kind {
        "pack" => crate::plugin::validate_pack_directory(root),
        "panel" => {
            let network = value
                .get("permissions")
                .and_then(|permissions| permissions.get("network"))
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let package = crate::plugin::PanelPackage::new(id, root, network)
                .map_err(|error| format!("invalid panel package: {error:?}"))?;
            package
                .response(&format!("anarc-plugin://{id}/"), "GET")
                .or_else(|_| package.response(&format!("lumen-plugin://{id}/"), "GET"))
                .map(|_| ())
                .map_err(|error| format!("invalid panel entry point: {error:?}"))
        }
        "wasm" => {
            if !cfg!(feature = "wasm-plugins") {
                return Err("WASM plugins are unavailable in this build".into());
            }
            let plugin = value
                .get("plugin")
                .and_then(toml::Value::as_table)
                .ok_or("manifest is missing [plugin]")?;
            let entry = string_field(plugin, "entry")?;
            if Path::new(&entry)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
            {
                return Err("WASM component path must stay inside the plugin directory".into());
            }
            let root = root
                .canonicalize()
                .map_err(|error| format!("{}: {error}", root.display()))?;
            let component = root
                .join(entry)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !component.starts_with(&root) || !component.is_file() {
                return Err("WASM component path escapes the plugin directory".into());
            }
            Ok(())
        }
        _ => Err(format!("unsupported plugin kind: {kind}")),
    }
}

fn string_field(
    table: &toml::map::Map<String, toml::Value>,
    field: &str,
) -> Result<String, String> {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("manifest plugin.{field} is missing"))
}

fn permission_summary(value: &toml::Value) -> Vec<String> {
    let mut summary: Vec<String> = value
        .get("permissions")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(key, value)| format!("{key} = {value}"))
                .collect()
        })
        .unwrap_or_default();
    if let Some(pack) = value.get("pack") {
        for (key, label) in [
            ("userstyle", "Userstyles can modify CSS on matched pages"),
            (
                "userscript",
                "Userscripts can read and modify matched pages",
            ),
        ] {
            let patterns = pack
                .get(key)
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.get("match"))
                .filter_map(toml::Value::as_array)
                .flatten()
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>();
            if !patterns.is_empty() {
                summary.push(format!("{label}: {}", patterns.join(", ")));
            }
        }
        let remote_filters = pack
            .get("filter_list")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("url"))
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>();
        if !remote_filters.is_empty() {
            summary.push(format!(
                "Downloads filter rules from: {}",
                remote_filters.join(", ")
            ));
        }
    }
    summary
}

fn fingerprint(key: &[u8]) -> String {
    key.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|chunk| chunk.join(""))
        .collect::<Vec<_>>()
        .join(":")
}
