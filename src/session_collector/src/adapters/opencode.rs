use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;

pub struct OpenCodeAdapter;

impl OpenCodeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn log_dir() -> PathBuf {
        Self::home_dir().join(".opencode").join("logs")
    }

    fn parse_session_id(path: &PathBuf) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn parse_jsonl_entry(line: &str) -> Option<(i64, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let ts = json.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tool = json.get("name").and_then(|v| v.as_str()).map(String::from);
        let text = json.get("text").and_then(|v| v.as_str()).map(String::from);
        Some((ts, tool.clone().or(text), tool))
    }

    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts))
    }
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

impl SessionAdapter for OpenCodeAdapter {
    fn name(&self) -> &str {
        "opencode"
    }

    fn is_available(&self) -> bool {
        Self::log_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let mut paths = Vec::new();
        let log_dir = Self::log_dir();
        if log_dir.exists() {
            paths.push(WatchPath {
                path: log_dir,
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

        let dir = Self::log_dir();
        if !dir.exists() {
            return sessions;
        }
        let mut paths = Vec::new();
        let _ = walkdir_recursive(&dir, &mut paths, 0, 10);

        for path in paths {
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(stat) = std::fs::metadata(&path) {
                let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                let mtime_ms = mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                if mtime_ms < threshold {
                    continue;
                }
                let session_id = format!("opencode:{}", Self::parse_session_id(&path));
                let (last_message, last_tool, last_activity) =
                    Self::read_last_jsonl_entry(&path).unwrap_or((None, None, mtime_ms));
                sessions.push(ActiveSession {
                    session_id,
                    provider: "opencode".to_string(),
                    agent_id: None,
                    agent_type: "main".to_string(),
                    model: "unknown".to_string(),
                    status: "active".to_string(),
                    last_activity,
                    project: path.parent().map(|p| p.to_string_lossy().to_string()),
                    last_message,
                    last_tool,
                    last_tool_input: None,
                    parent_session_id: None,
                });
            }
        }
        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let path_str = session_id.strip_prefix("opencode:")?;
        let path = PathBuf::from(path_str);
        let (last_message, last_tool, last_activity) =
            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0));
        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "opencode".to_string(),
            agent_id: None,
            agent_type: "main".to_string(),
            model: "unknown".to_string(),
            status: "active".to_string(),
            last_activity,
            project: path.parent().map(|p| p.to_string_lossy().to_string()),
            last_message,
            last_tool,
            last_tool_input: None,
            parent_session_id: None,
        })
    }
}
