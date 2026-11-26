//! Ed25519 key generation, canonical package serialization, and signature parsing.

use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};

use super::validation::validate_relative_path;
use super::{MAX_SIGNATURE_BYTES, PackageError};

/// Generate a new Ed25519 PKCS#8 private key and its raw public key.
pub(crate) fn generate_signing_key() -> Result<(Vec<u8>, Vec<u8>), PackageError> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| PackageError("could not generate Ed25519 key".into()))?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| PackageError("generated Ed25519 key was invalid".into()))?;
    Ok((pkcs8.as_ref().to_vec(), pair.public_key().as_ref().to_vec()))
}

pub(super) fn canonical_package_bytes<'a>(
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

pub(super) fn parse_signature(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), PackageError> {
    if bytes.len() as u64 > MAX_SIGNATURE_BYTES {
        return Err(PackageError("signature metadata is too large".into()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| PackageError("signature metadata is not UTF-8".into()))?;
    let mut lines = text.lines();
    let header = lines.next();
    if header != Some("ANARC-PLUGIN-SIGNATURE-1") && header != Some("LUMEN-PLUGIN-SIGNATURE-1") {
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

pub(super) fn encode_hex(bytes: &[u8]) -> String {
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
        .as_chunks::<2>()
        .0
        .iter()
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
