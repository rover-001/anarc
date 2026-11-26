//! Archive path, size, symlink, and plugin-ID validation.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use super::{
    ANARC_MANIFEST_NAME, MANIFEST_NAME, MAX_PACKAGE_ENTRIES, MAX_PACKAGE_PATH_BYTES,
    MAX_PACKAGE_UNCOMPRESSED_BYTES, PackageEntry, PackageError,
};

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
        if normalized == MANIFEST_NAME || normalized == ANARC_MANIFEST_NAME {
            has_manifest = true;
        }
        if !normalized_names.insert(normalized) {
            return Err(PackageError(format!(
                "duplicate package path (case-insensitive): {}",
                entry.path
            )));
        }
    }
    check_file_directory_conflicts(&normalized_names)?;
    if !has_manifest {
        return Err(PackageError(format!("package is missing {MANIFEST_NAME}")));
    }
    Ok(())
}

fn check_file_directory_conflicts(normalized_names: &HashSet<String>) -> Result<(), PackageError> {
    for name in normalized_names {
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

pub(crate) fn safe_plugin_id(id: &str) -> bool {
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
