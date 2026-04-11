use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[doc = "Represents an active agent session being tracked by the collector."]
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
#[doc = "A snapshot of all active sessions at a point in time, sent from collector to hub."]
pub struct Snapshot {
    pub collector_id: String,
    pub timestamp: i64,
    pub fingerprint: String,
    pub sessions: Vec<ActiveSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[doc = "Events emitted by the collector about session lifecycle."]
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
#[doc = "Messages sent from collector to hub."]
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
#[doc = "Messages sent from hub to collector (commands) or collector to hub (events)."]
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
