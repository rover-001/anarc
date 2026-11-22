use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentRequest {
    See {
        tab_id: Option<String>,
        scope: Option<String>,
    },
    Read {
        tab_id: Option<String>,
        target: Option<String>,
        mode: Option<String>,
        max: Option<usize>,
    },
    Find {
        tab_id: Option<String>,
        query: String,
    },
    Tabs,
    Doctor,
    Do {
        tab_id: Option<String>,
        ops: String,
    },
    Shot {
        tab_id: Option<String>,
        target: Option<String>,
        scale: Option<f32>,
    },
    Attach {
        space_id: Option<String>,
    },
    Pause,
    Resume,
    Status,
}

impl AgentRequest {
    #[cfg(target_family = "unix")]
    pub fn op(&self) -> &'static str {
        match self {
            Self::See { .. } => "see",
            Self::Read { .. } => "read",
            Self::Find { .. } => "find",
            Self::Do { .. } => "do",
            Self::Shot { .. } => "shot",
            Self::Tabs => "tabs",
            Self::Doctor => "doctor",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Status => "status",
            Self::Attach { .. } => "attach",
        }
    }

    pub fn target_tab(&self) -> Option<&str> {
        match self {
            Self::See { tab_id, .. }
            | Self::Read { tab_id, .. }
            | Self::Find { tab_id, .. }
            | Self::Do { tab_id, .. }
            | Self::Shot { tab_id, .. } => tab_id.as_deref(),
            _ => None,
        }
    }

    pub fn target_tab_mut(&mut self) -> Option<&mut Option<String>> {
        match self {
            Self::See { tab_id, .. }
            | Self::Read { tab_id, .. }
            | Self::Find { tab_id, .. }
            | Self::Do { tab_id, .. }
            | Self::Shot { tab_id, .. } => Some(tab_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok: bool,
    #[serde(skip)]
    pub pending_tab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentResponse {
    pub fn success(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            pending_tab: None,
            result: Some(msg.into()),
            error: None,
        }
    }

    pub fn err(err: impl Into<String>) -> Self {
        Self {
            ok: false,
            pending_tab: None,
            result: None,
            error: Some(err.into()),
        }
    }
}
