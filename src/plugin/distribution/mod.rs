//! Validation, signing, and atomic installation for `.lumenplugin` / `.anarcplugin` archives.

mod archive;
mod manifest;
mod signing;
#[cfg(test)]
mod tests;
mod validation;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

use archive::{
    collect_source_files, create_and_canonicalize, extract_files, read_zip, unique_sibling_path,
    write_zip,
};
use manifest::package_info;
#[cfg(test)]
pub(super) use signing::generate_signing_key;
use signing::{canonical_package_bytes, encode_hex, parse_signature};
pub(crate) use validation::validate_entries;
#[cfg(test)]
pub(super) use validation::validate_relative_path;

pub(crate) const MAX_PACKAGE_ENTRIES: usize = 10_000;
pub(crate) const MAX_PACKAGE_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
pub(super) const MAX_PACKAGE_ARCHIVE_BYTES: u64 = 160 * 1024 * 1024;
pub(crate) const MAX_PACKAGE_PATH_BYTES: usize = 1024;
pub(super) const MANIFEST_NAME: &str = "lumen-plugin.toml";
pub(super) const ANARC_MANIFEST_NAME: &str = "anarc-plugin.toml";
pub(super) const SIGNATURE_NAME: &str = "META-INF/lumen.signature";
pub(super) const ANARC_SIGNATURE_NAME: &str = "META-INF/anarc.signature";
pub(super) const MAX_SIGNATURE_BYTES: u64 = 1024;
pub(super) const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

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

struct SignedPackagePayload {
    signer_key: Vec<u8>,
    signature_bytes: Vec<u8>,
    canonical: Vec<u8>,
}

fn read_signature_and_canonical_payload(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<SignedPackagePayload, PackageError> {
    let signature = files
        .get(ANARC_SIGNATURE_NAME)
        .or_else(|| files.get(SIGNATURE_NAME))
        .ok_or_else(|| PackageError("package is missing signature".into()))?;
    let (signer_key, signature_bytes) = parse_signature(signature)?;
    let canonical = canonical_package_bytes(files.iter().filter(|(path, _)| {
        path.as_str() != ANARC_SIGNATURE_NAME && path.as_str() != SIGNATURE_NAME
    }))?;
    Ok(SignedPackagePayload {
        signer_key,
        signature_bytes,
        canonical,
    })
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
    if files.contains_key(SIGNATURE_NAME) || files.contains_key(ANARC_SIGNATURE_NAME) {
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
        "ANARC-PLUGIN-SIGNATURE-1\npub={}\nsig={}\n",
        encode_hex(pair.public_key().as_ref()),
        encode_hex(signature.as_ref()),
    );

    let parent = destination
        .parent()
        .ok_or_else(|| PackageError("destination has no parent directory".into()))?;
    fs::create_dir_all(parent)
        .map_err(|error| PackageError(format!("{}: {error}", parent.display())))?;
    let temp_path = unique_sibling_path(destination, "partial")?;
    if let Err(error) = write_zip(&temp_path, &files, signature_file.as_bytes()) {
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
    let SignedPackagePayload {
        signer_key,
        signature_bytes,
        canonical,
    } = read_signature_and_canonical_payload(&files)?;
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
    let SignedPackagePayload {
        signer_key,
        signature_bytes,
        canonical,
    } = read_signature_and_canonical_payload(&files)?;
    if signer_key != trusted_public_key {
        return Err(PackageError("package signer is not trusted".into()));
    }
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
    let staging = create_private_staging_dir(&destination)?;
    if let Err(error) = extract_files(&staging, &files) {
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

fn create_private_staging_dir(destination: &Path) -> Result<PathBuf, PackageError> {
    let staging = unique_sibling_path(destination, "install")?;
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
    Ok(staging)
}
