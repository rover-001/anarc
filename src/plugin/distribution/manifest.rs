//! Manifest metadata and permission summary extraction for plugin packages.

use std::collections::BTreeMap;

use super::validation::safe_plugin_id;
use super::{ANARC_MANIFEST_NAME, MANIFEST_NAME, MAX_MANIFEST_BYTES, PackageError, PackageInfo};

pub(super) fn package_info(
    files: &BTreeMap<String, Vec<u8>>,
    signer_public_key: Vec<u8>,
    signature_valid: bool,
    signature_trusted: bool,
) -> Result<PackageInfo, PackageError> {
    let manifest = files
        .get(ANARC_MANIFEST_NAME)
        .or_else(|| files.get(MANIFEST_NAME))
        .ok_or_else(|| {
            PackageError(format!(
                "package is missing manifest ({MANIFEST_NAME} or {ANARC_MANIFEST_NAME})"
            ))
        })?;
    if manifest.len() > MAX_MANIFEST_BYTES {
        return Err(PackageError("plugin manifest is too large".into()));
    }
    let manifest_toml = std::str::from_utf8(manifest)
        .map_err(|_| PackageError("plugin manifest is not valid UTF-8".into()))?
        .to_owned();
    let value: toml::Value = toml::from_str(&manifest_toml)
        .map_err(|error| PackageError(format!("invalid plugin manifest: {error}")))?;
    let plugin = value
        .get("plugin")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| PackageError("manifest is missing [plugin]".into()))?;
    let id = required_plugin_field(plugin, "id")?;
    if !safe_plugin_id(&id) {
        return Err(PackageError(format!("unsafe plugin id: {id}")));
    }
    let name = required_plugin_field(plugin, "name")?;
    let version = required_plugin_field(plugin, "version")?;
    let kind = required_plugin_field(plugin, "kind")?;
    let api = required_plugin_field(plugin, "api")?;
    if api != "1" {
        return Err(PackageError(format!(
            "unsupported plugin API version: {api}"
        )));
    }
    if !matches!(kind.as_str(), "pack" | "wasm" | "panel") {
        return Err(PackageError(format!(
            "packages cannot install plugin kind {kind:?}"
        )));
    }
    let permission_summary = manifest_permission_summary(&value);
    Ok(PackageInfo {
        id,
        name,
        version,
        kind,
        manifest_toml,
        permission_summary,
        signer_public_key,
        signature_valid,
        signature_trusted,
    })
}

fn manifest_permission_summary(value: &toml::Value) -> Vec<String> {
    let mut summary = value
        .get("permissions")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(key, value)| format!("{key} = {value}"))
                .collect::<Vec<_>>()
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

fn required_plugin_field(
    table: &toml::map::Map<String, toml::Value>,
    field: &str,
) -> Result<String, PackageError> {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| PackageError(format!("manifest plugin.{field} is missing")))
}
