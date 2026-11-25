mod channel;
mod dispatch;
mod telemetry;

use super::protocol::{AgentRequest, AgentResponse};
use super::refs::RefRegistry;
use super::server::{start_server, stop_server};
use crate::plugin::{
    Manifest, Plugin, PluginCtx, PluginDescriptor, PluginError, PluginHost, PluginKind,
};
use std::collections::HashMap;

pub static AGENT_PLUGIN_ID: &str = "anarc.agent";

pub(crate) use channel::{
    forward_request_to_engine, note_window_painted, register_engine_agent_channel,
};
pub(crate) use dispatch::{compact_tabs, handle_agent_request, no_such_tab};

#[derive(Default)]
pub struct AgentState {
    pub registries: HashMap<String, RefRegistry>,
    pub attached_space: Option<String>,
    pub is_paused: bool,
    pub confirmations: super::policy::Confirmations,
}

pub struct AgentPlugin {
    manifest: Manifest,
}

impl Default for AgentPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPlugin {
    pub fn new() -> Self {
        Self {
            manifest: Manifest {
                id: AGENT_PLUGIN_ID.to_string(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: vec!["agent".to_string()],
            },
        }
    }
}

impl Plugin for AgentPlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        let (req_tx, req_rx) =
            async_channel::bounded::<(AgentRequest, async_channel::Sender<AgentResponse>)>(32);

        let socket_res = start_server(req_tx);
        let _socket_path = match socket_res {
            Ok(p) => p,
            Err(e) => return Err(PluginError(e)),
        };

        let dispose_rx = req_rx.clone();
        let forwarder = std::thread::Builder::new()
            .name("anarc-agent-forwarder".into())
            .spawn(move || {
                while let Ok((request, resp_tx)) =
                    futures_lite::future::block_on(async { req_rx.recv().await })
                {
                    forward_request_to_engine(request, resp_tx);
                }
            });
        if forwarder.is_err() {
            dispose_rx.close();
            stop_server();
            return Err(PluginError(
                "Agent forwarder unavailable; use: status".into(),
            ));
        }
        telemetry::note_harness_best_effort();
        telemetry::note_plugins_installed();
        // Best-effort daily rollup. Consent is unset by default, so this is a
        // no-op until the user opts in; it runs off the plugin thread.
        std::thread::Builder::new()
            .name("anarc-telemetry-flush".into())
            .spawn(|| super::telemetry::flush_daily(env!("CARGO_PKG_VERSION")))
            .ok();
        ctx.effect(move || {
            dispose_rx.close();
            stop_server();
        });

        Ok(())
    }
}

pub(crate) fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: AGENT_PLUGIN_ID.to_string(),
        name: "Agent Control".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: PluginKind::Builtin,
        description:
            "Agent control server exposing see, do, read, find, shot tools over local socket"
                .to_string(),
        permissions: Vec::new(),
        enabled_by_default: true,
        can_toggle: true,
    }
}

pub(crate) fn register(host: &mut PluginHost) {
    host.register(descriptor(), || Box::new(AgentPlugin::new()));
}


#[cfg(test)]
mod tests {
    use anarc_telemetry::schema::ActionKind;

    #[test]
    fn dsl_ops_map_to_action_kinds_without_a_browser() {
        use super::super::act::{ActionOp, WaitCondition};
        assert_eq!(
            super::telemetry::action_kind_for_op(&ActionOp::Click {
                ref_id: "b1".into()
            }),
            ActionKind::Click
        );
        assert_eq!(
            super::telemetry::action_kind_for_op(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: String::new(),
            }),
            ActionKind::Type
        );
        assert_eq!(
            super::telemetry::action_kind_for_op(&ActionOp::Go {
                url: "https://example.com".into()
            }),
            ActionKind::Navigate
        );
        assert_eq!(
            super::telemetry::action_kind_for_op(&ActionOp::Wait {
                condition: WaitCondition::Idle
            }),
            ActionKind::Other
        );
    }

    #[test]
    fn missing_tab_includes_selection_hint() {
        assert_eq!(
            super::no_such_tab("1* chatgpt.com, 2 about:blank"),
            "no such tab. tabs: 1* chatgpt.com, 2 about:blank. use: do \"tab 1\""
        );
    }
}
