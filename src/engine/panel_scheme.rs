use super::*;
use crate::plugin::{PanelRegistryHandle, PanelRequest, PanelResponse};
use std::ptr;

pub(super) type PanelRegistry = PanelRegistryHandle;

pub(super) fn register_panel_scheme(registrar: &mut SchemeRegistrar) {
    let name = CefString::from("lumen-plugin");
    // STANDARD enables normal origin parsing; SECURE gives the scheme HTTPS-like
    // mixed-content semantics. CSP_BYPASSING and CORS are intentionally omitted.
    let options = (SchemeOptions::STANDARD.get_raw() | SchemeOptions::SECURE.get_raw()) as i32;
    let _ = registrar.add_custom_scheme(Some(&name), options);
}

pub(super) fn register_panel_factory(context: &RequestContext, registry: PanelRegistry) {
    let scheme = CefString::from("lumen-plugin");
    let mut factory = LumenPanelSchemeHandlerFactory::new(registry);
    let registered =
        context.register_scheme_handler_factory(Some(&scheme), None, Some(&mut factory));
    if registered == 0 {
        eprintln!("Lumen failed to register lumen-plugin CEF scheme handler");
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
            let mime_type = CefString::from(self.response.mime_type.as_str());
            response.set_status_text(Some(&status_text));
            response.set_mime_type(Some(&mime_type));
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
    let Some(host) = https_host(target_url) else {
        return false;
    };
    registry
        .lock()
        .ok()
        .and_then(|registry| registry.get(&source.plugin_id).cloned())
        .is_some_and(|package| package.allows_host(host))
}

fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
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
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PanelPackage;

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
        std::fs::remove_dir_all(root).unwrap();
    }
}
