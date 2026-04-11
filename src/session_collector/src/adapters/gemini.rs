use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;
use std::env;

/// SHA-256 hash of a path string (as hex lowercase)
fn sha256_hex(s: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Reverse resolve projectHash -> actual project path
fn resolve_project_path(project_hash: &str) -> Option<String> {
    let home = dirs::home_dir()?;
    let candidates: Vec<PathBuf> = std::iter::once(home.clone())
        .chain(
            ["Desktop", "Documents", "Projects", "Developer", "dev", "src", "code", "repos", "workspace", "work"]
                .iter()
                .map(|d| home.join(d))
                .filter(|p| p.is_dir())
        )
        .chain({
            // Subdirs of common dirs (up to 2 levels deep)
            let mut subdirs = Vec::new();
            for d in ["Desktop", "Documents", "Projects", "Developer", "dev", "src", "code", "repos", "workspace", "work"] {
                let base = home.join(d);
                if base.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&base) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let path = entry.path();
                            if path.is_dir() {
                                subdirs.push(path.clone());
                                // One more level deep
                                if let Ok(subentries) = std::fs::read_dir(&path) {
                                    for subentry in subentries.filter_map(|e| e.ok()) {
                                        let subpath = subentry.path();
                                        if subpath.is_dir() {
                                            subdirs.push(subpath);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            subdirs
        })
        .chain({
            // Claude Code project paths
            let mut paths = Vec::new();
            let claude_projects = home.join(".claude").join("projects");
            if claude_projects.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&claude_projects) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir() {
                            paths.push(path);
                        }
                    }
                }
            }
            paths
        })
        .collect();

    for candidate in candidates {
        let path_str = candidate.to_string_lossy();
        if sha256_hex(&path_str) == project_hash {
            return Some(path_str.to_string());
        }
    }
    None
}

/// Scan ~/.gemini/tmp/*/chats/ for session-*.json files
fn scan_chats_dirs() -> Vec<PathBuf> {
    let mut results = Vec::new();
    let tmp_dir = match dirs::home_dir() {
        Some(home) => home.join(".gemini").join("tmp"),
        None => return results,
    };
    if !tmp_dir.is_dir() {
        return results;
    }

    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let project_dir = entry.path();
            if project_dir.is_dir() {
                let chats_dir = project_dir.join("chats");
                if chats_dir.is_dir() {
                    if let Ok(session_entries) = std::fs::read_dir(&chats_dir) {
                        for session_entry in session_entries.filter_map(|e| e.ok()) {
                            let path = session_entry.path();
                            if path.is_file() {
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    if stem.starts_with("session-") && path.extension().and_then(|e| e.to_str()) == Some("json") {
                                        results.push(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

fn parse_timestamp(ts: &str) -> i64 {
    // Parse RFC3339 timestamp like "2024-01-15T10:30:00Z" or "2024-01-15T10:30:00+00:00"
    // Format: YYYY-MM-DDTHH:MM:SS[timezone]
    let parts: Vec<&str> = ts.split(&['T', '-', ':', '+', 'Z', 'z'][..]).collect();
    if parts.len() < 6 {
        return 0;
    }
    let year: i64 = parts[0].parse().unwrap_or(0);
    let month: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let day: i64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let hour: i64 = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let minute: i64 = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let second: i64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Days from year 0 to given date (simplified, ignoring timezone offset for now)
    let days = date_to_days(year, month, day);
    let total_seconds = days * 86400 + hour * 3600 + minute * 60 + second;
    total_seconds * 1000
}

fn date_to_days(year: i64, month: i64, day: i64) -> i64 {
    // Simplified: convert date to days since epoch (ignoring leap seconds and exact calendar rules)
    let mut days = (year - 1970) * 365;
    days += (year - 1970) / 4;
    days -= (year - 1970) / 100;
    days += (year - 1970) / 400;
    for m in 1..month {
        days += days_in_month(year, m);
    }
    days + day - 1
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn parse_session(path: &PathBuf) -> Option<(String, Option<String>, Option<String>, Option<String>, i64)> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let session_id = json.get("sessionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let messages = json.get("messages").and_then(|v| v.as_array())?;

    let mut last_message: Option<String> = None;
    let mut last_tool: Option<String> = None;
    let mut last_tool_input: Option<String> = None;
    let mut model: Option<String> = None;
    let mut last_activity: i64 = 0;

    for msg in messages {
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match msg_type {
            "user" | "gemini" => {
                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                    last_message = Some(content.to_string());
                }
                if msg_type == "gemini" {
                    if let Some(m) = msg.get("model").and_then(|v| v.as_str()) {
                        model = Some(m.to_string());
                    }
                }
                if let Some(ts) = msg.get("timestamp").and_then(|v| v.as_str()) {
                    let ts_ms = parse_timestamp(ts);
                    if ts_ms > last_activity {
                        last_activity = ts_ms;
                    }
                }
            }
            "tool_call" => {
                if let Some(name) = msg.get("name").and_then(|v| v.as_str()) {
                    last_tool = Some(name.to_string());
                }
                if let Some(input) = msg.get("input") {
                    last_tool_input = serde_json::to_string(&input).ok();
                }
                if let Some(ts) = msg.get("timestamp").and_then(|v| v.as_str()) {
                    let ts_ms = parse_timestamp(ts);
                    if ts_ms > last_activity {
                        last_activity = ts_ms;
                    }
                }
            }
            _ => {}
        }
    }

    if session_id.is_empty() {
        return None;
    }

    Some((model.unwrap_or_else(|| "unknown".to_string()), last_tool, last_tool_input, last_message, last_activity))
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

pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
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
}

impl SessionAdapter for GeminiAdapter {
    fn name(&self) -> &str {
        "gemini"
    }

    fn is_available(&self) -> bool {
        Self::tmp_dir().exists() || !Self::extra_scan_dirs().is_empty()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let mut paths = Vec::new();
        let tmp_dir = Self::tmp_dir();
        if tmp_dir.exists() {
            paths.push(WatchPath {
                path: tmp_dir,
                watch_type: WatchType::Directory,
                filter: Some("session-*.json".to_string()),
                recursive: true,
            });
        }
        // Extra scan dirs
        for dir in Self::extra_scan_dirs() {
            paths.push(WatchPath {
                path: dir,
                watch_type: WatchType::Directory,
                filter: Some("session-*.json".to_string()),
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
            if Self::tmp_dir().exists() {
                dirs.push(Self::tmp_dir());
            }
            dirs.extend(Self::extra_scan_dirs());
            dirs
        };

        for dir in all_dirs {
            if !dir.exists() {
                continue;
            }

            let paths = if dir == Self::tmp_dir() {
                // For tmp dir, scan for session-*.json
                scan_chats_dirs()
            } else {
                // Extra scan dirs: recursive walk
                let mut paths = Vec::new();
                let _ = walkdir_recursive(&dir, &mut paths, 0, 10);
                paths
            };

            for path in paths {
                // For tmp dir, filter is session-*.json already applied in scan_chats_dirs
                // For extra dirs, filter manually
                if dir == Self::tmp_dir() {
                    if path.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    if !path.file_stem().and_then(|s| s.to_str()).map(|s| s.starts_with("session-")).unwrap_or(false) {
                        continue;
                    }
                } else {
                    if path.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    if !path.file_stem().and_then(|s| s.to_str()).map(|s| s.starts_with("session-")).unwrap_or(false) {
                        continue;
                    }
                }

                if !is_active(&path, threshold) {
                    continue;
                }

                if let Some((model, last_tool, last_tool_input, last_message, last_activity)) = parse_session(&path) {
                    // Extract sessionId from filename: session-{sessionId}.json
                    let session_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let session_id_from_file = session_stem.strip_prefix("session-").unwrap_or(session_stem);
                    let session_id = format!("gemini:{}", session_id_from_file);

                    // Extract projectHash from path: ~/.gemini/tmp/{projectHash}/chats/session-{sessionId}.json
                    let project_hash = path.ancestors().nth(2)
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .map(String::from);

                    let project = project_hash.and_then(|h| resolve_project_path(&h));

                    sessions.push(ActiveSession {
                        session_id,
                        provider: "gemini".to_string(),
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
            }
        }
        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let path_str = session_id.strip_prefix("gemini:")?;
        let path = PathBuf::from(path_str);
        if !path.exists() {
            return None;
        }
        parse_session(&path).map(|(model, last_tool, last_tool_input, last_message, last_activity)| {
            let session_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let session_id_from_file = session_stem.strip_prefix("session-").unwrap_or(session_stem);
            let full_session_id = format!("gemini:{}", session_id_from_file);

            let project_hash = path.ancestors().nth(2)
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(String::from);

            let project = project_hash.and_then(|h| resolve_project_path(&h));

            ActiveSession {
                session_id: full_session_id,
                provider: "gemini".to_string(),
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
            }
        })
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
