//! PluginHost adapter for locally installed web panels.

use super::{
    Manifest, PanelPackage, PanelRegistryHandle, Plugin, PluginCtx, PluginDescriptor, PluginError,
    PluginHost, PluginKind,
};
use serde::Deserialize;
use std::{fs, path::Path};

const MANIFEST_FILE: &str = "lumen-plugin.toml";
const API_VERSION: &str = "1";

#[derive(Debug, Deserialize)]
struct PanelManifest {
    plugin: PanelMetadata,
    #[serde(default)]
    permissions: PanelPermissions,
}

#[derive(Debug, Deserialize)]
struct PanelMetadata {
    id: String,
    name: String,
    version: String,
    api: String,
    kind: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Default, Deserialize)]
struct PanelPermissions {
    #[serde(default)]
    network: Vec<String>,
}

#[derive(Clone)]
struct MountedPanel {
    manifest: Manifest,
    package: PanelPackage,
    registry: PanelRegistryHandle,
}

impl Plugin for MountedPanel {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let id = self.package.id.clone();
        {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| PluginError("panel registry is unavailable".into()))?;
            if registry.contains_key(&id) {
                return Err(PluginError(format!("panel is already registered: {id}")));
            }
            registry.insert(id.clone(), self.package.clone());
        }
        let registry = self.registry.clone();
        ctx.effect(move || {
            if let Ok(mut registry) = registry.lock() {
                registry.remove(&id);
            }
        });
        Ok(())
    }
}

#[derive(Clone)]
struct InvalidPanel {
    manifest: Manifest,
    error: String,
}

impl Plugin for InvalidPanel {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, _: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        Err(PluginError(self.error.clone()))
    }
}

/// Scan the plugin directory and register valid panel packages as default-off
/// plugins. Invalid panel manifests stay visible in the manager as error rows.
pub(crate) fn register_panel_plugins(
    host: &mut PluginHost,
    root: &Path,
    registry: PanelRegistryHandle,
) -> Result<(), String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", root.display())),
    };
    let mut entries = entries
        .map(|entry| entry.map_err(|error| format!("{}: {error}", root.display())))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if !entry.path().is_dir() {
            continue;
        }
        let directory = entry.path();
        let manifest_path = if directory.join("anarc-plugin.toml").is_file() {
            directory.join("anarc-plugin.toml")
        } else {
            directory.join(MANIFEST_FILE)
        };
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let value: toml::Value = match toml::from_str(&text) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value
            .get("plugin")
            .and_then(|plugin| plugin.get("kind"))
            .and_then(toml::Value::as_str)
            != Some("panel")
        {
            continue;
        }

        match parse_panel(&directory, &text) {
            Ok((metadata, package)) => {
                if host.is_registered(&metadata.id) {
                    continue;
                }
                let enabled_by_default = value
                    .get("plugin")
                    .and_then(|p| p.get("enabled_by_default"))
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false);
                let descriptor = PluginDescriptor {
                    id: metadata.id.clone(),
                    name: metadata.name,
                    version: metadata.version,
                    kind: PluginKind::Panel,
                    description: metadata.description,
                    permissions: package
                        .allowed_hosts()
                        .into_iter()
                        .map(|host| format!("HTTPS: {host}"))
                        .collect(),
                    enabled_by_default,
                    can_toggle: true,
                };
                let plugin = MountedPanel {
                    manifest: Manifest {
                        id: metadata.id,
                        required: Vec::new(),
                        optional: Vec::new(),
                        provides: Vec::new(),
                    },
                    package,
                    registry: registry.clone(),
                };
                host.register(descriptor, move || Box::new(plugin.clone()));
            }
            Err((id, name, version, error)) => {
                let id = id.unwrap_or_else(|| invalid_id(&directory));
                let descriptor = PluginDescriptor {
                    id: id.clone(),
                    name: name.unwrap_or_else(|| {
                        directory
                            .file_name()
                            .and_then(|v| v.to_str())
                            .unwrap_or("Invalid panel")
                            .into()
                    }),
                    version: version.unwrap_or_else(|| "0.0.0".into()),
                    kind: PluginKind::Panel,
                    description: format!("Invalid panel: {error}"),
                    permissions: Vec::new(),
                    enabled_by_default: false,
                    can_toggle: true,
                };
                let plugin = InvalidPanel {
                    manifest: Manifest {
                        id,
                        required: Vec::new(),
                        optional: Vec::new(),
                        provides: Vec::new(),
                    },
                    error,
                };
                host.register(descriptor, move || Box::new(plugin.clone()));
            }
        }
    }
    Ok(())
}

#[allow(clippy::type_complexity)]
fn parse_panel(
    directory: &Path,
    text: &str,
) -> Result<(PanelMetadata, PanelPackage), (Option<String>, Option<String>, Option<String>, String)>
{
    let parsed: PanelManifest =
        toml::from_str(text).map_err(|error| (None, None, None, error.to_string()))?;
    let metadata = parsed.plugin;
    let context = (
        Some(metadata.id.clone()),
        Some(metadata.name.clone()),
        Some(metadata.version.clone()),
    );
    if metadata.api != API_VERSION {
        return Err((
            context.0,
            context.1,
            context.2,
            format!(
                "unsupported plugin API {}; expected {API_VERSION}",
                metadata.api
            ),
        ));
    }
    if metadata.kind != "panel" {
        return Err((
            context.0,
            context.1,
            context.2,
            "plugin.kind must be 'panel'".into(),
        ));
    }
    if metadata.id.starts_with("lumen.") {
        return Err((
            context.0,
            context.1,
            context.2,
            "panel IDs under the reserved lumen.* namespace are not allowed".into(),
        ));
    }
    let package = PanelPackage::new(metadata.id.clone(), directory, parsed.permissions.network)
        .map_err(|error| {
            (
                context.0.clone(),
                context.1.clone(),
                context.2.clone(),
                format!("invalid panel package: {error:?}"),
            )
        })?;
    package
        .response(&format!("lumen-plugin://{}/", metadata.id), "GET")
        .map_err(|error| {
            (
                context.0,
                context.1,
                context.2,
                format!("missing or invalid index.html: {error:?}"),
            )
        })?;
    Ok((metadata, package))
}

fn invalid_id(directory: &Path) -> String {
    let slug = directory
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("unknown");
    format!(
        "invalid.panel.{}",
        slug.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            })
            .collect::<String>()
    )
}
