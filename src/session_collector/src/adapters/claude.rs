use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn history_file() -> PathBuf {
        Self::home_dir().join(".claude").join("history.jsonl")
    }

    fn projects_dir() -> PathBuf {
        Self::home_dir().join(".claude").join("projects")
    }

    /// Encode a project path to the format used in directory names.
    /// `/` becomes `-` (so `/Users/me/project` becomes `-Users-me-project`)
    fn encode_project_path(project: &str) -> String {
        project.replace('/', "-")
    }

    /// Decode a project directory name back to the original path.
    /// `-` becomes `/` (so `-Users-me-project` becomes `/Users/me/project`)
    fn decode_project_path(encoded: &str) -> String {
        encoded.replace('-', "/")
    }

    fn parse_session_id(path: &PathBuf) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    /// Parse a history.jsonl entry to extract session metadata.
    /// Format: {"sessionId":"abc","project":"/path/to/project","agentId":"...","agentType":"main","model":"claude-opus-4-6","timestamp":1234567890,"display":"What I worked on"}
    fn parse_history_entry(line: &str) -> Option<HistoryEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        Some(HistoryEntry {
            session_id: json.get("sessionId")?.as_str()?.to_string(),
            project: json.get("project")?.as_str()?.to_string(),
            agent_id: json.get("agentId").and_then(|v| v.as_str()).map(String::from),
            agent_type: json
                .get("agentType")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "main".to_string()),
            model: json
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "unknown".to_string()),
            timestamp: json.get("timestamp")?.as_i64()?,
            display: json.get("display").and_then(|v| v.as_str()).map(String::from),
        })
    }

    /// Parse a session file entry to extract message/tool info.
    fn parse_session_entry(line: &str) -> Option<(i64, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let ts = json.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tool = json.get("name").and_then(|v| v.as_str()).map(String::from);
        let text = json.get("text").and_then(|v| v.as_str()).map(String::from);
        Some((ts, tool.clone().or(text), tool))
    }

    /// Read the last entry from a session file to get recent activity.
    fn read_last_session_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_session_entry(last_line)?;
        Some((msg, tool, ts))
    }

    /// Read history.jsonl and return a map of sessionId -> HistoryEntry
    fn read_history_entries(&self) -> HashMap<String, HistoryEntry> {
        let mut entries = HashMap::new();
        let history_path = Self::history_file();
        if !history_path.exists() {
            return entries;
        }
        if let Ok(content) = std::fs::read_to_string(&history_path) {
            for line in content.lines() {
                if let Some(entry) = Self::parse_history_entry(line) {
                    entries.insert(entry.session_id.clone(), entry);
                }
            }
        }
        entries
    }

    /// Find all project directories matching the encoded project name.
    fn find_project_session_dir(&self, encoded_project: &str) -> Option<PathBuf> {
        let projects_dir = Self::projects_dir();
        if !projects_dir.exists() {
            return None;
        }
        // Look for a directory matching the encoded project path
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.file_name().and_then(|n| n.to_str()) == Some(encoded_project) {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Scan subagents directory for a session and return subagent entries.
    fn scan_subagents(&self, session_dir: &PathBuf) -> Vec<SubAgentInfo> {
        let mut subagents = Vec::new();
        let subagents_dir = session_dir.join("subagents");
        if !subagents_dir.exists() {
            return subagents;
        }
        if let Ok(entries) = std::fs::read_dir(subagents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        // Format: agent-{agentId}.jsonl
                        if let Some(agent_id) = filename.strip_prefix("agent-").and_then(|s| s.strip_suffix(".jsonl")) {
                            let (last_message, last_tool, last_activity) =
                                Self::read_last_session_entry(&path).unwrap_or((None, None, 0));
                            subagents.push(SubAgentInfo {
                                agent_id: agent_id.to_string(),
                                last_message,
                                last_tool,
                                last_activity,
                            });
                        }
                    }
                }
            }
        }
        subagents
    }
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    session_id: String,
    project: String,
    agent_id: Option<String>,
    agent_type: String,
    model: String,
    timestamp: i64,
    display: Option<String>,
}

#[derive(Debug, Clone)]
struct SubAgentInfo {
    agent_id: String,
    last_message: Option<String>,
    last_tool: Option<String>,
    last_activity: i64,
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

impl SessionAdapter for ClaudeAdapter {
    fn name(&self) -> &str {
        "claude"
    }

    fn is_available(&self) -> bool {
        Self::history_file().exists() || Self::projects_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let mut paths = Vec::new();
        let history_file = Self::history_file();
        if history_file.exists() {
            paths.push(WatchPath {
                path: history_file,
                watch_type: WatchType::File,
                filter: None,
                recursive: false,
            });
        }
        let projects_dir = Self::projects_dir();
        if projects_dir.exists() {
            paths.push(WatchPath {
                path: projects_dir,
                watch_type: WatchType::Directory,
                filter: Some("*.jsonl".to_string()),
                recursive: true,
            });
        }
        paths
    }

    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession> {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let threshold_secs = now_secs - (threshold_ms / 1000) as i64;
        let mut sessions = Vec::new();

        // Step 1: Parse history.jsonl to build sessions map keyed by sessionId
        let history_entries = self.read_history_entries();

        // Step 2: For each session, check file mtime + history timestamp >= threshold
        for (session_id, entry) in &history_entries {
            // Build encoded project path to find session file
            let encoded_project = Self::encode_project_path(&entry.project);
            let project_session_dir = self.find_project_session_dir(&encoded_project);

            let (session_dir, session_path) = match &project_session_dir {
                Some(dir) => {
                    let session_path = dir.join(format!("{}.jsonl", session_id));
                    if session_path.exists() {
                        (Some(dir.clone()), Some(session_path))
                    } else {
                        (Some(dir.clone()), None)
                    }
                }
                None => (None, None),
            };

            // Check file mtime if session file exists (in milliseconds)
            let mtime_ms = session_path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64
                })
                .unwrap_or(entry.timestamp);

            // Skip if BOTH history timestamp AND file mtime are too old
            // entry.timestamp = when session started, mtime_ms = when session was last active
            if entry.timestamp < threshold_secs * 1000 && mtime_ms < threshold_secs * 1000 {
                continue;
            }

            // Determine last activity: use max of history timestamp and file mtime (both in ms)
            let last_activity = entry.timestamp.max(mtime_ms);

            // Try to get recent message/tool from session file
            let (last_message, last_tool) = session_path
                .as_ref()
                .and_then(|p| Self::read_last_session_entry(p))
                .map(|(msg, tool, _)| (msg, tool))
                .unwrap_or((entry.display.clone(), None));

            let session_id_str = format!("claude:{}", session_id);

            sessions.push(ActiveSession {
                session_id: session_id_str,
                provider: "claude".to_string(),
                agent_id: entry.agent_id.clone(),
                agent_type: entry.agent_type.clone(),
                model: entry.model.clone(),
                status: "active".to_string(),
                last_activity,
                project: Some(entry.project.clone()),
                last_message,
                last_tool,
                last_tool_input: None,
                parent_session_id: None,
            });

            // Step 3: Also scan subagents dirs if we found the session directory
            if let Some(dir) = session_dir {
                let subagents = self.scan_subagents(&dir);
                for subagent in subagents {
                    let sub_session_id = format!("claude:{}:{}", session_id, subagent.agent_id);
                    sessions.push(ActiveSession {
                        session_id: sub_session_id,
                        provider: "claude".to_string(),
                        agent_id: Some(subagent.agent_id),
                        agent_type: "sub-agent".to_string(),
                        model: entry.model.clone(),
                        status: "active".to_string(),
                        last_activity: subagent.last_activity,
                        project: Some(entry.project.clone()),
                        last_message: subagent.last_message,
                        last_tool: subagent.last_tool,
                        last_tool_input: None,
                        parent_session_id: Some(format!("claude:{}", session_id)),
                    });
                }
            }
        }

        // Also scan project directories for any session files not in history
        let projects_dir = Self::projects_dir();
        if projects_dir.exists() {
            let mut paths = Vec::new();
            let _ = walkdir_recursive(&projects_dir, &mut paths, 0, 10);

            for path in &paths {
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                // Skip subagent files - those are handled separately
                if path.to_string_lossy().contains("/subagents/") {
                    continue;
                }

                let session_id = Self::parse_session_id(path);
                if session_id.is_empty() {
                    continue;
                }

                // Skip if already in sessions from history
                if history_entries.contains_key(&session_id) {
                    continue;
                }

                if let Ok(stat) = std::fs::metadata(path) {
                    let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                    let mtime_ms = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                    if mtime_ms < threshold_secs * 1000 {
                        continue;
                    }

                    let (last_message, last_tool, last_activity) =
                        Self::read_last_session_entry(path).unwrap_or((None, None, mtime_ms));

                    // Get project from parent directory name
                    let project = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .map(|n| Self::decode_project_path(n));

                    sessions.push(ActiveSession {
                        session_id: format!("claude:{}", session_id),
                        provider: "claude".to_string(),
                        agent_id: None,
                        agent_type: "main".to_string(),
                        model: "unknown".to_string(),
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

        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        // Session ID format: "claude:{sessionId}" or "claude:{sessionId}:{agentId}" for subagents
        let session_id = session_id.strip_prefix("claude:")?;

        // Check for subagent format: sessionId:agentId
        let (session_id, is_subagent) = if let Some(idx) = session_id.rfind(':') {
            let potential_agent_id = &session_id[idx + 1..];
            // If this looks like an agent ID (not just another part of the session ID),
            // check if there's a subagent file for it
            let test_session_id = &session_id[..idx];
            let subagent_path = Self::projects_dir()
                .join(Self::encode_project_path("")) // Will need project info from history
                .join(test_session_id)
                .join("subagents")
                .join(format!("agent-{}.jsonl", potential_agent_id));

            if subagent_path.exists() || test_session_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                (session_id, true)
            } else {
                (session_id, false)
            }
        } else {
            (session_id, false)
        };

        // First check history.jsonl for session metadata
        let history_entries = self.read_history_entries();

        if let Some(entry) = history_entries.get(session_id) {
            let encoded_project = Self::encode_project_path(&entry.project);
            let project_session_dir = Self::projects_dir().join(&encoded_project);

            if is_subagent {
                // Find the subagent file - we need to parse the agent ID from the session_id
                if let Some(colon_idx) = session_id.rfind(':') {
                    let agent_id = &session_id[colon_idx + 1..];
                    let subagent_path = project_session_dir
                        .join(session_id[..colon_idx].to_string())
                        .join("subagents")
                        .join(format!("agent-{}.jsonl", agent_id));

                    if let Ok(stat) = std::fs::metadata(&subagent_path) {
                        let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                        let mtime_ms = mtime
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as i64;
                        let (last_message, last_tool, _) =
                            Self::read_last_session_entry(&subagent_path)
                                .unwrap_or((None, None, mtime_ms));

                        return Some(ActiveSession {
                            session_id: format!("claude:{}", session_id),
                            provider: "claude".to_string(),
                            agent_id: Some(agent_id.to_string()),
                            agent_type: "sub-agent".to_string(),
                            model: entry.model.clone(),
                            status: "active".to_string(),
                            last_activity: mtime_ms,
                            project: Some(entry.project.clone()),
                            last_message,
                            last_tool,
                            last_tool_input: None,
                            parent_session_id: Some(format!("claude:{}", &session_id[..colon_idx])),
                        });
                    }
                }
            } else {
                // Main session
                let session_path = project_session_dir.join(format!("{}.jsonl", session_id));

                let (last_message, last_tool, last_activity) =
                    Self::read_last_session_entry(&session_path)
                        .unwrap_or((entry.display.clone(), None, entry.timestamp));

                return Some(ActiveSession {
                    session_id: format!("claude:{}", session_id),
                    provider: "claude".to_string(),
                    agent_id: entry.agent_id.clone(),
                    agent_type: entry.agent_type.clone(),
                    model: entry.model.clone(),
                    status: "active".to_string(),
                    last_activity,
                    project: Some(entry.project.clone()),
                    last_message,
                    last_tool,
                    last_tool_input: None,
                    parent_session_id: None,
                });
            }
        }

        // Fallback: try to find session file directly in projects directory
        // Try all project directories
        let projects_dir = Self::projects_dir();
        if projects_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&projects_dir) {
                for entry in entries.flatten() {
                    let project_path = entry.path();
                    if !project_path.is_dir() {
                        continue;
                    }

                    let session_path = project_path.join(format!("{}.jsonl", session_id));
                    if session_path.exists() {
                        let (last_message, last_tool, last_activity) =
                            Self::read_last_session_entry(&session_path)
                                .unwrap_or((None, None, 0));

                        let project = project_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| Self::decode_project_path(n));

                        return Some(ActiveSession {
                            session_id: format!("claude:{}", session_id),
                            provider: "claude".to_string(),
                            agent_id: None,
                            agent_type: "main".to_string(),
                            model: "unknown".to_string(),
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

        None
    }
}