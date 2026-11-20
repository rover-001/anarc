use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const MANIFEST_NAME: &str = "lumen-plugin.toml";
const SIGNATURE_NAME: &str = "META-INF/lumen.signature";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: lumen-signer <source_plugin_dir> <output.lumenplugin>");
        std::process::exit(1);
    }

    let source = PathBuf::from(&args[1]).canonicalize()?;
    let destination = PathBuf::from(&args[2]);

    if !source.join(MANIFEST_NAME).is_file() {
        eprintln!("Error: source directory does not contain {}", MANIFEST_NAME);
        std::process::exit(1);
    }

    // Collect all files
    let mut files = BTreeMap::new();
    collect_files(&source, &source, &mut files)?;

    if files.contains_key(SIGNATURE_NAME) {
        eprintln!("Error: source must not contain {}", SIGNATURE_NAME);
        std::process::exit(1);
    }

    // Canonical bytes
    let mut canonical = Vec::new();
    for (path, content) in &files {
        let path_len = u32::try_from(path.len())?;
        canonical.extend_from_slice(&path_len.to_be_bytes());
        canonical.extend_from_slice(path.as_bytes());
        let content_len = content.len() as u64;
        canonical.extend_from_slice(&content_len.to_be_bytes());
        canonical.extend_from_slice(content);
    }

    // Generate Ed25519 key
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|e| format!("Ed25519 key generation failed: {e:?}"))?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|e| format!("Ed25519 key pair failed: {e:?}"))?;

    // Sign canonical bytes
    let sig = key_pair.sign(&canonical);
    let pub_hex = hex_encode(key_pair.public_key().as_ref());
    let sig_hex = hex_encode(sig.as_ref());

    let signature_content = format!(
        "LUMEN-PLUGIN-SIGNATURE-1\npub={}\nsig={}\n",
        pub_hex, sig_hex
    );

    // Create zip
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(&destination)?;
    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // Write regular files
    for (path, content) in &files {
        zip.start_file(path, options)?;
        zip.write_all(content)?;
    }

    // Write signature
    zip.start_file(SIGNATURE_NAME, options)?;
    zip.write_all(signature_content.as_bytes())?;

    zip.finish()?;
    println!("Successfully exported signed plugin package to {}", destination.display());
    println!("Public key fingerprint: {}", &pub_hex[..16]);
    Ok(())
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root)?.to_str().unwrap().replace('\\', "/");
            // Skip .lumenplugin archives or target directories
            if rel.ends_with(".lumenplugin") || rel.contains("target/") || rel.contains(".git/") {
                continue;
            }
            let mut content = Vec::new();
            File::open(&path)?.read_to_end(&mut content)?;
            files.insert(rel, content);
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
