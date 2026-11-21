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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentResponse {
    pub fn success(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            result: Some(msg.into()),
            error: None,
        }
    }

    pub fn err(err: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(err.into()),
        }
    }
}
