use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;
use std::env;

pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn log_dir() -> PathBuf {
        Self::home_dir().join(".gemini").join("logs")
    }

    fn tmp_dir() -> PathBuf {
        Self::home_dir().join(".gemini").join("tmp")
    }

    /// Additional scan dirs from environment variable
    fn extra_scan_dirs() -> Vec<PathBuf> {
        let home = Self::home_dir();
        match env::var("AGENTROOM_GEMINI_SCAN_DIRS") {
            Ok(value) => value
                .split([',', ';'])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    if entry.starts_with("~/") {
                        home.join(entry.trim_start_matches("~/"))
                    } else {
                        PathBuf::from(entry)
                    }
                })
                .filter(|p| p.exists())
                .collect(),
            Err(_) => Vec::new(),
        }
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
        Some((ts, text.or(tool.clone()), tool))
    }

    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts))
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

impl SessionAdapter for GeminiAdapter {
    fn name(&self) -> &str {
        "gemini"
    }

    fn is_available(&self) -> bool {
        Self::log_dir().exists() || Self::tmp_dir().exists() || !Self::extra_scan_dirs().is_empty()
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
        let tmp_dir = Self::tmp_dir();
        if tmp_dir.exists() {
            paths.push(WatchPath {
                path: tmp_dir,
                watch_type: WatchType::Directory,
                filter: Some("*.jsonl".to_string()),
                recursive: true,
            });
        }
        // Extra scan dirs
        for dir in Self::extra_scan_dirs() {
            paths.push(WatchPath {
                path: dir,
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

        let all_dirs: Vec<PathBuf> = {
            let mut dirs = Vec::new();
            if Self::log_dir().exists() { dirs.push(Self::log_dir()); }
            if Self::tmp_dir().exists() { dirs.push(Self::tmp_dir()); }
            dirs.extend(Self::extra_scan_dirs());
            dirs
        };

        for dir in all_dirs {
            if !dir.exists() {
                continue;
            }
            let mut paths = Vec::new();
            let _ = walkdir_recursive(&dir, &mut paths, 0, 10);

            for path in paths {
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                if !Self::is_active(&path, threshold) {
                    continue;
                }
                let session_id = format!("gemini:{}", Self::parse_session_id(&path));
                let (last_message, last_tool, last_activity) =
                    Self::read_last_jsonl_entry(&path).unwrap_or((None, None, threshold));
                sessions.push(ActiveSession {
                    session_id,
                    provider: "gemini".to_string(),
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
        let path_str = session_id.strip_prefix("gemini:")?;
        let path = PathBuf::from(path_str);
        let (last_message, last_tool, last_activity) =
            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0));
        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "gemini".to_string(),
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
