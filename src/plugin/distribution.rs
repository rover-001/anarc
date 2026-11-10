//! Validation primitives for untrusted `.lumenplugin` archives.
//!
//! Archive parsing/extraction and signing are intentionally left to the host
//! integration. Call `validate_entries` before writing any archive member.

use std::path::{Component, Path, PathBuf};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

use ring::{
    rand::SystemRandom,
    signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) const MAX_PACKAGE_ENTRIES: usize = 10_000;
pub(crate) const MAX_PACKAGE_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PACKAGE_ARCHIVE_BYTES: u64 = 160 * 1024 * 1024;
pub(crate) const MAX_PACKAGE_PATH_BYTES: usize = 1024;
const MANIFEST_NAME: &str = "lumen-plugin.toml";
const SIGNATURE_NAME: &str = "META-INF/lumen.signature";
const MAX_SIGNATURE_BYTES: u64 = 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackageEntry {
    /// Archive-relative path using `/` separators.
    pub(crate) path: String,
    /// Uncompressed byte size reported by the archive parser.
    pub(crate) uncompressed_size: u64,
    /// True for entries whose archive metadata marks them as symbolic links.
    pub(crate) is_symlink: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackageError(pub(crate) String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackageInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) kind: String,
    pub(crate) manifest_toml: String,
    pub(crate) permission_summary: Vec<String>,
    pub(crate) signer_public_key: Vec<u8>,
    pub(crate) signature_valid: bool,
    pub(crate) signature_trusted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstalledPackage {
    pub(crate) path: PathBuf,
    pub(crate) info: PackageInfo,
}

/// Validate archive names and resource limits before extraction.
///
/// This does not parse ZIP data. The caller must provide the path, uncompressed
/// size, and symlink bit obtained from a trusted ZIP parser, then extract only
/// the returned validated relative paths.
pub(crate) fn validate_entries(entries: &[PackageEntry]) -> Result<(), PackageError> {
    if entries.is_empty() {
        return Err(PackageError("package archive is empty".into()));
    }
    if entries.len() > MAX_PACKAGE_ENTRIES {
        return Err(PackageError(format!(
            "package has more than {MAX_PACKAGE_ENTRIES} entries"
        )));
    }

    let mut total_size = 0u64;
    let mut normalized_names = HashSet::with_capacity(entries.len());
    let mut has_manifest = false;
    for entry in entries {
        let relative = validate_relative_path(&entry.path)?;
        if entry.is_symlink {
            return Err(PackageError(format!(
                "symbolic links are not allowed in packages: {}",
                entry.path
            )));
        }
        total_size = total_size
            .checked_add(entry.uncompressed_size)
            .filter(|size| *size <= MAX_PACKAGE_UNCOMPRESSED_BYTES)
            .ok_or_else(|| {
                PackageError(format!(
                    "package expands beyond {MAX_PACKAGE_UNCOMPRESSED_BYTES} bytes"
                ))
            })?;

        let normalized = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        if normalized == MANIFEST_NAME {
            has_manifest = true;
        }
        if !normalized_names.insert(normalized.clone()) {
            return Err(PackageError(format!(
                "duplicate package path (case-insensitive): {}",
                entry.path
            )));
        }
    }
    for name in &normalized_names {
        let mut prefix = String::new();
        for component in name
            .split('/')
            .take(name.split('/').count().saturating_sub(1))
        {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if normalized_names.contains(&prefix) {
                return Err(PackageError(format!(
                    "package path is both a file and a directory: {prefix}"
                )));
            }
        }
    }
    if !has_manifest {
        return Err(PackageError(format!("package is missing {MANIFEST_NAME}")));
    }
    Ok(())
}

/// Return a safe relative path, rejecting platform-specific traversal forms.
pub(crate) fn validate_relative_path(value: &str) -> Result<PathBuf, PackageError> {
    if value.is_empty()
        || value.len() > MAX_PACKAGE_PATH_BYTES
        || value.contains('\0')
        || value.contains('\\')
        || value.starts_with('/')
        || value.contains(':')
    {
        return Err(PackageError(format!("invalid package path: {value:?}")));
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(PackageError(format!("invalid package path: {value:?}")));
    }

    let path = Path::new(value);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => safe.push(part),
            _ => return Err(PackageError(format!("invalid package path: {value:?}"))),
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(PackageError(format!("invalid package path: {value:?}")));
    }
    Ok(safe)
}

/// Resolve a validated archive path below an existing extraction root.
///
/// The caller still must create files with no-follow semantics (or extract into
/// a fresh private directory) to prevent pre-existing symlink races.
pub(crate) fn extraction_target(root: &Path, archive_path: &str) -> Result<PathBuf, PackageError> {
    let relative = validate_relative_path(archive_path)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| PackageError(format!("{}: {error}", root.display())))?;
    let target = canonical_root.join(relative);
    if !target.starts_with(&canonical_root) {
        return Err(PackageError(format!(
            "package path escapes root: {archive_path}"
        )));
    }
    Ok(target)
}

/// Generate a new Ed25519 PKCS#8 private key and its raw public key.
pub(crate) fn generate_signing_key() -> Result<(Vec<u8>, Vec<u8>), PackageError> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| PackageError("could not generate Ed25519 key".into()))?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| PackageError("generated Ed25519 key was invalid".into()))?;
    Ok((pkcs8.as_ref().to_vec(), pair.public_key().as_ref().to_vec()))
}

/// Create a signed `.lumenplugin` ZIP from a directory containing a manifest.
/// Existing destination files are left untouched.
pub(crate) fn export_package(
    source_root: &Path,
    destination: &Path,
    signing_key_pkcs8: &[u8],
) -> Result<(), PackageError> {
    let root = source_root
        .canonicalize()
        .map_err(|error| PackageError(format!("{}: {error}", source_root.display())))?;
    if !root.is_dir() {
        return Err(PackageError("package source is not a directory".into()));
    }
    let pair = Ed25519KeyPair::from_pkcs8(signing_key_pkcs8)
        .map_err(|_| PackageError("invalid Ed25519 PKCS#8 signing key".into()))?;
    let mut files = BTreeMap::new();
    let mut total_size = 0u64;
    collect_source_files(&root, &root, &mut files, &mut total_size)?;
    if files.contains_key(SIGNATURE_NAME) {
        return Err(PackageError(format!(
            "source must not contain reserved path {SIGNATURE_NAME}"
        )));
    }
    let entries = files
        .iter()
        .map(|(path, bytes)| PackageEntry {
            path: path.clone(),
            uncompressed_size: bytes.len() as u64,
            is_symlink: false,
        })
        .collect::<Vec<_>>();
    validate_entries(&entries)?;
    let canonical = canonical_package_bytes(files.iter())?;
    let signature = pair.sign(&canonical);
    let signature_file = format!(
        "LUMEN-PLUGIN-SIGNATURE-1\npub={}\nsig={}\n",
        encode_hex(pair.public_key().as_ref()),
        encode_hex(signature.as_ref()),
    );

    let parent = destination
        .parent()
        .ok_or_else(|| PackageError("destination has no parent directory".into()))?;
    fs::create_dir_all(parent)
        .map_err(|error| PackageError(format!("{}: {error}", parent.display())))?;
    let temp_path = unique_sibling_path(destination, "partial")?;
    let result = write_zip(&temp_path, &files, signature_file.as_bytes());
    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = fs::hard_link(&temp_path, destination) {
        let _ = fs::remove_file(&temp_path);
        return Err(PackageError(format!("{}: {error}", destination.display())));
    }
    let _ = fs::remove_file(&temp_path);
    Ok(())
}

/// Inspect and validate a package. With no trusted key, signature validity is
/// reported as false so callers can display publisher details before asking
/// the user to trust that key. Supplying a trusted key requires an exact key
/// match and valid signature.
pub(crate) fn inspect_package(
    package_path: &Path,
    trusted_public_key: Option<&[u8]>,
) -> Result<PackageInfo, PackageError> {
    let files = read_zip(package_path)?;
    let signature = files
        .get(SIGNATURE_NAME)
        .ok_or_else(|| PackageError(format!("package is missing {SIGNATURE_NAME}")))?;
    let (signer_key, signature_bytes) = parse_signature(signature)?;
    let canonical = canonical_package_bytes(
        files
            .iter()
            .filter(|(path, _)| path.as_str() != SIGNATURE_NAME),
    )?;
    let signature_valid = UnparsedPublicKey::new(&ED25519, signer_key.as_slice())
        .verify(&canonical, &signature_bytes)
        .is_ok();
    let signature_trusted =
        signature_valid && trusted_public_key.is_some_and(|trusted| trusted == signer_key);
    package_info(&files, signer_key, signature_valid, signature_trusted)
}

/// Verify against a previously trusted public key, then extract atomically to
/// `<plugin_root>/<manifest plugin.id>`. Existing installations are not
/// replaced by this helper.
pub(crate) fn install_package(
    package_path: &Path,
    plugin_root: &Path,
    trusted_public_key: &[u8],
) -> Result<InstalledPackage, PackageError> {
    let files = read_zip(package_path)?;
    let signature = files
        .get(SIGNATURE_NAME)
        .ok_or_else(|| PackageError(format!("package is missing {SIGNATURE_NAME}")))?;
    let (signer_key, signature_bytes) = parse_signature(signature)?;
    if signer_key != trusted_public_key {
        return Err(PackageError("package signer is not trusted".into()));
    }
    let canonical = canonical_package_bytes(
        files
            .iter()
            .filter(|(path, _)| path.as_str() != SIGNATURE_NAME),
    )?;
    UnparsedPublicKey::new(&ED25519, trusted_public_key)
        .verify(&canonical, &signature_bytes)
        .map_err(|_| PackageError("package signature verification failed".into()))?;
    let info = package_info(&files, signer_key, true, true)?;
    let plugin_root = create_and_canonicalize(plugin_root)?;
    let destination = plugin_root.join(&info.id);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(PackageError(format!(
            "plugin is already installed: {}",
            info.id
        )));
    }
    let staging = unique_sibling_path(&destination, "install")?;
    fs::create_dir(&staging)
        .map_err(|error| PackageError(format!("{}: {error}", staging.display())))?;
    #[cfg(unix)]
    if let Err(error) = fs::set_permissions(
        &staging,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    ) {
        let _ = fs::remove_dir(&staging);
        return Err(PackageError(format!("{}: {error}", staging.display())));
    }
    let extraction = extract_files(&staging, &files);
    if let Err(error) = extraction {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(PackageError(format!("{}: {error}", destination.display())));
    }
    Ok(InstalledPackage {
        path: destination,
        info,
    })
}

fn read_zip(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    let file =
        File::open(path).map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
    let archive_size = file
        .metadata()
        .map_err(|error| PackageError(format!("{}: {error}", path.display())))?
        .len();
    if archive_size > MAX_PACKAGE_ARCHIVE_BYTES {
        return Err(PackageError(format!(
            "package archive exceeds {MAX_PACKAGE_ARCHIVE_BYTES} bytes"
        )));
    }
    let mut archive = ZipArchive::new(file)
        .map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
    if archive.len() > MAX_PACKAGE_ENTRIES {
        return Err(PackageError(format!(
            "package has more than {MAX_PACKAGE_ENTRIES} entries"
        )));
    }
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| PackageError(format!("invalid ZIP entry {index}: {error}")))?;
        if entry.is_dir() {
            return Err(PackageError(format!(
                "explicit directory entries are not allowed: {}",
                entry.name()
            )));
        }
        if entry.encrypted() {
            return Err(PackageError(format!(
                "encrypted ZIP entries are not allowed: {}",
                entry.name()
            )));
        }
        if entry.name() == SIGNATURE_NAME && entry.size() > MAX_SIGNATURE_BYTES {
            return Err(PackageError("signature metadata is too large".into()));
        }
        entries.push(PackageEntry {
            path: entry.name().to_owned(),
            uncompressed_size: entry.size(),
            is_symlink: entry.is_symlink(),
        });
    }
    validate_entries(&entries)?;

    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| PackageError(format!("invalid ZIP entry {index}: {error}")))?;
        let name = entry.name().to_owned();
        let reported_size = entry.size();
        let mut content = Vec::with_capacity(reported_size as usize);
        entry
            .take(MAX_PACKAGE_UNCOMPRESSED_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|error| PackageError(format!("could not read {name}: {error}")))?;
        if content.len() as u64 != reported_size {
            return Err(PackageError(format!("ZIP size mismatch for {name}")));
        }
        if files.insert(name.clone(), content).is_some() {
            return Err(PackageError(format!("duplicate ZIP entry: {name}")));
        }
    }
    Ok(files)
}

fn package_info(
    files: &BTreeMap<String, Vec<u8>>,
    signer_public_key: Vec<u8>,
    signature_valid: bool,
    signature_trusted: bool,
) -> Result<PackageInfo, PackageError> {
    let manifest = files
        .get(MANIFEST_NAME)
        .ok_or_else(|| PackageError(format!("package is missing {MANIFEST_NAME}")))?;
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

fn safe_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id.split('.').count() >= 2
        && id.split('.').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
}

fn canonical_package_bytes<'a>(
    files: impl IntoIterator<Item = (&'a String, &'a Vec<u8>)>,
) -> Result<Vec<u8>, PackageError> {
    let mut canonical = Vec::new();
    for (path, content) in files {
        let path = validate_relative_path(path)?;
        let path = path
            .to_str()
            .ok_or_else(|| PackageError("package path is not valid UTF-8".into()))?;
        let path_length = u32::try_from(path.len())
            .map_err(|_| PackageError("package path is too long".into()))?;
        canonical.extend_from_slice(&path_length.to_be_bytes());
        canonical.extend_from_slice(path.as_bytes());
        canonical.extend_from_slice(&(content.len() as u64).to_be_bytes());
        canonical.extend_from_slice(content);
    }
    Ok(canonical)
}

fn parse_signature(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), PackageError> {
    if bytes.len() as u64 > MAX_SIGNATURE_BYTES {
        return Err(PackageError("signature metadata is too large".into()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PackageError("signature metadata is not UTF-8".into()))?;
    let mut lines = text.lines();
    if lines.next() != Some("LUMEN-PLUGIN-SIGNATURE-1") {
        return Err(PackageError("unsupported package signature format".into()));
    }
    let public_key = lines
        .next()
        .and_then(|line| line.strip_prefix("pub="))
        .ok_or_else(|| PackageError("signature metadata is missing pub".into()))?;
    let signature = lines
        .next()
        .and_then(|line| line.strip_prefix("sig="))
        .ok_or_else(|| PackageError("signature metadata is missing sig".into()))?;
    if lines.next().is_some() {
        return Err(PackageError(
            "signature metadata has unexpected fields".into(),
        ));
    }
    let public_key = decode_hex(public_key)?;
    let signature = decode_hex(signature)?;
    if public_key.len() != 32 || signature.len() != 64 {
        return Err(PackageError(
            "invalid Ed25519 key or signature length".into(),
        ));
    }
    Ok((public_key, signature))
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, PackageError> {
    if !value.len().is_multiple_of(2) {
        return Err(PackageError("invalid hex in signature metadata".into()));
    }
    value
        .as_bytes()
        .as_chunks::<2>().0.iter()
        .map(|chunk| {
            let high = hex_nibble(chunk[0])?;
            let low = hex_nibble(chunk[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8, PackageError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PackageError("invalid hex in signature metadata".into())),
    }
}

fn collect_source_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    total_size: &mut u64,
) -> Result<(), PackageError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| PackageError(format!("{}: {error}", directory.display())))?
        .map(|entry| entry.map_err(|error| PackageError(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
        if metadata.file_type().is_symlink() {
            return Err(PackageError(format!(
                "symbolic links are not allowed in packages: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_source_files(root, &path, files, total_size)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(PackageError(format!(
                "unsupported package entry: {}",
                path.display()
            )));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| PackageError(error.to_string()))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| PackageError("package path is not valid UTF-8".into()))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        validate_relative_path(&relative)?;
        *total_size = total_size
            .checked_add(metadata.len())
            .filter(|size| *size <= MAX_PACKAGE_UNCOMPRESSED_BYTES)
            .ok_or_else(|| {
                PackageError(format!(
                    "package expands beyond {MAX_PACKAGE_UNCOMPRESSED_BYTES} bytes"
                ))
            })?;
        let content = fs::read(&path)
            .map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
        files.insert(relative, content);
        if files.len() > MAX_PACKAGE_ENTRIES {
            return Err(PackageError(format!(
                "package has more than {MAX_PACKAGE_ENTRIES} entries"
            )));
        }
    }
    Ok(())
}

fn write_zip(
    path: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    signature: &[u8],
) -> Result<(), PackageError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, content) in files {
        archive
            .start_file(name, options)
            .map_err(|error| PackageError(format!("could not write {name}: {error}")))?;
        archive
            .write_all(content)
            .map_err(|error| PackageError(format!("could not write {name}: {error}")))?;
    }
    archive
        .start_file(SIGNATURE_NAME, options)
        .map_err(|error| PackageError(format!("could not write signature: {error}")))?;
    archive
        .write_all(signature)
        .map_err(|error| PackageError(format!("could not write signature: {error}")))?;
    archive
        .finish()
        .map_err(|error| PackageError(format!("could not finish package: {error}")))?;
    Ok(())
}

fn extract_files(
    staging_root: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), PackageError> {
    let root = staging_root
        .canonicalize()
        .map_err(|error| PackageError(format!("{}: {error}", staging_root.display())))?;
    for (archive_path, content) in files {
        let target = extraction_target(&root, archive_path)?;
        let parent = target
            .parent()
            .ok_or_else(|| PackageError(format!("invalid package target: {archive_path}")))?;
        fs::create_dir_all(parent)
            .map_err(|error| PackageError(format!("{}: {error}", parent.display())))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| PackageError(format!("{}: {error}", target.display())))?;
        output
            .write_all(content)
            .map_err(|error| PackageError(format!("{}: {error}", target.display())))?;
    }
    Ok(())
}

fn create_and_canonicalize(root: &Path) -> Result<PathBuf, PackageError> {
    fs::create_dir_all(root)
        .map_err(|error| PackageError(format!("{}: {error}", root.display())))?;
    root.canonicalize()
        .map_err(|error| PackageError(format!("{}: {error}", root.display())))
}

fn unique_sibling_path(path: &Path, suffix: &str) -> Result<PathBuf, PackageError> {
    let parent = path
        .parent()
        .ok_or_else(|| PackageError("path has no parent".into()))?;
    let name = path
        .file_name()
        .ok_or_else(|| PackageError("path has no file name".into()))?
        .to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| PackageError(error.to_string()))?
        .as_nanos();
    Ok(parent.join(format!(
        ".{name}.{}.{timestamp}.{suffix}",
        std::process::id()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn entry(path: &str, size: u64) -> PackageEntry {
        PackageEntry {
            path: path.into(),
            uncompressed_size: size,
            is_symlink: false,
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("lumen-package-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_traversal_absolute_drive_and_windows_separator_paths() {
        for path in [
            "../outside",
            "nested/../../outside",
            "/etc/passwd",
            "C:/outside",
            "nested\\outside",
            "",
            ".",
            "nested/./file",
        ] {
            assert!(validate_relative_path(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn accepts_only_bounded_archive_with_root_manifest_and_unique_names() {
        assert!(
            validate_entries(&[
                entry("lumen-plugin.toml", 20),
                entry("styles/theme.css", 100),
            ])
            .is_ok()
        );

        assert!(validate_entries(&[entry("theme.css", 10)]).is_err());
        assert!(
            validate_entries(&[
                entry("lumen-plugin.toml", 10),
                entry("Assets/icon.png", 10),
                entry("assets/icon.png", 10),
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_symlinks_and_expansion_bombs() {
        let mut symlink = entry("link", 0);
        symlink.is_symlink = true;
        assert!(validate_entries(&[entry("lumen-plugin.toml", 0), symlink]).is_err());

        assert!(
            validate_entries(&[
                entry("lumen-plugin.toml", MAX_PACKAGE_UNCOMPRESSED_BYTES),
                entry("large.bin", 1),
            ])
            .is_err()
        );
    }

    #[test]
    fn signed_package_exports_inspects_and_installs_with_trusted_key() {
        let temp = TestDirectory::new();
        let source = temp.0.join("source");
        fs::create_dir_all(source.join("assets")).unwrap();
        fs::write(
            source.join(MANIFEST_NAME),
            "[plugin]\nid='example.notes'\nname='Notes'\nversion='1.0.0'\napi='1'\nkind='pack'\n[permissions]\nstorage=true\n",
        )
        .unwrap();
        fs::write(source.join("assets/readme.txt"), "notes asset").unwrap();

        let (private_key, public_key) = generate_signing_key().unwrap();
        let package = temp.0.join("notes.lumenplugin");
        export_package(&source, &package, &private_key).unwrap();

        let untrusted = inspect_package(&package, None).unwrap();
        assert!(untrusted.signature_valid);
        assert!(!untrusted.signature_trusted);
        assert_eq!(untrusted.id, "example.notes");
        assert_eq!(untrusted.permission_summary, ["storage = true"]);

        let trusted = inspect_package(&package, Some(&public_key)).unwrap();
        assert!(trusted.signature_valid && trusted.signature_trusted);
        let other_key = generate_signing_key().unwrap().1;
        let mismatched = inspect_package(&package, Some(&other_key)).unwrap();
        assert!(mismatched.signature_valid);
        assert!(!mismatched.signature_trusted);

        let installed_root = temp.0.join("installed");
        let installed = install_package(&package, &installed_root, &public_key).unwrap();
        assert_eq!(installed.info.id, "example.notes");
        assert_eq!(
            fs::read(installed.path.join("assets/readme.txt")).unwrap(),
            b"notes asset"
        );
        assert!(install_package(&package, &installed_root, &public_key).is_err());
    }

    #[test]
    fn tampered_package_and_unsafe_zip_paths_are_rejected() {
        let temp = TestDirectory::new();
        let source = temp.0.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join(MANIFEST_NAME),
            "[plugin]\nid='example.test'\nname='Test'\nversion='1.0.0'\napi='1'\nkind='pack'\n",
        )
        .unwrap();
        fs::write(source.join("data.txt"), "original").unwrap();
        let (private_key, public_key) = generate_signing_key().unwrap();
        let package = temp.0.join("signed.lumenplugin");
        export_package(&source, &package, &private_key).unwrap();

        let mut original = ZipArchive::new(File::open(&package).unwrap()).unwrap();
        let mut files = BTreeMap::new();
        for index in 0..original.len() {
            let mut file = original.by_index(index).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            let name = file.name().to_owned();
            files.insert(name, bytes);
        }
        files.insert("data.txt".into(), b"tampered".to_vec());
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in &files {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        let tampered = temp.0.join("tampered.lumenplugin");
        fs::write(&tampered, writer.finish().unwrap().into_inner()).unwrap();
        let info = inspect_package(&tampered, Some(&public_key)).unwrap();
        assert!(!info.signature_valid);
        assert!(!info.signature_trusted);
        assert!(install_package(&tampered, &temp.0.join("bad-install"), &public_key).is_err());

        let mut unsafe_zip = ZipWriter::new(Cursor::new(Vec::new()));
        unsafe_zip
            .start_file("../outside", SimpleFileOptions::default())
            .unwrap();
        unsafe_zip.write_all(b"escape").unwrap();
        let unsafe_path = temp.0.join("unsafe.lumenplugin");
        fs::write(&unsafe_path, unsafe_zip.finish().unwrap().into_inner()).unwrap();
        assert!(read_zip(&unsafe_path).is_err());
    }
}
