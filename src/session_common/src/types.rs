use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub session_id: String,
    pub provider: String,
    pub agent_id: Option<String>,
    pub agent_type: String,
    pub model: String,
    pub status: String,
    pub last_activity: i64,
    pub project: Option<String>,
    pub last_message: Option<String>,
    pub last_tool: Option<String>,
    pub last_tool_input: Option<String>,
    pub parent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub collector_id: String,
    pub timestamp: i64,
    pub fingerprint: String,
    pub sessions: Vec<ActiveSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionStarted {
        session_id: String,
        provider: String,
        project: Option<String>,
        model: String,
        timestamp: i64,
        last_tool: Option<String>,
        last_message: Option<String>,
        agent_id: Option<String>,
        agent_type: String,
    },
    Activity {
        session_id: String,
        provider: String,
        timestamp: i64,
        tool: Option<String>,
        message_preview: Option<String>,
    },
    SessionEnded {
        session_id: String,
        provider: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum CollectorMessage {
    Snapshot {
        collector_id: String,
        timestamp: i64,
        fingerprint: String,
        sessions: Vec<ActiveSession>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum HubMessage {
    Ack {
        fingerprint: String,
    },
    Error {
        message: String,
    },
    SessionStarted {
        session_id: String,
        provider: String,
        project: Option<String>,
        model: String,
        timestamp: i64,
        last_tool: Option<String>,
        last_message: Option<String>,
        agent_id: Option<String>,
        agent_type: String,
    },
    Activity {
        session_id: String,
        provider: String,
        timestamp: i64,
        tool: Option<String>,
        message_preview: Option<String>,
    },
    SessionEnded {
        session_id: String,
        provider: String,
        timestamp: i64,
    },
    StateSync {
        sessions: Vec<ActiveSession>,
    },
}
