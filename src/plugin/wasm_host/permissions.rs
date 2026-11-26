//! Capability and network-host permission grants for WebAssembly plugins.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    TabsRead,
    TabsWrite,
    SpacesRead,
    SpacesWrite,
    HistoryRead,
    HistoryWrite,
    Storage,
    Notify,
    Commands,
    Keymap,
    UiSlots,
}

/// Explicit grants produced by the install/permission flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionGrants {
    capabilities: BTreeSet<Capability>,
    network_hosts: Vec<String>,
}

impl PermissionGrants {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
            network_hosts: Vec::new(),
        }
    }

    pub fn with_network_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
        self.network_hosts = hosts.into_iter().collect();
        self
    }

    pub fn parse(capabilities: &[String], network_hosts: &[String]) -> Result<Self, String> {
        let mut grants = Self::default().with_network_hosts(network_hosts.iter().cloned());
        for raw in capabilities {
            match raw.as_str() {
                "tabs=read" => {
                    grants.capabilities.insert(Capability::TabsRead);
                }
                "tabs=read-write" => {
                    grants.capabilities.insert(Capability::TabsRead);
                    grants.capabilities.insert(Capability::TabsWrite);
                }
                "spaces=read" => {
                    grants.capabilities.insert(Capability::SpacesRead);
                }
                "spaces=read-write" => {
                    grants.capabilities.insert(Capability::SpacesRead);
                    grants.capabilities.insert(Capability::SpacesWrite);
                }
                "history=read" => {
                    grants.capabilities.insert(Capability::HistoryRead);
                }
                "history=read-write" => {
                    grants.capabilities.insert(Capability::HistoryRead);
                    grants.capabilities.insert(Capability::HistoryWrite);
                }
                "storage=true" => {
                    grants.capabilities.insert(Capability::Storage);
                }
                "notify=true" => {
                    grants.capabilities.insert(Capability::Notify);
                }
                "commands=true" => {
                    grants.capabilities.insert(Capability::Commands);
                }
                "keymap=true" => {
                    grants.capabilities.insert(Capability::Keymap);
                }
                "ui-slots=true" => {
                    grants.capabilities.insert(Capability::UiSlots);
                }
                _ => return Err(format!("unsupported permission declaration: {raw}")),
            }
        }
        Ok(grants)
    }

    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub(super) fn require(&self, capability: Capability) -> Result<(), String> {
        if self.allows(capability) {
            Ok(())
        } else {
            Err(format!("permission denied: {capability:?}"))
        }
    }

    pub(super) fn allows_network_url(&self, raw_url: &str) -> bool {
        let Ok(url) = url::Url::parse(raw_url) else {
            return false;
        };
        if url.scheme() != "https" {
            return false;
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return false;
        };
        self.network_hosts.iter().any(|pattern| {
            let pattern = pattern.trim().to_ascii_lowercase();
            if let Some(suffix) = pattern.strip_prefix("*.") {
                host.strip_suffix(&format!(".{suffix}"))
                    .is_some_and(|prefix| !prefix.is_empty())
            } else {
                host == pattern
            }
        })
    }

    fn grants_network_pattern(&self, requested: &str) -> bool {
        let requested = requested
            .strip_prefix("https://")
            .unwrap_or(requested)
            .trim_end_matches('/');
        let requested = requested.to_ascii_lowercase();
        self.network_hosts.iter().any(|granted| {
            let granted = granted
                .strip_prefix("https://")
                .unwrap_or(granted)
                .trim_end_matches('/')
                .to_ascii_lowercase();
            if let Some(requested_suffix) = requested.strip_prefix("*.") {
                granted == requested
                    || granted.strip_prefix("*.").is_some_and(|granted_suffix| {
                        requested_suffix == granted_suffix
                            || requested_suffix.ends_with(&format!(".{granted_suffix}"))
                    })
            } else if let Some(granted_suffix) = granted.strip_prefix("*.") {
                requested == granted_suffix || requested.ends_with(&format!(".{granted_suffix}"))
            } else {
                requested == granted
            }
        })
    }
}

pub(super) fn validate_requested_permissions(
    requested: &[String],
    approved: &PermissionGrants,
) -> Result<(), String> {
    let mut capabilities = Vec::new();
    let mut network_hosts = Vec::new();
    for permission in requested {
        if let Some(host) = permission.strip_prefix("network=") {
            network_hosts.push(host.to_owned());
        } else {
            capabilities.push(permission.clone());
        }
    }
    let requested = PermissionGrants::parse(&capabilities, &network_hosts)?;
    for capability in requested.capabilities {
        approved.require(capability)?;
    }
    for host in requested.network_hosts {
        if !approved.grants_network_pattern(&host) {
            return Err(format!(
                "network permission denied for host pattern: {host}"
            ));
        }
    }
    Ok(())
}
