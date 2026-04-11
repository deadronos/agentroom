use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;

pub struct CodexAdapter;

#[derive(Debug, Clone)]
struct CodexEntry {
    entry_type: String,
    payload: serde_json::Value,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn sessions_dir() -> PathBuf {
        Self::home_dir().join(".codex").join("sessions")
    }

    /// Parse session ID from filename like "rollout-2025-01-22T10-30-00-abc123.jsonl"
    fn parse_session_id(path: &PathBuf) -> Option<String> {
        let filename = path.file_stem()?;
        let name = filename.to_str()?;
        // Expected format: rollout-{timestamp}
        let ts = name.strip_prefix("rollout-")?;
        Some(ts.to_string())
    }

    fn parse_jsonl_entry(line: &str) -> Option<CodexEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let entry_type = json.get("type")?.as_str()?.to_string();
        let payload = json.get("payload")?.clone();
        Some(CodexEntry { entry_type, payload })
    }

    /// Read all JSONL entries from a file
    fn read_jsonl_entries(path: &PathBuf) -> Option<Vec<CodexEntry>> {
        let content = std::fs::read_to_string(path).ok()?;
        let entries: Vec<CodexEntry> = content
            .lines()
            .filter_map(|line| Self::parse_jsonl_entry(line))
            .collect();
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    /// Extract model from session entries
    fn extract_model(entries: &[CodexEntry]) -> String {
        for entry in entries {
            if entry.entry_type == "session_meta" {
                if let Some(model) = entry.payload.get("model") {
                    if let Some(s) = model.as_str() {
                        return s.to_string();
                    }
                }
            }
            if entry.entry_type == "turn_context" {
                if let Some(model) = entry.payload.get("model") {
                    if let Some(s) = model.as_str() {
                        return s.to_string();
                    }
                }
            }
            if entry.entry_type == "event_msg" {
                if let Some(model) = entry.payload.get("model") {
                    if let Some(s) = model.as_str() {
                        return s.to_string();
                    }
                }
            }
        }
        "unknown".to_string()
    }

    /// Extract project (cwd) from session entries
    fn extract_project(entries: &[CodexEntry]) -> Option<String> {
        for entry in entries {
            if entry.entry_type == "session_meta" {
                if let Some(cwd) = entry.payload.get("cwd") {
                    if let Some(s) = cwd.as_str() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    }

    /// Extract last tool call from response_item entries
    fn extract_last_tool(entries: &[CodexEntry]) -> Option<String> {
        for entry in entries.iter().rev() {
            if entry.entry_type == "response_item" {
                let payload = &entry.payload;
                if let Some(payload_type) = payload.get("type") {
                    if payload_type.as_str() == Some("function_call") {
                        if let Some(name) = payload.get("name") {
                            return name.as_str().map(String::from);
                        }
                    }
                }
            }
        }
        None
    }

    /// Extract last message content from response_item entries
    fn extract_last_message(entries: &[CodexEntry]) -> Option<String> {
        for entry in entries.iter().rev() {
            if entry.entry_type == "response_item" {
                let payload = &entry.payload;
                if let Some(payload_type) = payload.get("type") {
                    if payload_type.as_str() == Some("message") {
                        if let Some(content) = payload.get("content") {
                            if let Some(arr) = content.as_array() {
                                for item in arr {
                                    if let Some(text) = item.get("text") {
                                        if let Some(s) = text.as_str() {
                                            return Some(s.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if file matches the rollout-*.jsonl pattern
    fn is_rollout_file(path: &PathBuf) -> bool {
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            return false;
        }
        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            return filename.starts_with("rollout-");
        }
        false
    }

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
        total_days + day - 1
    }

    /// Get current date parts (year, month, day) as i64
    fn current_date_parts() -> (i64, i64, i64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        // Days since epoch
        let days = now.as_secs() / 86400;
        let mut year = 1970;
        let mut remaining_days = days as i64;

        loop {
            let days_in_year = if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                366
            } else {
                365
            };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }

        let is_leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let days_in_month = |y: i64, m: i64| match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap(y) => 29,
            2 => 28,
            _ => 0,
        };

        let mut month = 1;
        while month <= 12 {
            let dim = days_in_month(year, month);
            if remaining_days < dim {
                break;
            }
            remaining_days -= dim;
            month += 1;
        }
        let day = remaining_days + 1;

        (year, month, day)
    }

    /// Check if a file's mtime is within the threshold
    fn is_file_recent(path: &PathBuf, threshold_ms: i64) -> bool {
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

    /// Build path to a session file for a given date
    fn session_path_for_date(timestamp: &str, year: i64, month: i64, day: i64) -> PathBuf {
        Self::sessions_dir()
            .join(format!("{}", year))
            .join(format!("{:02}", month))
            .join(format!("{:02}", day))
            .join(format!("rollout-{}.jsonl", timestamp))
    }
}

impl SessionAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "codex"
    }

    fn is_available(&self) -> bool {
        Self::sessions_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let sessions_dir = Self::sessions_dir();
        if sessions_dir.exists() {
            vec![WatchPath {
                path: sessions_dir,
                watch_type: WatchType::Directory,
                filter: Some("rollout-*.jsonl".to_string()),
                recursive: true,
            }]
        } else {
            vec![]
        }
    }

    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - threshold_ms as i64;
        let mut sessions = Vec::new();

        let dir = Self::sessions_dir();
        if !dir.exists() {
            return sessions;
        }

        // Walk only today's and yesterday's directories for recent sessions
        let (cur_year, cur_month, cur_day) = Self::current_date_parts();

        for days_ago in 0..=1 {
            let (year, month, day) = if days_ago == 0 {
                (cur_year, cur_month, cur_day)
            } else {
                // Calculate yesterday's date
                let total_days = Self::days_since_epoch(cur_year, cur_month, cur_day) - 1;
                let mut y = 1970;
                let mut remaining = total_days;
                loop {
                    let days_in_year = if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                        366
                    } else {
                        365
                    };
                    if remaining < days_in_year {
                        break;
                    }
                    remaining -= days_in_year;
                    y += 1;
                }
                let is_leap = |yr: i64| (yr % 4 == 0 && yr % 100 != 0) || (yr % 400 == 0);
                let dim = |yr: i64, m: i64| -> i64 {
                    match m {
                        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                        4 | 6 | 9 | 11 => 30,
                        2 if is_leap(yr) => 29,
                        2 => 28,
                        _ => 0,
                    }
                };
                let mut m = 1;
                while m <= 12 {
                    if remaining < dim(y, m) {
                        break;
                    }
                    remaining -= dim(y, m);
                    m += 1;
                }
                (y, m, remaining + 1)
            };

            let date_path = dir
                .join(format!("{}", year))
                .join(format!("{:02}", month))
                .join(format!("{:02}", day));

            if !date_path.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(&date_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !Self::is_rollout_file(&path) {
                        continue;
                    }

                    if !Self::is_file_recent(&path, threshold) {
                        continue;
                    }

                    let mtime_ms = std::fs::metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as i64
                        })
                        .unwrap_or(0);

                    let session_id = Self::parse_session_id(&path)
                        .map(|ts| format!("codex:{}", ts))
                        .unwrap_or_else(|| format!("codex:{}", mtime_ms));

                    let entries_data = Self::read_jsonl_entries(&path);
                    let model = entries_data
                        .as_ref()
                        .map(|e| Self::extract_model(e))
                        .unwrap_or_else(|| "unknown".to_string());
                    let project = entries_data
                        .as_ref()
                        .and_then(|e| Self::extract_project(e));
                    let last_tool = entries_data
                        .as_ref()
                        .and_then(|e| Self::extract_last_tool(e));
                    let last_message = entries_data
                        .as_ref()
                        .and_then(|e| Self::extract_last_message(e));

                    sessions.push(ActiveSession {
                        session_id,
                        provider: "codex".to_string(),
                        agent_id: None,
                        agent_type: "main".to_string(),
                        model,
                        status: "active".to_string(),
                        last_activity: mtime_ms,
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
        // Session ID format: codex:{timestamp}
        let timestamp = session_id.strip_prefix("codex:")?;

        // Try to find the file in sessions/YYYY/MM/DD/rollout-{timestamp}.jsonl
        let _sessions_dir = Self::sessions_dir();
        let (cur_year, cur_month, cur_day) = Self::current_date_parts();

        for days_ago in 0..=30 {
            // Calculate the target date
            let total_days = Self::days_since_epoch(cur_year, cur_month, cur_day) - days_ago;
            let mut year = 1970;
            let mut remaining = total_days;
            loop {
                let days_in_year = if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                    366
                } else {
                    365
                };
                if remaining < days_in_year {
                    break;
                }
                remaining -= days_in_year;
                year += 1;
            }
            let is_leap = |yr: i64| (yr % 4 == 0 && yr % 100 != 0) || (yr % 400 == 0);
            let dim = |yr: i64, m: i64| -> i64 {
                match m {
                    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                    4 | 6 | 9 | 11 => 30,
                    2 if is_leap(yr) => 29,
                    2 => 28,
                    _ => 0,
                }
            };
            let mut month = 1;
            while month <= 12 {
                if remaining < dim(year, month) {
                    break;
                }
                remaining -= dim(year, month);
                month += 1;
            }
            let day = remaining + 1;

            let path = Self::session_path_for_date(timestamp, year, month, day);

            if path.exists() {
                let entries_data = Self::read_jsonl_entries(&path);
                let model = entries_data
                    .as_ref()
                    .map(|e| Self::extract_model(e))
                    .unwrap_or_else(|| "unknown".to_string());
                let project = entries_data
                    .as_ref()
                    .and_then(|e| Self::extract_project(e));
                let last_tool = entries_data
                    .as_ref()
                    .and_then(|e| Self::extract_last_tool(e));
                let last_message = entries_data
                    .as_ref()
                    .and_then(|e| Self::extract_last_message(e));

                let stat = std::fs::metadata(&path).ok()?;
                let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                let mtime_ms = mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                return Some(ActiveSession {
                    session_id: session_id.to_string(),
                    provider: "codex".to_string(),
                    agent_id: None,
                    agent_type: "main".to_string(),
                    model,
                    status: "active".to_string(),
                    last_activity: mtime_ms,
                    project,
                    last_message,
                    last_tool,
                    last_tool_input: None,
                    parent_session_id: None,
                });
            }
        }

        None
    }
}
