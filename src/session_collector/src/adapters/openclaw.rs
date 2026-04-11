use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;

/// Calculate days since epoch (1970-01-01) for a given date
fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    let is_leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let days_in_month = |y: i64, m: i64| match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    };

    // Calculate total days from year 0 to the given date
    let mut total_days = 0i64;
    for y in 1970..year {
        total_days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        total_days += days_in_month(year, m);
    }
    total_days + day - 1 // Subtract 1 since day 1 is day 0
}

pub struct OpenClawAdapter;

impl OpenClawAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn agents_dir() -> PathBuf {
        Self::home_dir().join(".openclaw").join("agents")
    }

    /// Parse an RFC3339 timestamp string to milliseconds since epoch
    fn parse_rfc3339_timestamp(s: &str) -> i64 {
        // RFC3339 format: "2024-01-15T10:30:00.000Z" or "2024-01-15T10:30:00Z"
        // Parse without chrono: extract date/time parts manually
        let s = s.trim_end_matches('Z');
        // Handle both with and without subsecond precision
        let parts: Vec<&str> = s.split(|c| c == 'T' || c == '-' || c == ':' || c == '.').collect();
        if parts.len() >= 6 {
            let year: i64 = parts[0].parse().unwrap_or(0);
            let month: i64 = parts[1].parse().unwrap_or(1);
            let day: i64 = parts[2].parse().unwrap_or(1);
            let hour: i64 = parts[3].parse().unwrap_or(0);
            let minute: i64 = parts[4].parse().unwrap_or(0);
            let second: i64 = parts[5].parse().unwrap_or(0);

            // Calculate days since epoch (1970-01-01)
            let days = days_since_epoch(year, month, day);
            let total_seconds = days * 86400 + hour * 3600 + minute * 60 + second;
            total_seconds * 1000
        } else {
            0
        }
    }

    /// Parse a JSONL entry and extract timestamp, message, tool name, and model.
    /// Returns (timestamp_ms, message, tool_name, model)
    fn parse_jsonl_entry(line: &str) -> Option<(i64, Option<String>, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let entry_type = json.get("type")?.as_str()?;

        let timestamp = match entry_type {
            "session" => {
                json.get("timestamp")
                    .and_then(|v| v.as_str())
                    .map(Self::parse_rfc3339_timestamp)
                    .unwrap_or(0)
            }
            "model_change" | "message" => {
                json.get("timestamp")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            }
            _ => return None,
        };

        let (message, tool, model) = match entry_type {
            "session" => {
                // session entry marks start of session, not useful for last message
                (None, None, None)
            }
            "model_change" => {
                let model_id = json.get("modelId").and_then(|v| v.as_str()).map(String::from);
                (None, None, model_id)
            }
            "message" => {
                let msg = json.get("message")?;
                let content = msg.get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.iter().find_map(|item| {
                        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                            item.get("text").and_then(|t| t.as_str()).map(String::from)
                        } else {
                            None
                        }
                    }));
                let role = msg.get("role").and_then(|v| v.as_str());
                let model = msg.get("model").and_then(|v| v.as_str()).map(String::from);
                let tool = if role == Some("assistant") {
                    None // assistant messages don't have tool calls in this format
                } else {
                    None
                };
                (content, tool, model)
            }
            _ => return None,
        };

        Some((timestamp, message, tool, model))
    }

    /// Read the last JSONL entry to get model and last message
    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64, Option<String>)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool, model) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts, model))
    }

    /// Build session ID from agent_id and session_id (filename without .jsonl)
    fn build_session_id(agent_id: &str, session_id: &str) -> String {
        let encoded_agent = encode_uri_component(agent_id);
        let encoded_session = encode_uri_component(session_id);
        format!("openclaw:{}:{}", encoded_agent, encoded_session)
    }

    /// Parse session ID back to (agent_id, session_id)
    fn parse_session_id_parts(session_id: &str) -> Option<(String, String)> {
        let stripped = session_id.strip_prefix("openclaw:")?;
        let parts: Vec<&str> = stripped.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let agent_id = decode_uri_component(parts[0])?;
        let session_id = decode_uri_component(parts[1])?;
        Some((agent_id, session_id))
    }
}

/// Simple URI component encoding (for characters that are problematic in paths/IDs)
fn encode_uri_component(s: &str) -> String {
    s.chars().map(|c| {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ':' => "%3A".to_string(),
            '/' => "%2F".to_string(),
            ' ' => "%20".to_string(),
            _ => format!("%{:02X}", c as u8),
        }
    }).collect()
}

/// Decode a URI component string back to regular string
fn decode_uri_component(s: &str) -> Option<String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                let byte = u8::from_str_radix(&hex, 16).ok()?;
                result.push(byte as char);
            } else {
                return None;
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}

fn walkdir_recursive(
    dir: &PathBuf,
    paths: &mut Vec<PathBuf>,
    depth: usize,
    max_depth: usize,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walkdir_recursive(&path, paths, depth + 1, max_depth)?;
        } else {
            paths.push(path);
        }
    }
    Ok(())
}

impl SessionAdapter for OpenClawAdapter {
    fn name(&self) -> &str {
        "openclaw"
    }

    fn is_available(&self) -> bool {
        Self::agents_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let mut paths = Vec::new();
        let agents_dir = Self::agents_dir();
        if agents_dir.exists() {
            paths.push(WatchPath {
                path: agents_dir.clone(),
                watch_type: WatchType::Directory,
                filter: Some("*.jsonl".to_string()),
                recursive: true,
            });
        }
        paths
    }

    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - threshold_ms as i64;
        let mut sessions = Vec::new();

        let agents_dir = Self::agents_dir();
        if !agents_dir.exists() {
            return sessions;
        }

        // Iterate through agent directories
        if let Ok(agent_entries) = std::fs::read_dir(&agents_dir) {
            for agent_entry in agent_entries.flatten() {
                let agent_path = agent_entry.path();
                if !agent_path.is_dir() {
                    continue;
                }

                let agent_id = agent_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                let sessions_dir = agent_path.join("sessions");
                if !sessions_dir.exists() {
                    continue;
                }

                let mut session_paths = Vec::new();
                let _ = walkdir_recursive(&sessions_dir, &mut session_paths, 0, 10);

                for path in session_paths {
                    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                        continue;
                    }

                    // Get session ID from filename
                    let session_id = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");

                    let full_session_id = Self::build_session_id(&agent_id, session_id);

                    if let Ok(stat) = std::fs::metadata(&path) {
                        let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                        let mtime_ms = mtime
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as i64;
                        if mtime_ms < threshold {
                            continue;
                        }

                        let (last_message, last_tool, last_activity, model) =
                            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, mtime_ms, None));

                        // Try to get cwd from the first session entry in the file
                        let project = Self::extract_cwd_from_session(&path)
                            .or_else(|| path.parent().map(|p| p.to_string_lossy().to_string()));

                        sessions.push(ActiveSession {
                            session_id: full_session_id,
                            provider: "openclaw".to_string(),
                            agent_id: Some(agent_id.clone()),
                            agent_type: "main".to_string(),
                            model: model.unwrap_or_else(|| "unknown".to_string()),
                            status: "active".to_string(),
                            last_activity,
                            project,
                            last_message,
                            last_tool,
                            last_tool_input: None,
                            parent_session_id: None,
                        });
                    }
                }
            }
        }

        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let (agent_id, session_id) = Self::parse_session_id_parts(session_id)?;

        let path = Self::agents_dir()
            .join(&agent_id)
            .join("sessions")
            .join(format!("{}.jsonl", session_id));

        if !path.exists() {
            return None;
        }

        let (last_message, last_tool, last_activity, model) =
            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0, None));

        let project = Self::extract_cwd_from_session(&path)
            .or_else(|| path.parent().map(|p| p.to_string_lossy().to_string()));

        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "openclaw".to_string(),
            agent_id: Some(agent_id),
            agent_type: "main".to_string(),
            model: model.unwrap_or_else(|| "unknown".to_string()),
            status: "active".to_string(),
            last_activity,
            project,
            last_message,
            last_tool,
            last_tool_input: None,
            parent_session_id: None,
        })
    }
}

impl OpenClawAdapter {
    /// Extract cwd from the first "session" type entry in the file
    fn extract_cwd_from_session(path: &PathBuf) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        for line in content.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("type").and_then(|t| t.as_str()) == Some("session") {
                    return json.get("cwd")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }
            }
        }
        None
    }
}