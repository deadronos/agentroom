use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;

pub struct CopilotAdapter;

impl CopilotAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn session_state_dir() -> PathBuf {
        Self::home_dir().join(".copilot").join("session-state")
    }

    fn read_lines(path: &PathBuf, n: usize) -> Vec<String> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content.lines().rev().take(n).map(String::from).collect::<Vec<_>>().into_iter().rev().collect()
    }

    fn parse_jsonl_entry(line: &str) -> Option<serde_json::Value> {
        serde_json::from_str(line).ok()
    }

    fn get_type(value: &serde_json::Value) -> Option<String> {
        value.get("type").and_then(|v| v.as_str()).map(String::from)
    }

    fn get_data(value: &serde_json::Value) -> Option<&serde_json::Value> {
        value.get("data")
    }

    fn get_session_start_info(data: &serde_json::Value) -> (Option<String>, Option<String>) {
        let selected_model = data.get("selectedModel").and_then(|v| v.as_str()).map(String::from);
        let context = data.get("context");
        let cwd = context.and_then(|c| c.get("cwd")).and_then(|v| v.as_str()).map(String::from);
        (selected_model, cwd)
    }

    fn get_message_content(data: &serde_json::Value) -> Option<String> {
        let content = data.get("content")?;
        if let Some(arr) = content.as_array() {
            let text_parts: Vec<String> = arr
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                        item.get("text").and_then(|t| t.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect();
            if !text_parts.is_empty() {
                return Some(text_parts.join(""));
            }
        }
        if let Some(s) = content.as_str() {
            return Some(s.to_string());
        }
        None
    }

    fn get_tool_call_info(data: &serde_json::Value) -> (Option<String>, Option<String>) {
        let name = data.get("name").and_then(|v| v.as_str()).map(String::from);
        let input = data.get("input").map(|v| serde_json::to_string(v).unwrap_or_default());
        (name, input)
    }

    fn is_active(path: &PathBuf, threshold_ms: i64) -> bool {
        if let Ok(stat) = std::fs::metadata(path) {
            let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
            let mtime_ms = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            mtime_ms >= threshold_ms
        } else {
            false
        }
    }
}

impl SessionAdapter for CopilotAdapter {
    fn name(&self) -> &str {
        "copilot"
    }

    fn is_available(&self) -> bool {
        Self::session_state_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let session_state_dir = Self::session_state_dir();
        if session_state_dir.exists() {
            vec![WatchPath {
                path: session_state_dir.clone(),
                watch_type: WatchType::Directory,
                filter: Some("events.jsonl".to_string()),
                recursive: true,
            }]
        } else {
            Vec::new()
        }
    }

    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - threshold_ms as i64;
        let mut sessions = Vec::new();

        let session_state_dir = Self::session_state_dir();
        if !session_state_dir.exists() {
            return sessions;
        }

        let entries = match std::fs::read_dir(&session_state_dir) {
            Ok(e) => e,
            Err(_) => return sessions,
        };

        for entry in entries.flatten() {
            let uuid_path = entry.path();
            if !uuid_path.is_dir() {
                continue;
            }

            let uuid = uuid_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if uuid.is_empty() {
                continue;
            }

            let events_path = uuid_path.join("events.jsonl");
            if !events_path.exists() {
                continue;
            }

            if !Self::is_active(&events_path, threshold) {
                continue;
            }

            let lines = Self::read_lines(&events_path, 100);

            let mut model = "unknown".to_string();
            let mut project = None;
            let mut last_message = None;
            let mut last_tool = None;
            let mut last_tool_input = None;
            let mut last_activity = threshold;

            for line in &lines {
                if let Some(value) = Self::parse_jsonl_entry(line) {
                    let entry_type = Self::get_type(&value);
                    let data = Self::get_data(&value);

                    if entry_type == Some("session.start".to_string()) {
                        if let Some(d) = data {
                            let (m, c) = Self::get_session_start_info(d);
                            if let Some(mv) = m {
                                model = mv;
                            }
                            project = c;
                        }
                    } else if entry_type == Some("user.message".to_string())
                        || entry_type == Some("assistant.message".to_string())
                    {
                        if let Some(d) = data {
                            last_message = Self::get_message_content(d);
                        }
                    } else if entry_type == Some("tool_call".to_string()) {
                        if let Some(d) = data {
                            let (name, input) = Self::get_tool_call_info(d);
                            last_tool = name;
                            last_tool_input = input;
                        }
                    }
                }
            }

            let session_id = format!("copilot:{}", uuid);

            // Estimate last_activity from file mtime
            if let Ok(stat) = std::fs::metadata(&events_path) {
                if let Ok(mtime) = stat.modified() {
                    last_activity = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                }
            }

            sessions.push(ActiveSession {
                session_id,
                provider: "copilot".to_string(),
                agent_id: None,
                agent_type: "main".to_string(),
                model,
                status: "active".to_string(),
                last_activity,
                project,
                last_message,
                last_tool,
                last_tool_input,
                parent_session_id: None,
            });
        }

        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let uuid = session_id.strip_prefix("copilot:")?;
        let session_state_dir = Self::session_state_dir();
        let events_path = session_state_dir.join(uuid).join("events.jsonl");

        if !events_path.exists() {
            return None;
        }

        let lines = Self::read_lines(&events_path, 100);

        let mut model = "unknown".to_string();
        let mut project = None;
        let mut last_message = None;
        let mut last_tool = None;
        let mut last_tool_input = None;

        for line in &lines {
            if let Some(value) = Self::parse_jsonl_entry(line) {
                let entry_type = Self::get_type(&value);
                let data = Self::get_data(&value);

                if entry_type == Some("session.start".to_string()) {
                    if let Some(d) = data {
                        let (m, c) = Self::get_session_start_info(d);
                        if let Some(mv) = m {
                            model = mv;
                        }
                        project = c;
                    }
                } else if entry_type == Some("user.message".to_string())
                    || entry_type == Some("assistant.message".to_string())
                {
                    if let Some(d) = data {
                        last_message = Self::get_message_content(d);
                    }
                } else if entry_type == Some("tool_call".to_string()) {
                    if let Some(d) = data {
                        let (name, input) = Self::get_tool_call_info(d);
                        last_tool = name;
                        last_tool_input = input;
                    }
                }
            }
        }

        let last_activity = std::fs::metadata(&events_path)
            .and_then(|s| s.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64
            })
            .unwrap_or(0);

        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "copilot".to_string(),
            agent_id: None,
            agent_type: "main".to_string(),
            model,
            status: "active".to_string(),
            last_activity,
            project,
            last_message,
            last_tool,
            last_tool_input,
            parent_session_id: None,
        })
    }
}