use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::*;

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
        let path =
            std::env::temp_dir().join(format!("lumen-package-test-{}-{id}", std::process::id()));
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
