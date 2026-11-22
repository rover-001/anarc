use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PlaybookMacro {
    pub name: String,
    pub args: Vec<String>,
    pub ops: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentSiteConfig {
    pub r#match: Vec<String>,
    #[serde(default)]
    pub hide: Vec<String>,
    #[serde(default)]
    pub label: HashMap<String, String>,
    #[serde(default)]
    pub r#macro: Vec<PlaybookMacro>,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybookRegistry {
    pub sites: Vec<AgentSiteConfig>,
    pub macros: HashMap<String, PlaybookMacro>,
}

static GLOBAL_REGISTRY: Mutex<Option<PlaybookRegistry>> = Mutex::new(None);
static RECORDING_SESSION: Mutex<Option<(String, Vec<String>)>> = Mutex::new(None);

impl PlaybookRegistry {
    pub fn new() -> Self {
        Self {
            sites: Vec::new(),
            macros: HashMap::new(),
        }
    }

    pub fn load_from_packs_or_file() -> Self {
        let mut reg = Self::new();
        // Load default known macros (e.g. search)
        reg.macros.insert(
            "search".to_string(),
            PlaybookMacro {
                name: "search".to_string(),
                args: vec!["q".to_string()],
                ops: "i:\"Search\"=$q; key Enter".to_string(),
            },
        );

        let playbook_path = if let Ok(data_dir) = std::env::var("ANARC_DATA_DIR") {
            PathBuf::from(data_dir).join("agent").join("playbooks.toml")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local").join("share").join("anarc").join("agent").join("playbooks.toml")
        } else {
            return reg;
        };

        if let Ok(content) = std::fs::read_to_string(&playbook_path) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                if let Some(sites_arr) = parsed.get("agent_site").and_then(|v| v.as_array()) {
                    for site_val in sites_arr {
                        if let Ok(site_cfg) = site_val.clone().try_into::<AgentSiteConfig>() {
                            for m in &site_cfg.r#macro {
                                reg.macros.insert(m.name.clone(), m.clone());
                            }
                            reg.sites.push(site_cfg);
                        }
                    }
                }
            }
        }

        reg
    }

    // Kept as the lookup hook for site-scoped playbooks loaded from playbooks.toml.
    #[allow(dead_code)]
    pub fn find_site_config(&self, url: &str) -> Option<&AgentSiteConfig> {
        for site in &self.sites {
            for pattern in &site.r#match {
                let pat_clean = pattern.trim_end_matches('*');
                if url.starts_with(pat_clean) {
                    return Some(site);
                }
            }
        }
        None
    }

    pub fn expand_macro(&self, name: &str, args: &[String]) -> Result<String, String> {
        let m = match self.macros.get(name) {
            Some(m) => m,
            None => return Err("macro missing. use: see".into()),
        };

        let mut expanded = m.ops.clone();
        for (i, param_name) in m.args.iter().enumerate() {
            let val = match args.get(i) {
                Some(v) => v.as_str(),
                None => "",
            };
            let needle = format!("${param_name}");
            expanded = expanded.replace(&needle, val);
        }

        Ok(expanded)
    }
}

pub fn get_or_init_registry() -> PlaybookRegistry {
    if let Ok(guard) = GLOBAL_REGISTRY.lock() {
        if let Some(ref reg) = *guard {
            return reg.clone();
        }
    }

    let loaded = PlaybookRegistry::load_from_packs_or_file();
    if let Ok(mut guard) = GLOBAL_REGISTRY.lock() {
        *guard = Some(loaded.clone());
    }
    loaded
}

// Recording entry points for the playbook recorder; `record_action` is already
// wired into the DSL executor, these two bound a recording session.
#[allow(dead_code)]
pub fn start_recording(name: &str) {
    if let Ok(mut guard) = RECORDING_SESSION.lock() {
        *guard = Some((name.to_string(), Vec::new()));
    }
}

pub fn record_action(op: &str) {
    if let Ok(mut guard) = RECORDING_SESSION.lock() {
        if let Some((_, ref mut ops)) = *guard {
            ops.push(op.to_string());
        }
    }
}

#[allow(dead_code)]
pub fn stop_recording() -> Option<PlaybookMacro> {
    let session = if let Ok(mut guard) = RECORDING_SESSION.lock() {
        guard.take()
    } else {
        None
    }?;

    let (name, ops_vec) = session;
    if ops_vec.is_empty() {
        return None;
    }

    let combined_ops = ops_vec.join("; ");
    let macro_obj = PlaybookMacro {
        name: name.clone(),
        args: Vec::new(),
        ops: combined_ops,
    };

    // Save to global registry
    if let Ok(mut guard) = GLOBAL_REGISTRY.lock() {
        if let Some(ref mut reg) = *guard {
            reg.macros.insert(name, macro_obj.clone());
        }
    }

    // Persist to playbooks.toml
    let playbook_path = if let Ok(data_dir) = std::env::var("ANARC_DATA_DIR") {
        PathBuf::from(data_dir).join("agent").join("playbooks.toml")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("share").join("anarc").join("agent").join("playbooks.toml")
    } else {
        PathBuf::from("/tmp/anarc-playbooks.toml")
    };

    if let Some(parent) = playbook_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&playbook_path) {
        let snippet = format!(
            "\n[[agent_site.macro]]\nname = \"{}\"\nargs = []\nops = \"{}\"\n",
            macro_obj.name, macro_obj.ops
        );
        let _ = f.write_all(snippet.as_bytes());
    }

    Some(macro_obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macro_expansion() {
        let mut reg = PlaybookRegistry::new();
        reg.macros.insert(
            "login".to_string(),
            PlaybookMacro {
                name: "login".to_string(),
                args: vec!["user".to_string(), "pass".to_string()],
                ops: "i:\"Username\"=$user; i:\"Password\"=$pass; b:\"Sign in\"".to_string(),
            },
        );

        let expanded = reg
            .expand_macro("login", &[String::from("rover@x.com"), String::from("secret123")])
            .unwrap_or_default();

        assert_eq!(
            expanded,
            "i:\"Username\"=rover@x.com; i:\"Password\"=secret123; b:\"Sign in\""
        );
    }

    #[test]
    fn test_site_pattern_matching() {
        let mut reg = PlaybookRegistry::new();
        reg.sites.push(AgentSiteConfig {
            r#match: vec!["https://github.com/*".to_string()],
            hide: vec![".cookie-notice".to_string(), "footer".to_string()],
            label: HashMap::new(),
            r#macro: Vec::new(),
        });

        let site = reg.find_site_config("https://github.com/torvalds/linux");
        assert!(site.is_some());
        assert_eq!(site.map(|site| site.hide.len()), Some(2));

        let non_site = reg.find_site_config("https://google.com");
        assert!(non_site.is_none());
    }
}
