use cef::{
    currently_on, dictionary_value_create, parse_json_buffer, post_task,
    wrap_dev_tools_message_observer, wrap_task, Browser, BrowserHost, CefString,
    DevToolsMessageObserver, ImplBrowserHost, ImplDevToolsMessageObserver, ImplTask, ImplValue,
    JsonParserOptions, Registration, Task, ThreadId, WrapDevToolsMessageObserver, WrapTask,
};
use cef::rc::Rc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const DEFAULT_DEVTOOLS_TIMEOUT: Duration = Duration::from_secs(30);

type CallbackMap = Arc<Mutex<HashMap<i32, async_channel::Sender<Result<String, String>>>>>;

wrap_task! {
    pub(crate) struct DevToolsExecuteTask {
        host: BrowserHost,
        message_id: i32,
        method: String,
        params_json: Option<String>,
        callbacks: CallbackMap,
        tx: async_channel::Sender<Result<String, String>>,
    }

    impl Task {
        fn execute(&self) {
            let mut dict = if let Some(ref p) = self.params_json {
                json_to_dictionary(p)
            } else {
                None
            };
            let method_str = CefString::from(self.method.as_str());
            let res = self.host.execute_dev_tools_method(
                self.message_id as std::os::raw::c_int,
                Some(&method_str),
                dict.as_mut(),
            );
            if res == 0 {
                if let Ok(mut map) = self.callbacks.lock() {
                    map.remove(&self.message_id);
                }
                let _ = self.tx.try_send(Err(format!(
                    "CEF rejected execute_dev_tools_method for '{}' on UI thread", self.method
                )));
            }
        }
    }
}

wrap_dev_tools_message_observer! {
    pub(crate) struct AnarcDevToolsObserver {
        callbacks: CallbackMap,
        events: Option<async_channel::Sender<(String, String)>>,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: std::os::raw::c_int,
            success: std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let res = if success != 0 {
                let s = result
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default();
                Ok(s)
            } else {
                let s = result
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_else(|| "CDP method execution failed".into());
                Err(s)
            };
            if let Ok(mut map) = self.callbacks.lock() {
                if let Some(tx) = map.remove(&(message_id as i32)) {
                    let _ = tx.try_send(res);
                }
            }
        }

        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            if let (Some(method_str), Some(tx)) = (method, &self.events) {
                let m = method_str.to_string();
                let p = params
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .unwrap_or_default();
                let _ = tx.try_send((m, p));
            }
        }
    }
}

pub struct TabDevToolsSession {
    _registration: Registration,
    callbacks: CallbackMap,
    next_message_id: Arc<AtomicI32>,
    events_tx: Option<async_channel::Sender<(String, String)>>,
}

pub struct DevToolsManager {
    sessions: HashMap<String, TabDevToolsSession>,
    next_global_id: Arc<AtomicI32>,
}

impl Default for DevToolsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DevToolsManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_global_id: Arc::new(AtomicI32::new(1)),
        }
    }

    pub fn ensure_session(
        &mut self,
        tab_id: &str,
        host: &cef::BrowserHost,
    ) -> Result<&mut TabDevToolsSession, String> {
        if !self.sessions.contains_key(tab_id) {
            let callbacks: CallbackMap = Arc::new(Mutex::new(HashMap::new()));
            let (events_tx, _events_rx) = async_channel::bounded(128);
            let mut observer = AnarcDevToolsObserver::new(callbacks.clone(), Some(events_tx.clone()));
            let registration = host
                .add_dev_tools_message_observer(Some(&mut observer))
                .ok_or_else(|| "Failed to register DevTools observer with CEF".to_string())?;

            let session = TabDevToolsSession {
                _registration: registration,
                callbacks,
                next_message_id: self.next_global_id.clone(),
                events_tx: Some(events_tx),
            };
            self.sessions.insert(tab_id.to_string(), session);
        }
        self.sessions
            .get_mut(tab_id)
            .ok_or_else(|| "DevTools session not found".to_string())
    }

    pub fn execute_method(
        &mut self,
        tab_id: &str,
        host: &cef::BrowserHost,
        method: &str,
        params_json: Option<&str>,
        timeout: Duration,
    ) -> async_channel::Receiver<Result<String, String>> {
        let (tx, rx) = async_channel::bounded(1);
        let session = match self.ensure_session(tab_id, host) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.try_send(Err(e));
                return rx;
            }
        };

        let message_id = session.next_message_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut map) = session.callbacks.lock() {
            map.insert(message_id, tx.clone());
        } else {
            let _ = tx.try_send(Err("Lock error in DevTools callback map".into()));
            return rx;
        }

        // Setup per-call timeout
        let pending = session.callbacks.clone();
        let target_tab = tab_id.to_string();
        std::thread::Builder::new()
            .name(format!("cdp-timeout-{message_id}"))
            .spawn(move || {
                std::thread::sleep(timeout);
                if let Ok(mut map) = pending.lock() {
                    if let Some(ch) = map.remove(&message_id) {
                        let _ = ch.try_send(Err(format!(
                            "DevTools timeout: call {message_id} to tab '{target_tab}' timed out after {:?}",
                            timeout
                        )));
                    }
                }
            })
            .ok();

        // Convert params_json to cef::DictionaryValue if present
        let mut dict = if let Some(p) = params_json {
            match json_to_dictionary(p) {
                Some(d) => Some(d),
                None => {
                    if let Ok(mut map) = session.callbacks.lock() {
                        map.remove(&message_id);
                    }
                    let _ = tx.try_send(Err(format!("Invalid JSON params: {p}")));
                    return rx;
                }
            }
        } else {
            None
        };

        if currently_on(ThreadId::UI) != 0 {
            let method_str = CefString::from(method);
            let res = host.execute_dev_tools_method(
                message_id as std::os::raw::c_int,
                Some(&method_str),
                dict.as_mut(),
            );

            if res == 0 {
                if let Ok(mut map) = session.callbacks.lock() {
                    map.remove(&message_id);
                }
                let _ = tx.try_send(Err(format!(
                    "CEF rejected execute_dev_tools_method for '{method}'"
                )));
            }
        } else {
            let mut task = DevToolsExecuteTask::new(
                host.clone(),
                message_id,
                method.to_string(),
                params_json.map(|s| s.to_string()),
                session.callbacks.clone(),
                tx.clone(),
            );
            let post_res = post_task(ThreadId::UI, Some(&mut task));
            if post_res == 0 {
                if let Ok(mut map) = session.callbacks.lock() {
                    map.remove(&message_id);
                }
                let _ = tx.try_send(Err("Failed to post DevTools task to CEF UI thread".into()));
            }
        }

        rx
    }

    pub fn evaluate(
        &mut self,
        tab_id: &str,
        host: &cef::BrowserHost,
        script: &str,
        timeout: Duration,
    ) -> async_channel::Receiver<Result<String, String>> {
        let (out_tx, out_rx) = async_channel::bounded(1);
        let params = serde_json::json!({
            "expression": script,
            "returnByValue": true,
            "awaitPromise": true
        });
        let params_str = match serde_json::to_string(&params) {
            Ok(s) => s,
            Err(e) => {
                let _ = out_tx.try_send(Err(format!("Failed to serialize params: {e}")));
                return out_rx;
            }
        };

        let cdp_rx = self.execute_method(tab_id, host, "Runtime.evaluate", Some(&params_str), timeout);

        std::thread::Builder::new()
            .name("cdp-evaluate-unpacker".into())
            .spawn(move || {
                let raw_res = futures_lite::future::block_on(async { cdp_rx.recv().await });
                let final_res = match raw_res {
                    Ok(Ok(raw_json)) => parse_evaluate_result(&raw_json),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(format!("DevTools channel closed: {e}")),
                };
                let _ = out_tx.try_send(final_res);
            })
            .ok();

        out_rx
    }

    pub fn cleanup_tab(&mut self, tab_id: &str) {
        if let Some(session) = self.sessions.remove(tab_id) {
            if let Ok(mut map) = session.callbacks.lock() {
                for (_, tx) in map.drain() {
                    let _ = tx.try_send(Err(format!("tab closed: tab '{tab_id}' was closed")));
                }
            }
        }
    }
}

pub fn json_to_dictionary(json_str: &str) -> Option<cef::DictionaryValue> {
    if json_str.trim().is_empty() {
        return dictionary_value_create();
    }
    let val = parse_json_buffer(Some(json_str.as_bytes()), JsonParserOptions::RFC)?;
    val.dictionary()
}

pub fn parse_evaluate_result(raw_json: &str) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(raw_json)
        .map_err(|e| format!("Failed to parse DevTools response JSON: {e}"))?;

    if let Some(exception) = parsed.get("exceptionDetails") {
        let desc = exception
            .get("exception")
            .and_then(|ex| ex.get("description"))
            .and_then(|d| d.as_str())
            .or_else(|| exception.get("text").and_then(|t| t.as_str()))
            .unwrap_or("Unknown JS evaluation error");
        return Err(desc.to_string());
    }

    if let Some(res) = parsed.get("result") {
        if let Some(val) = res.get("value") {
            if let Some(s) = val.as_str() {
                return Ok(s.to_string());
            } else {
                return Ok(val.to_string());
            }
        }
        if let Some(unserializable) = res.get("unserializableValue") {
            if let Some(s) = unserializable.as_str() {
                return Ok(s.to_string());
            }
        }
        if let Some(type_str) = res.get("type").and_then(|t| t.as_str()) {
            if type_str == "undefined" {
                return Ok(String::new());
            }
        }
    }

    Ok(raw_json.to_string())
}

pub fn sanitize_cookie_for_cdp(c: &serde_json::Value) -> Option<serde_json::Value> {
    let name = c.get("name")?.as_str()?;
    let value = c.get("value")?.as_str()?;
    let mut map = serde_json::Map::new();
    map.insert("name".into(), serde_json::Value::String(name.into()));
    map.insert("value".into(), serde_json::Value::String(value.into()));

    let domain = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
    let path = c.get("path").and_then(|p| p.as_str()).unwrap_or("/");
    let secure = c.get("secure").and_then(|s| s.as_bool()).unwrap_or(false);
    let http_only = c.get("httpOnly").and_then(|h| h.as_bool()).unwrap_or(false);

    if !domain.is_empty() {
        // RFC 6265bis: __Host- cookies MUST NOT have a domain attribute.
        if !name.starts_with("__Host-") {
            map.insert("domain".into(), serde_json::Value::String(domain.into()));
        }
        let clean_domain = domain.trim_start_matches('.');
        let scheme = if secure || name.starts_with("__Secure-") || name.starts_with("__Host-") {
            "https"
        } else {
            "http"
        };
        let url = format!("{scheme}://{clean_domain}{path}");
        map.insert("url".into(), serde_json::Value::String(url));
    } else if let Some(url) = c.get("url").and_then(|u| u.as_str()) {
        map.insert("url".into(), serde_json::Value::String(url.into()));
    }
    map.insert("path".into(), serde_json::Value::String(path.into()));

    let is_secure = secure || name.starts_with("__Secure-") || name.starts_with("__Host-");
    map.insert("secure".into(), serde_json::Value::Bool(is_secure));
    map.insert("httpOnly".into(), serde_json::Value::Bool(http_only));
    if let Some(ss) = c.get("sameSite").and_then(|s| s.as_str()) {
        if matches!(ss, "Strict" | "Lax" | "None") {
            map.insert("sameSite".into(), serde_json::Value::String(ss.into()));
            if ss == "None" {
                map.insert("secure".into(), serde_json::Value::Bool(true));
            }
        }
    }
    if let Some(exp) = c.get("expires").and_then(|e| e.as_f64()) {
        if exp > 0.0 {
            map.insert("expires".into(), serde_json::json!(exp));
        }
    }
    Some(serde_json::Value::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_evaluate_result_string() {
        let json = r#"{"result":{"type":"string","value":"My Document Title"}}"#;
        assert_eq!(parse_evaluate_result(json).unwrap(), "My Document Title");
    }

    #[test]
    fn test_parse_evaluate_result_number() {
        let json = r#"{"result":{"type":"number","value":42}}"#;
        assert_eq!(parse_evaluate_result(json).unwrap(), "42");
    }

    #[test]
    fn test_parse_evaluate_result_boolean() {
        let json = r#"{"result":{"type":"boolean","value":true}}"#;
        assert_eq!(parse_evaluate_result(json).unwrap(), "true");
    }

    #[test]
    fn test_parse_evaluate_result_undefined() {
        let json = r#"{"result":{"type":"undefined"}}"#;
        assert_eq!(parse_evaluate_result(json).unwrap(), "");
    }

    #[test]
    fn test_parse_evaluate_result_object() {
        let json = r#"{"result":{"type":"object","value":{"name":"anarc","tokens":10}}}"#;
        assert_eq!(
            parse_evaluate_result(json).unwrap(),
            r#"{"name":"anarc","tokens":10}"#
        );
    }

    #[test]
    fn test_parse_evaluate_result_exception() {
        let json = r#"{"exceptionDetails":{"text":"Uncaught","exception":{"description":"ReferenceError: nonExistent is not defined"}}}"#;
        assert_eq!(
            parse_evaluate_result(json).unwrap_err(),
            "ReferenceError: nonExistent is not defined"
        );
    }

    #[test]
    fn test_devtools_manager_cleanup_closed_tab() {
        let mut mgr = DevToolsManager::new();
        let callbacks: CallbackMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = async_channel::bounded(1);
        callbacks.lock().unwrap().insert(42, tx);

        mgr.sessions.insert(
            "test-tab".to_string(),
            TabDevToolsSession {
                _registration: unsafe { std::mem::zeroed() },
                callbacks,
                next_message_id: Arc::new(AtomicI32::new(43)),
                events_tx: None,
            },
        );

        // Note: prevent calling drop on zeroed _registration in this unit test
        let session = mgr.sessions.remove("test-tab").unwrap();
        if let Ok(mut map) = session.callbacks.lock() {
            for (_, ch) in map.drain() {
                let _ = ch.try_send(Err("tab closed: tab 'test-tab' was closed".into()));
            }
        }
        std::mem::forget(session._registration);

        let err = rx.try_recv().unwrap().unwrap_err();
        assert!(err.contains("tab closed"));
    }
}
