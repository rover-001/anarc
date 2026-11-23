//! Security boundary and static-resource resolver for web-panel plugins.
//!
//! The CEF scheme-factory adapter is intentionally kept at the engine boundary;
//! this module validates requests and prepares responses without exposing CEF
//! types to plugins.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

const MAX_PANEL_RESOURCE_BYTES: u64 = 8 * 1024 * 1024;

/// Shared registry used by the plugin host and CEF request handlers.
pub(crate) type PanelRegistryHandle = Arc<Mutex<std::collections::HashMap<String, PanelPackage>>>;

pub(crate) fn new_panel_registry() -> PanelRegistryHandle {
    Arc::new(Mutex::new(std::collections::HashMap::new()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PanelPackage {
    pub(crate) id: String,
    root: PathBuf,
    allowed_hosts: BTreeSet<String>,
}

impl PanelPackage {
    pub(crate) fn new(
        id: impl Into<String>,
        root: impl Into<PathBuf>,
        allowed_hosts: impl IntoIterator<Item = String>,
    ) -> Result<Self, PanelError> {
        let id = id.into();
        validate_plugin_id(&id)?;
        let root = fs::canonicalize(root.into()).map_err(|_| PanelError::NotFound)?;
        if !root.is_dir() {
            return Err(PanelError::NotFound);
        }
        let allowed_hosts = allowed_hosts
            .into_iter()
            .map(|host| normalize_allowed_host(&host))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id,
            root,
            allowed_hosts,
        })
    }

    pub(crate) fn allows_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if (host == "127.0.0.1" || host == "localhost")
            && (self.allowed_hosts.contains("localhost")
                || self.allowed_hosts.contains("127.0.0.1"))
        {
            return true;
        }
        self.allowed_hosts.iter().any(|pattern| {
            if let Some(suffix) = pattern.strip_prefix("*.") {
                host.len() > suffix.len()
                    && host.ends_with(suffix)
                    && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            } else {
                host == *pattern
            }
        })
    }

    pub(crate) fn allowed_hosts(&self) -> Vec<String> {
        self.allowed_hosts.iter().cloned().collect()
    }

    pub(crate) fn csp(&self) -> String {
        let network = self
            .allowed_hosts
            .iter()
            .map(|host| {
                if host == "localhost" || host == "127.0.0.1" {
                    format!("http://{host}:* ws://{host}:* http://localhost:* ws://localhost:*")
                } else if let Some(suffix) = host.strip_prefix("*.") {
                    format!("https://*.{suffix}")
                } else {
                    format!("https://{host}")
                }
            })
            .collect::<Vec<_>>();
        let connect = if network.is_empty() {
            "'self'".to_owned()
        } else {
            format!("'self' {}", network.join(" "))
        };
        format!(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: {network_sources}; font-src 'self' {network_sources}; connect-src {connect}; media-src 'self' {network_sources}; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            network_sources = network.join(" ")
        )
    }

    pub(crate) fn response(
        &self,
        request_url: &str,
        method: &str,
    ) -> Result<PanelResponse, PanelError> {
        if method != "GET" && method != "HEAD" {
            return Err(PanelError::MethodNotAllowed);
        }
        let request = PanelRequest::parse(request_url)?;
        if request.plugin_id != self.id {
            return Err(PanelError::UnknownPlugin);
        }
        let path = self.resolve_asset_path(&request.path)?;
        let metadata = fs::metadata(&path).map_err(|_| PanelError::NotFound)?;
        if !metadata.is_file() {
            return Err(PanelError::NotFound);
        }
        if metadata.len() > MAX_PANEL_RESOURCE_BYTES {
            return Err(PanelError::TooLarge);
        }
        let body = if method == "HEAD" {
            Vec::new()
        } else {
            fs::read(&path).map_err(|_| PanelError::NotFound)?
        };
        let mime_type = mime_type(&path).to_owned();
        Ok(PanelResponse {
            status: 200,
            mime_type,
            body,
            headers: vec![
                ("Content-Security-Policy".into(), self.csp()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
                ("X-Frame-Options".into(), "DENY".into()),
                ("Cache-Control".into(), "no-store".into()),
            ],
        })
    }

    fn resolve_asset_path(&self, resource_path: &str) -> Result<PathBuf, PanelError> {
        let decoded = percent_decode(resource_path)?;
        let relative = Path::new(&decoded);
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(PanelError::InvalidPath);
        }
        let relative = if decoded.is_empty() {
            Path::new("index.html")
        } else {
            relative
        };
        let path = fs::canonicalize(self.root.join(relative)).map_err(|_| PanelError::NotFound)?;
        if !path.starts_with(&self.root) {
            return Err(PanelError::InvalidPath);
        }
        Ok(path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PanelRequest {
    pub(crate) plugin_id: String,
    pub(crate) path: String,
}

impl PanelRequest {
    pub(crate) fn parse(url: &str) -> Result<Self, PanelError> {
        let rest = url
            .strip_prefix("anarc-plugin://")
            .or_else(|| url.strip_prefix("lumen-plugin://"))
            .ok_or(PanelError::InvalidUrl)?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.is_empty() || authority.contains(['@', ':', '%']) {
            return Err(PanelError::InvalidUrl);
        }
        validate_plugin_id(authority)?;
        let path = path.split(['?', '#']).next().unwrap_or_default();
        Ok(Self {
            plugin_id: authority.to_owned(),
            path: path.to_owned(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PanelResponse {
    pub(crate) status: u16,
    pub(crate) mime_type: String,
    pub(crate) body: Vec<u8>,
    pub(crate) headers: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelError {
    InvalidUrl,
    InvalidPluginId,
    InvalidHost,
    InvalidPath,
    UnknownPlugin,
    MethodNotAllowed,
    NotFound,
    TooLarge,
}

fn validate_plugin_id(id: &str) -> Result<(), PanelError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        || id.starts_with('.')
        || id.ends_with('.')
        || id.contains("..")
    {
        return Err(PanelError::InvalidPluginId);
    }
    Ok(())
}

fn normalize_allowed_host(host: &str) -> Result<String, PanelError> {
    let host = host.trim().to_ascii_lowercase();
    let host = host.strip_prefix("https://").unwrap_or(&host);
    if host.contains('/') || host.contains(':') || host.contains('@') || host.is_empty() {
        return Err(PanelError::InvalidHost);
    }
    let (wildcard, bare) = host
        .strip_prefix("*.")
        .map_or((false, host), |suffix| (true, suffix));
    if bare.is_empty()
        || !bare
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        || bare.starts_with('.')
        || bare.ends_with('.')
        || bare.contains("..")
    {
        return Err(PanelError::InvalidHost);
    }
    Ok(if wildcard {
        format!("*.{bare}")
    } else {
        bare.to_owned()
    })
}

fn percent_decode(path: &str) -> Result<String, PanelError> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = bytes.get(index + 1).and_then(|v| hex(*v));
            let lo = bytes.get(index + 2).and_then(|v| hex(*v));
            let (Some(hi), Some(lo)) = (hi, lo) else {
                return Err(PanelError::InvalidPath);
            };
            decoded.push((hi << 4) | lo);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| PanelError::InvalidPath)?;
    if decoded.contains(['\\', '\0']) {
        return Err(PanelError::InvalidPath);
    }
    Ok(decoded)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lumen-panel-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
                .replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_")
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("index.html"), "<h1>Notes</h1>").unwrap();
        root
    }

    #[test]
    fn resolves_plugin_assets_and_sets_security_headers() {
        let root = test_root();
        let package = PanelPackage::new(
            "dev.example.notes",
            &root,
            ["api.example.com".to_owned(), "*.cdn.example".to_owned()],
        )
        .unwrap();
        let response = package
            .response("lumen-plugin://dev.example.notes/", "GET")
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.mime_type, "text/html; charset=utf-8");
        assert!(response.headers.iter().any(|(name, value)| {
            name == "Content-Security-Policy" && value.contains("https://api.example.com")
        }));
        assert_eq!(response.body, b"<h1>Notes</h1>");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blocks_traversal_foreign_plugin_and_unsupported_methods() {
        let root = test_root();
        let package = PanelPackage::new("notes", &root, Vec::<String>::new()).unwrap();
        assert_eq!(
            package.response("lumen-plugin://notes/%2e%2e/secret", "GET"),
            Err(PanelError::InvalidPath)
        );
        assert_eq!(
            package.response("lumen-plugin://other/", "GET"),
            Err(PanelError::UnknownPlugin)
        );
        assert_eq!(
            package.response("lumen-plugin://notes/", "POST"),
            Err(PanelError::MethodNotAllowed)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn allowlist_matches_exact_and_subdomains_without_suffix_confusion() {
        let root = test_root();
        let package = PanelPackage::new("notes", &root, ["*.example.com".to_owned()]).unwrap();
        assert!(package.allows_host("cdn.example.com"));
        assert!(!package.allows_host("example.com"));
        assert!(!package.allows_host("notexample.com"));
        fs::remove_dir_all(root).unwrap();
    }
}
