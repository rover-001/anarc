//! Bounded ZIP reading, writing, source collection, and atomic staging helpers.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::validation::{extraction_target, validate_entries, validate_relative_path};
use super::{
    ANARC_SIGNATURE_NAME, MAX_PACKAGE_ARCHIVE_BYTES, MAX_PACKAGE_ENTRIES,
    MAX_PACKAGE_UNCOMPRESSED_BYTES, MAX_SIGNATURE_BYTES, PackageEntry, PackageError,
    SIGNATURE_NAME,
};

pub(super) fn read_zip(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    let mut archive = open_bounded_zip(path)?;
    let entries = collect_zip_entries(&mut archive)?;
    validate_entries(&entries)?;
    read_zip_contents(&mut archive)
}

fn open_bounded_zip(path: &Path) -> Result<ZipArchive<File>, PackageError> {
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
    let archive = ZipArchive::new(file)
        .map_err(|error| PackageError(format!("{}: {error}", path.display())))?;
    if archive.len() > MAX_PACKAGE_ENTRIES {
        return Err(PackageError(format!(
            "package has more than {MAX_PACKAGE_ENTRIES} entries"
        )));
    }
    Ok(archive)
}

fn collect_zip_entries(archive: &mut ZipArchive<File>) -> Result<Vec<PackageEntry>, PackageError> {
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
        if (entry.name() == SIGNATURE_NAME || entry.name() == ANARC_SIGNATURE_NAME)
            && entry.size() > MAX_SIGNATURE_BYTES
        {
            return Err(PackageError("signature metadata is too large".into()));
        }
        entries.push(PackageEntry {
            path: entry.name().to_owned(),
            uncompressed_size: entry.size(),
            is_symlink: entry.is_symlink(),
        });
    }
    Ok(entries)
}

fn read_zip_contents(
    archive: &mut ZipArchive<File>,
) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
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

pub(super) fn collect_source_files(
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

pub(super) fn write_zip(
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
    for sig_name in [ANARC_SIGNATURE_NAME, SIGNATURE_NAME] {
        archive
            .start_file(sig_name, options)
            .map_err(|error| PackageError(format!("could not write signature: {error}")))?;
        archive
            .write_all(signature)
            .map_err(|error| PackageError(format!("could not write signature: {error}")))?;
    }
    archive
        .finish()
        .map_err(|error| PackageError(format!("could not finish package: {error}")))?;
    Ok(())
}

pub(super) fn extract_files(
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

pub(super) fn create_and_canonicalize(root: &Path) -> Result<PathBuf, PackageError> {
    fs::create_dir_all(root)
        .map_err(|error| PackageError(format!("{}: {error}", root.display())))?;
    root.canonicalize()
        .map_err(|error| PackageError(format!("{}: {error}", root.display())))
}

pub(super) fn unique_sibling_path(path: &Path, suffix: &str) -> Result<PathBuf, PackageError> {
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
