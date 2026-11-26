//! Discovery, manifest parsing, and `Plugin` trait adapters for WebAssembly packages.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::host_bridge::{HostApi, HostRequest, HostResponse};
use super::permissions::PermissionGrants;
use super::worker::WasmWorker;
use crate::plugin::{Manifest, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind};

#[derive(Clone)]
struct WasmPackage {
    id: String,
    name: String,
    version: String,
    description: String,
    directory: PathBuf,
    component: PathBuf,
    permission_summary: Vec<String>,
}

/// Register installed WebAssembly packages as disabled manager entries.
/// Grants intentionally start empty; an approval flow must provide explicit
/// grants before packages that request capabilities can activate.
pub(crate) fn register_wasm_plugins(host: &mut PluginHost, root: &Path) -> Result<(), String> {
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
        if !entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        register_wasm_entry(host, &entry.path());
    }
    Ok(())
}

fn register_wasm_entry(host: &mut PluginHost, directory: &Path) {
    let manifest_path = if directory.join("anarc-plugin.toml").is_file() {
        directory.join("anarc-plugin.toml")
    } else {
        directory.join("lumen-plugin.toml")
    };
    let Ok(source) = fs::read_to_string(&manifest_path) else {
        return;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&source) else {
        return;
    };
    let Some(plugin) = value.get("plugin").and_then(toml::Value::as_table) else {
        return;
    };
    if plugin.get("kind").and_then(toml::Value::as_str) != Some("wasm") {
        return;
    }

    match parse_wasm_package(directory, &value) {
        Ok(package) => {
            let descriptor = PluginDescriptor {
                id: package.id.clone(),
                name: package.name.clone(),
                version: package.version.clone(),
                kind: PluginKind::Wasm,
                description: package.description.clone(),
                permissions: package.permission_summary.clone(),
                enabled_by_default: false,
                can_toggle: true,
            };
            host.register(descriptor, move || {
                Box::new(WasmPlugin::new(package.clone()))
            });
        }
        Err(error) => register_invalid_wasm_plugin(host, directory, plugin, error),
    }
}

fn register_invalid_wasm_plugin(
    host: &mut PluginHost,
    directory: &Path,
    plugin: &toml::map::Map<String, toml::Value>,
    error: String,
) {
    let id = plugin
        .get("id")
        .and_then(toml::Value::as_str)
        .filter(|id| safe_plugin_id(id))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("invalid.wasm.{}", path_slug(directory)));
    let descriptor = PluginDescriptor {
        id: id.clone(),
        name: plugin
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or("Invalid WebAssembly plugin")
            .to_owned(),
        version: plugin
            .get("version")
            .and_then(toml::Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        kind: PluginKind::Wasm,
        description: format!("Invalid WebAssembly package: {error}"),
        permissions: Vec::new(),
        enabled_by_default: false,
        can_toggle: true,
    };
    let invalid = InvalidWasmPlugin {
        manifest: Manifest {
            id,
            ..Manifest::default()
        },
        error,
    };
    host.register(descriptor, move || Box::new(invalid.clone()));
}

fn parse_wasm_package(directory: &Path, value: &toml::Value) -> Result<WasmPackage, String> {
    let plugin = value
        .get("plugin")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "missing [plugin] table".to_owned())?;
    let field = |name: &str| {
        plugin
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("missing plugin.{name}"))
    };
    let id = field("id")?;
    if !safe_plugin_id(&id) {
        return Err(format!("invalid plugin ID {id:?}"));
    }
    if field("api")? != "1" {
        return Err("unsupported plugin API version".into());
    }
    let entry = field("entry")?;
    let relative = Path::new(&entry);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("plugin entry must be a relative path inside its package".into());
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve package directory: {error}"))?;
    let component = directory.join(relative);
    let canonical_component = component
        .canonicalize()
        .map_err(|error| format!("cannot resolve component entry: {error}"))?;
    if !canonical_component.starts_with(&directory) || !canonical_component.is_file() {
        return Err("component entry must be a regular file inside its package".into());
    }
    let permission_summary = value
        .get("permissions")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(name, value)| format!("{name} = {value}"))
                .collect()
        })
        .unwrap_or_default();
    Ok(WasmPackage {
        id,
        name: field("name")?,
        version: field("version")?,
        description: plugin
            .get("description")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        directory,
        component: canonical_component,
        permission_summary,
    })
}

fn safe_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
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
struct InvalidWasmPlugin {
    manifest: Manifest,
    error: String,
}

impl Plugin for InvalidWasmPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, _: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        Err(PluginError(self.error.clone()))
    }
}

struct WasmPlugin {
    package: WasmPackage,
    worker: Arc<Mutex<Option<WasmWorker>>>,
    manifest: Manifest,
}

impl WasmPlugin {
    fn new(package: WasmPackage) -> Self {
        Self {
            manifest: Manifest {
                id: package.id.clone(),
                ..Manifest::default()
            },
            package,
            worker: Arc::new(Mutex::new(None)),
        }
    }
}

/// The current manager supplies no permission approval UI or main-thread
/// service bridge. This adapter therefore grants nothing and rejects every
/// host request. Integrators may provide a bounded dispatcher only after the
/// user's explicit grants have been persisted and reviewed.
struct DeniedHostApi;

impl HostApi for DeniedHostApi {
    fn dispatch(&self, _request: HostRequest) -> Result<HostResponse, String> {
        Err("host service adapter is not connected; request denied".into())
    }
}

impl Plugin for WasmPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let worker = WasmWorker::spawn(
            &self.package.component,
            self.package.directory.join(".lumen-data"),
            String::new(),
            PermissionGrants::default(),
            Arc::new(DeniedHostApi),
        )
        .map_err(|error| PluginError(error.to_string()))?;
        let slot = Arc::clone(&self.worker);
        if let Ok(mut current) = slot.lock() {
            *current = Some(worker);
        }
        ctx.effect(move || {
            if let Ok(mut current) = slot.lock()
                && let Some(worker) = current.take()
            {
                worker.stop();
            }
        });
        Ok(())
    }
}
