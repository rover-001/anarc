use super::*;
use crate::plugin::{PanelRegistryHandle, PanelRequest, PanelResponse};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::ptr;

pub(super) type PanelRegistry = PanelRegistryHandle;

pub(super) const LIVE_SHARE_PICKER_URL: &str =
    "anarc-plugin://com.anarc.live-share/__anarc_choose_space";
const LIVE_SHARE_PAGE_CHECK: &str = "location.href === 'anarc-plugin://com.anarc.live-share/' || \
    location.href === 'anarc-plugin://com.anarc.live-share/index.html'";

pub(super) fn is_live_share_page(url: &str) -> bool {
    matches!(
        url,
        "anarc-plugin://com.anarc.live-share/" | "anarc-plugin://com.anarc.live-share/index.html"
    )
}

pub(crate) struct LiveShareSpaceRequest {
    pub(super) tab_id: String,
    pub(super) frame: Frame,
}

impl LiveShareSpaceRequest {
    pub(crate) fn tab_id(&self) -> &str {
        &self.tab_id
    }

    pub(crate) fn is_current(&self) -> bool {
        self.frame.is_valid() != 0
            && is_live_share_page(&CefStringUtf16::from(&self.frame.url()).to_string())
    }

    pub(crate) fn send_selection(&self, selection: &serde_json::Value) {
        run_live_share_script(
            &self.frame,
            &format!(
                "window.dispatchEvent(new CustomEvent('anarc:space-selected', {{ detail: {selection} }}));"
            ),
        );
    }
}

pub(super) fn run_live_share_script(frame: &Frame, body: &str) {
    if frame.is_valid() != 0 && is_live_share_page(&CefStringUtf16::from(&frame.url()).to_string()) {
        // Recheck in the renderer in case the frame navigates after this check.
        let script = format!("if ({LIVE_SHARE_PAGE_CHECK}) {{ {body} }}");
        frame.execute_java_script(Some(&CefString::from(script.as_str())), None, 1);
    }
}

pub(super) fn register_panel_scheme(registrar: &mut SchemeRegistrar) {
    let options = (SchemeOptions::STANDARD.get_raw()
        | SchemeOptions::LOCAL.get_raw()
        | SchemeOptions::SECURE.get_raw()
        | SchemeOptions::CORS_ENABLED.get_raw()
        | SchemeOptions::FETCH_ENABLED.get_raw()) as i32;
    let anarc_name = CefString::from("anarc-plugin");
    let _ = registrar.add_custom_scheme(Some(&anarc_name), options);
    let lumen_name = CefString::from("lumen-plugin");
    let _ = registrar.add_custom_scheme(Some(&lumen_name), options);
}

pub(super) fn register_panel_factory(context: &RequestContext, registry: PanelRegistry) {
    let anarc_scheme = CefString::from("anarc-plugin");
    let mut anarc_factory = LumenPanelSchemeHandlerFactory::new(registry.clone());
    let _ = context.register_scheme_handler_factory(Some(&anarc_scheme), None, Some(&mut anarc_factory));

    let lumen_scheme = CefString::from("lumen-plugin");
    let mut lumen_factory = LumenPanelSchemeHandlerFactory::new(registry);
    let registered =
        context.register_scheme_handler_factory(Some(&lumen_scheme), None, Some(&mut lumen_factory));
    if registered == 0 {
        eprintln!("Anarc failed to register plugin CEF scheme handlers");
    }
}

cef::wrap_scheme_handler_factory! {
    pub(super) struct LumenPanelSchemeHandlerFactory {
        registry: PanelRegistry,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request = request?;
            let url = CefStringUtf16::from(&request.url()).to_string();
            let method = CefStringUtf16::from(&request.method()).to_string();
            let parsed = PanelRequest::parse(&url);
            if let Ok(p) = &parsed {
                if p.plugin_id == "com.anarc.live-share" {
                    crate::app::ensure_live_share_daemon_running();
                }
            }
            if url.contains("__anarc_start_service") {
                crate::app::ensure_live_share_daemon_running();
                let resp = PanelResponse {
                    status: 200,
                    mime_type: "application/json".into(),
                    body: b"{\"ok\":true}".to_vec(),
                    headers: vec![
                        ("Access-Control-Allow-Origin".into(), "*".into()),
                        ("Content-Type".into(), "application/json".into()),
                    ],
                };
                return Some(PanelResourceHandler::new(Arc::new(resp), Arc::new(Mutex::new(0))));
            }
            // Reverse-proxy: anarc-plugin://com.anarc.live-share/api-proxy/<path>
            // → http://127.0.0.1:7433/<path>  (same-process, no browser CORS)
            const PROXY_PREFIX: &str = "anarc-plugin://com.anarc.live-share/api-proxy";
            if url.starts_with(PROXY_PREFIX) {
                let daemon_path = &url[PROXY_PREFIX.len()..];
                let daemon_path = if daemon_path.is_empty() { "/" } else { daemon_path };
                let resp = live_share_proxy_request(&method, daemon_path, request);
                return Some(PanelResourceHandler::new(Arc::new(resp), Arc::new(Mutex::new(0))));
            }
            let package = parsed.as_ref().ok().and_then(|parsed| {
                self.registry
                    .lock()
                    .ok()
                    .and_then(|registry| registry.get(&parsed.plugin_id).cloned())
            });
            let response = package
                .as_ref()
                .and_then(|package| package.response(&url, &method).ok())
                .unwrap_or_else(blocked_panel_response);
            Some(PanelResourceHandler::new(Arc::new(response), Arc::new(Mutex::new(0))))
        }
    }
}

cef::wrap_resource_handler! {
    pub(super) struct PanelResourceHandler {
        response: Arc<PanelResponse>,
        offset: Arc<Mutex<usize>>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut i32>,
            _callback: Option<&mut Callback>,
        ) -> i32 {
            if let Some(handle_request) = handle_request {
                *handle_request = 1;
            }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let Some(response) = response else { return; };
            response.set_status(i32::from(self.response.status));
            let status_text = CefString::from(if self.response.status == 200 { "OK" } else { "Not Found" });
            let content_type = self.response.mime_type.as_str();
            let (mime_type, charset) = content_type
                .split_once("; charset=")
                .map_or((content_type, None), |(mime_type, charset)| (mime_type, Some(charset)));
            response.set_status_text(Some(&status_text));
            response.set_mime_type(Some(&CefString::from(mime_type)));
            if let Some(charset) = charset {
                response.set_charset(Some(&CefString::from(charset)));
            }
            for (name, value) in &self.response.headers {
                let name = CefString::from(name.as_str());
                let value = CefString::from(value.as_str());
                response.set_header_by_name(Some(&name), Some(&value), 1);
            }
            if let Some(response_length) = response_length {
                *response_length = self.response.body.len() as i64;
            }
        }

        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: i32,
            bytes_read: Option<&mut i32>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> i32 {
            let Some(bytes_read) = bytes_read else { return 0; };
            if data_out.is_null() || bytes_to_read <= 0 {
                *bytes_read = 0;
                return 0;
            }
            let Ok(mut offset) = self.offset.lock() else { return 0; };
            let remaining = self.response.body.len().saturating_sub(*offset);
            let count = remaining.min(bytes_to_read as usize);
            if count == 0 {
                *bytes_read = 0;
                return 0;
            }
            // CEF owns a writable buffer of at least `bytes_to_read` bytes.
            unsafe {
                ptr::copy_nonoverlapping(
                    self.response.body.as_ptr().add(*offset),
                    data_out,
                    count,
                );
            }
            *offset += count;
            *bytes_read = count as i32;
            1
        }
    }
}

/// Synchronous TCP proxy: forward an HTTP request to the Live Share daemon at
/// 127.0.0.1:7433 and return the response as a PanelResponse.  Because this
/// runs on the CEF IO thread the call must complete quickly; the daemon is
/// local so the round-trip is negligible.
fn live_share_proxy_request(method: &str, path: &str, request: &mut cef::Request) -> PanelResponse {
    let error_resp = |msg: &str| PanelResponse {
        status: 502,
        mime_type: "application/json; charset=utf-8".into(),
        body: format!("{{\"error\":\"{msg}\"}}").into_bytes(),
        headers: vec![
            ("Access-Control-Allow-Origin".into(), "*".into()),
            ("Content-Type".into(), "application/json".into()),
        ],
    };

    // Read POST body bytes from CEF post data
    let body_bytes: Vec<u8> = {
        let mut bytes = Vec::new();
        if let Some(post_data) = request.post_data() {
            let count = post_data.element_count();
            if count > 0 {
                let mut elements: Vec<Option<cef::PostDataElement>> = Vec::new();
                post_data.elements(Some(&mut elements));
                for elem in elements.into_iter().flatten() {
                    if elem.get_type() == cef::PostdataelementType::BYTES {
                        let size = elem.bytes_count();
                        if size > 0 {
                            let mut buf = vec![0u8; size];
                            elem.bytes(size, buf.as_mut_ptr());
                            bytes.extend_from_slice(&buf);
                        }
                    }
                }
            }
        }
        bytes
    };

    let Ok(mut stream) = TcpStream::connect("127.0.0.1:7433") else {
        return error_resp("daemon not reachable");
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    let http_req = if body_bytes.is_empty() {
        format!(
            "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:7433\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    } else {
        format!(
            "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:7433\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body_bytes.len()
        )
    };

    if stream.write_all(http_req.as_bytes()).is_err()
        || (!body_bytes.is_empty() && stream.write_all(&body_bytes).is_err())
    {
        return error_resp("write failed");
    }

    let mut raw = Vec::new();
    if stream.read_to_end(&mut raw).is_err() && raw.is_empty() {
        return error_resp("read failed");
    }

    // Parse HTTP response: split at \r\n\r\n
    if let Some(header_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
        let header_part = std::str::from_utf8(&raw[..header_end]).unwrap_or("");
        let body = raw[header_end + 4..].to_vec();

        let mut lines = header_part.lines();
        let status_line = lines.next().unwrap_or("HTTP/1.0 200 OK");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);

        let mut mime_type = "application/json; charset=utf-8".to_string();
        for line in lines {
            if let Some(val) = line.strip_prefix("Content-Type:").or_else(|| line.strip_prefix("content-type:")) {
                mime_type = val.trim().to_string();
            }
        }

        PanelResponse {
            status,
            mime_type,
            body,
            headers: vec![("Access-Control-Allow-Origin".into(), "*".into())],
        }
    } else {
        // No header separator — treat entire response as body
        PanelResponse {
            status: 200,
            mime_type: "application/json; charset=utf-8".into(),
            body: raw,
            headers: vec![("Access-Control-Allow-Origin".into(), "*".into())],
        }
    }
}

fn blocked_panel_response() -> PanelResponse {
    PanelResponse {
        status: 404,
        mime_type: "text/plain; charset=utf-8".to_owned(),
        body: b"Panel resource unavailable".to_vec(),
        headers: vec![
            (
                "Content-Security-Policy".to_owned(),
                "default-src 'none'; frame-ancestors 'none'".to_owned(),
            ),
            ("X-Content-Type-Options".to_owned(), "nosniff".to_owned()),
            ("Cache-Control".to_owned(), "no-store".to_owned()),
        ],
    }
}

pub(super) fn allowed_panel_network_request(
    source_url: &str,
    target_url: &str,
    registry: &PanelRegistry,
) -> bool {
    let Ok(source) = PanelRequest::parse(source_url) else {
        return true;
    };
    if target_url.starts_with("data:") || target_url.starts_with("blob:") {
        return true;
    }
    if let Ok(target) = PanelRequest::parse(target_url) {
        return target.plugin_id == source.plugin_id;
    }
    let Some(host) = network_target_host(target_url) else {
        return false;
    };
    registry
        .lock()
        .ok()
        .and_then(|registry| registry.get(&source.plugin_id).cloned())
        .is_some_and(|package| package.allows_host(host))
}

fn network_target_host(url: &str) -> Option<&str> {
    if let Some(rest) = url.strip_prefix("https://") {
        let authority = rest.split(['/', '?', '#']).next()?;
        if authority.is_empty() || authority.contains(['@', '%']) {
            return None;
        }
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if port.is_some_and(|port| port != "443") || host.contains(']') || host.contains('[') {
            return None;
        }
        return (!host.is_empty()).then_some(host);
    }
    if let Some(rest) = url.strip_prefix("http://") {
        let authority = rest.split(['/', '?', '#']).next()?;
        if authority.is_empty() || authority.contains(['@', '%']) {
            return None;
        }
        let (host, _port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if host == "127.0.0.1" || host == "localhost" {
            return Some(host);
        }
    }
    if let Some(rest) = url.strip_prefix("ws://") {
        let authority = rest.split(['/', '?', '#']).next()?;
        let (host, _port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if host == "127.0.0.1" || host == "localhost" {
            return Some(host);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PanelPackage;

    #[test]
    fn panel_handler_sets_cef_mime_type_and_charset_separately() {
        let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
        for (content_type, mime_type, charset) in [
            ("text/html; charset=utf-8", "text/html", "utf-8"),
            ("text/css; charset=utf-8", "text/css", "utf-8"),
            ("text/javascript; charset=utf-8", "text/javascript", "utf-8"),
            ("image/png", "image/png", ""),
        ] {
            let handler = PanelResourceHandler::new(
                Arc::new(PanelResponse {
                    status: 200,
                    mime_type: content_type.to_owned(),
                    body: Vec::new(),
                    headers: Vec::new(),
                }),
                Arc::new(Mutex::new(0)),
            );
            let mut response = cef::response_create().unwrap();
            handler.response_headers(Some(&mut response), None, None);
            assert_eq!(
                CefStringUtf16::from(&response.mime_type()).to_string(),
                mime_type
            );
            assert_eq!(
                CefStringUtf16::from(&response.charset()).to_string(),
                charset
            );
        }
    }

    #[test]
    fn panel_network_gate_limits_external_requests_to_declared_https_hosts() {
        let root = std::env::temp_dir().join(format!("lumen-panel-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let package = PanelPackage::new("notes", &root, ["api.example.com".to_owned()]).unwrap();
        let registry = Arc::new(Mutex::new(HashMap::from([("notes".to_owned(), package)])));
        let source = "lumen-plugin://notes/index.html";
        assert!(allowed_panel_network_request(
            source,
            "https://api.example.com/v1/notes",
            &registry
        ));
        assert!(!allowed_panel_network_request(
            source,
            "https://evil.example.com/v1/notes",
            &registry
        ));
        assert!(!allowed_panel_network_request(
            source,
            "http://api.example.com/v1/notes",
            &registry
        ));
        assert!(allowed_panel_network_request(
            source,
            "lumen-plugin://notes/style.css",
            &registry
        ));
        assert!(!allowed_panel_network_request(
            source,
            "lumen-plugin://other/index.html",
            &registry
        ));
        let live_pkg = PanelPackage::new("live", &root, ["127.0.0.1".to_owned()]).unwrap();
        let live_reg = Arc::new(Mutex::new(HashMap::from([("live".to_owned(), live_pkg)])));
        let live_source = "anarc-plugin://live/index.html";
        assert!(allowed_panel_network_request(
            live_source,
            "http://127.0.0.1:7433/api/share",
            &live_reg
        ));
        assert!(allowed_panel_network_request(
            live_source,
            "ws://127.0.0.1:7433/ws",
            &live_reg
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
