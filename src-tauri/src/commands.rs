use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionTag {
    session_id: String,
    summary: String,
    category: String,
    tagged_at: u64,
    model: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TagStore {
    version: u8,
    tags: HashMap<String, SessionTag>,
}

#[derive(serde::Deserialize)]
struct ModelTagResponse {
    summary: String,
    category: String,
}

fn cass_bin() -> String {
    env::var("CASS_BIN").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/Projects/AgentRoom/cass/target/release/cass", home)
    })
}

fn now_millis() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as u64,
        Err(_) => 0,
    }
}

fn chrono_now() -> String {
    let millis = now_millis();
    let secs = millis / 1000;
    let ms = millis % 1000;
    format!("{secs}.{ms:03}")
}

fn default_tag_store() -> TagStore {
    TagStore {
        version: 1,
        tags: HashMap::new(),
    }
}

fn tags_store_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    Ok(Path::new(&home)
        .join(".agentroom")
        .join("session-tags.json"))
}

fn load_tag_store() -> TagStore {
    let Ok(path) = tags_store_path() else {
        return default_tag_store();
    };

    let Ok(contents) = fs::read_to_string(path) else {
        return default_tag_store();
    };

    serde_json::from_str::<TagStore>(&contents).unwrap_or_else(|_| default_tag_store())
}

fn save_tag_store(store: &TagStore) -> Result<(), String> {
    let path = tags_store_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let payload = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(path, payload).map_err(|e| e.to_string())
}

fn normalize_summary(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "未分类".to_string();
    }

    truncate_chars_head_tail(trimmed, 24)
}

fn normalize_category(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "misc".to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let _ = lines.next();
    let body = lines.collect::<Vec<_>>().join("\n");
    body.trim_end_matches("```").trim().to_string()
}

fn extract_first_json_object(text: &str) -> Option<String> {
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }

        if ch == '{' {
            if start.is_none() {
                start = Some(idx);
            }
            depth += 1;
            continue;
        }

        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    return Some(text[s..=idx].to_string());
                }
            }
        }
    }

    None
}

fn parse_model_tag_json(text: &str) -> Option<ModelTagResponse> {
    let cleaned = strip_markdown_fences(text);
    if let Ok(parsed) = serde_json::from_str::<ModelTagResponse>(&cleaned) {
        return Some(parsed);
    }

    let extracted = extract_first_json_object(&cleaned)?;
    serde_json::from_str::<ModelTagResponse>(&extracted).ok()
}

fn collect_text_from_value(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => String::new(),
        Value::String(text) => text.to_string(),
        Value::Array(items) => items
            .iter()
            .map(collect_text_from_value)
            .collect::<Vec<_>>()
            .join(""),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return text.to_string();
            }

            for key in ["message", "content", "result", "response"] {
                if let Some(found) = map.get(key) {
                    let collected = collect_text_from_value(found);
                    if !collected.trim().is_empty() {
                        return collected;
                    }
                }
            }

            String::new()
        }
    }
}

fn parse_tag_from_json_value(root: &Value) -> Option<ModelTagResponse> {
    if let (Some(summary), Some(category)) = (
        root.get("summary").and_then(Value::as_str),
        root.get("category").and_then(Value::as_str),
    ) {
        return Some(ModelTagResponse {
            summary: summary.to_string(),
            category: category.to_string(),
        });
    }

    let text = collect_text_from_value(root);
    if text.trim().is_empty() {
        return None;
    }
    parse_model_tag_json(&text)
}

fn parse_tag_from_claude_stdout(stdout: &str) -> Option<ModelTagResponse> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = serde_json::from_str::<ModelTagResponse>(trimmed) {
        return Some(parsed);
    }

    if let Ok(root) = serde_json::from_str::<Value>(trimmed) {
        if let Some(parsed) = parse_tag_from_json_value(&root) {
            return Some(parsed);
        }
    }

    for line in trimmed.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(root) = serde_json::from_str::<Value>(line) {
            if let Some(parsed) = parse_tag_from_json_value(&root) {
                return Some(parsed);
            }
            continue;
        }

        if let Some(extracted) = extract_first_json_object(line) {
            if let Ok(root) = serde_json::from_str::<Value>(&extracted) {
                if let Some(parsed) = parse_tag_from_json_value(&root) {
                    return Some(parsed);
                }
            }
        }
    }

    if let Some(extracted) = extract_first_json_object(trimmed) {
        if let Ok(root) = serde_json::from_str::<Value>(&extracted) {
            return parse_tag_from_json_value(&root);
        }
    }

    None
}

fn parse_tag_from_gemini_stdout(stdout: &str) -> Option<ModelTagResponse> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(root) = serde_json::from_str::<Value>(trimmed) {
        if let Some(parsed) = parse_tag_from_json_value(&root) {
            return Some(parsed);
        }
    }

    for line in trimmed.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(root) = serde_json::from_str::<Value>(line) {
            if let Some(parsed) = parse_tag_from_json_value(&root) {
                return Some(parsed);
            }

            if let Some(response) = root.get("response").and_then(Value::as_str) {
                if let Some(parsed) = parse_model_tag_json(response) {
                    return Some(parsed);
                }
            }
            continue;
        }

        if let Some(extracted) = extract_first_json_object(line) {
            if let Ok(root) = serde_json::from_str::<Value>(&extracted) {
                if let Some(parsed) = parse_tag_from_json_value(&root) {
                    return Some(parsed);
                }
            }
        }
    }

    if let Some(extracted) = extract_first_json_object(trimmed) {
        if let Ok(root) = serde_json::from_str::<Value>(&extracted) {
            if let Some(parsed) = parse_tag_from_json_value(&root) {
                return Some(parsed);
            }
            if let Some(response) = root.get("response").and_then(Value::as_str) {
                return parse_model_tag_json(response);
            }
        }
    }

    None
}

fn workspace_basename(workspace: Option<&str>) -> String {
    workspace
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown".to_string())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn truncate_chars_head_tail(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }

    if max_chars <= 3 {
        return chars.into_iter().take(max_chars).collect();
    }

    let keep = max_chars - 3;
    let head = keep.div_ceil(2);
    let tail = keep / 2;
    let prefix: String = chars.iter().take(head).copied().collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(tail)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

fn normalize_randomness(raw: Option<f64>) -> f64 {
    raw.unwrap_or(0.2).clamp(0.0, 1.0)
}

fn randomness_instruction(randomness: f64) -> &'static str {
    if randomness <= 0.15 {
        "Prefer deterministic, literal summaries. Avoid creativity."
    } else if randomness <= 0.45 {
        "Keep summaries precise with mild abstraction."
    } else if randomness <= 0.75 {
        "Allow moderate abstraction while staying faithful."
    } else {
        "Allow creative phrasing, but do not invent missing facts."
    }
}

fn build_tag_prompt(
    agent: &str,
    title: &str,
    workspace: Option<&str>,
    context: &str,
    randomness: f64,
) -> String {
    let ws = workspace_basename(workspace);
    format!(
        r#"You are tagging a coding agent chat session. Respond with EXACTLY this JSON, nothing else:
{{"summary":"<凝练标题, 可中英混合>","category":"<项目名 or 'misc'>"}}

Session:
- Agent: {agent}
- Workspace: {workspace}
- Title: {title}

User messages:
{context}

Rules:
- summary: 6-24 chars, concise session title, Chinese/English mixed is allowed.
- summary should be specific and action-oriented. Avoid generic words like "讨论一下" or "记录".
- examples: "Gemini Resume修复", "AgentRoom Tag系统", "Claude路径解析", "搜索过滤重构"
- category: Project/topic name if clear (e.g., "AgentRoom", "VibeLab", "kobo-note"), else "misc"
- Derive project from workspace path if conversation is ambiguous
- creativity/randomness (0-1): {randomness}
- {randomness_instruction}
- Output valid JSON only, no markdown fences"#,
        agent = truncate_chars(agent, 64),
        workspace = truncate_chars(&ws, 64),
        title = truncate_chars(title, 160),
        context = truncate_chars(context, 4000),
        randomness = randomness,
        randomness_instruction = randomness_instruction(randomness)
    )
}

fn upsert_tag(
    session_id: &str,
    summary: &str,
    category: &str,
    model: Option<String>,
) -> Result<SessionTag, String> {
    let mut store = load_tag_store();
    let normalized = SessionTag {
        session_id: session_id.to_string(),
        summary: normalize_summary(summary),
        category: normalize_category(category),
        tagged_at: now_millis(),
        model,
    };
    store
        .tags
        .insert(session_id.to_string(), normalized.clone());
    save_tag_store(&store)?;
    Ok(normalized)
}

fn get_existing_tag(session_id: &str) -> Option<SessionTag> {
    load_tag_store().tags.get(session_id).cloned()
}

fn is_hex_64(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn extract_gemini_project_hash(path: &str) -> Option<String> {
    let marker = "/.gemini/tmp/";
    let start = path.find(marker)?;
    let rest = &path[start + marker.len()..];
    let hash = rest.split('/').next()?;
    if is_hex_64(hash) {
        Some(hash.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_gemini_hash_workspace(path: &str) -> bool {
    extract_gemini_project_hash(path).is_some()
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn load_gemini_trusted_folders() -> Vec<String> {
    let home = match env::var("HOME") {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let path = Path::new(&home).join(".gemini").join("trustedFolders.json");
    let contents = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let parsed: Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    parsed
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn extract_workspace_from_jsonl(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(2000) {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(cwd) = parsed.get("cwd").and_then(Value::as_str) {
            let trimmed = cwd.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        if let Some(workspace) = parsed.get("workspace").and_then(Value::as_str) {
            let trimmed = workspace.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn resolve_claude_main_session_file(source: &Path) -> Option<PathBuf> {
    let file_name = source.file_name()?.to_str()?;
    if !(file_name.starts_with("agent-") && file_name.ends_with(".jsonl")) {
        return None;
    }

    let subagents_dir = source.parent()?;
    if subagents_dir.file_name()?.to_str()? != "subagents" {
        return None;
    }

    let session_dir = subagents_dir.parent()?;
    let session_id = session_dir.file_name()?.to_str()?;
    let project_dir = session_dir.parent()?;
    let main_session = project_dir.join(format!("{session_id}.jsonl"));

    if main_session.exists() {
        Some(main_session)
    } else {
        None
    }
}

fn expand_home(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }

    path.to_string()
}

fn normalize_existing_dir(path: &str) -> Option<String> {
    let candidate = PathBuf::from(path);
    if !candidate.exists() || !candidate.is_dir() {
        return None;
    }

    fs::canonicalize(&candidate)
        .map(|resolved| resolved.to_string_lossy().to_string())
        .ok()
        .or_else(|| Some(candidate.to_string_lossy().to_string()))
}

fn extra_gemini_scan_dirs() -> Vec<String> {
    let home = env::var("HOME").unwrap_or_default();
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    let default_root = if home.is_empty() {
        None
    } else {
        normalize_existing_dir(&format!("{home}/.gemini/tmp"))
    };

    let mut candidates: Vec<String> = Vec::new();
    if let Ok(configured) = env::var("AGENTROOM_GEMINI_SCAN_DIRS") {
        candidates.extend(
            configured
                .split([',', ';'])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    if home.is_empty() {
                        entry.to_string()
                    } else {
                        expand_home(entry, &home)
                    }
                }),
        );
    }

    if !home.is_empty() {
        candidates.push(format!("{home}/openclaw/.gemini/tmp"));
    }

    for candidate in candidates {
        let Some(normalized) = normalize_existing_dir(&candidate) else {
            continue;
        };

        if default_root
            .as_ref()
            .is_some_and(|default_dir| default_dir == &normalized)
        {
            continue;
        }

        if seen.insert(normalized.clone()) {
            roots.push(normalized);
        }
    }

    roots
}

fn output_to_string(output: std::process::Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn normalize_agent_name(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('-', "_")
}

fn is_supported_agent_name(raw: &str) -> bool {
    matches!(
        normalize_agent_name(raw).as_str(),
        "claude_code" | "codex" | "gemini"
    )
}

fn session_source_exists(entry: &Value) -> bool {
    let Some(path) = entry.get("source_path").and_then(Value::as_str) else {
        return false;
    };
    Path::new(path).exists()
}

fn has_nonzero_message_count(entry: &Value) -> bool {
    match entry.get("message_count") {
        None => true,
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_u64() {
                return value > 0;
            }
            if let Some(value) = number.as_i64() {
                return value > 0;
            }
            true
        }
        Some(Value::String(value)) => value.trim().parse::<u64>().map(|count| count > 0).unwrap_or(true),
        _ => true,
    }
}

fn should_keep_session_entry(entry: &Value) -> bool {
    let Some(agent) = entry.get("agent").and_then(Value::as_str) else {
        return false;
    };

    if !is_supported_agent_name(agent) {
        return false;
    }

    if !session_source_exists(entry) {
        return false;
    }

    has_nonzero_message_count(entry)
}

fn filter_cass_session_list(raw_json: String) -> String {
    let Ok(mut parsed) = serde_json::from_str::<Value>(&raw_json) else {
        return raw_json;
    };

    let Some(root) = parsed.as_object_mut() else {
        return raw_json;
    };

    let Some(sessions) = root.get_mut("sessions").and_then(Value::as_array_mut) else {
        return raw_json;
    };

    sessions.retain(should_keep_session_entry);
    let kept = sessions.len();
    let _ = sessions;
    root.insert("total_sessions".to_string(), json!(kept));

    serde_json::to_string(&parsed).unwrap_or(raw_json)
}

fn filter_cass_search_hits(raw_json: String) -> String {
    let Ok(mut parsed) = serde_json::from_str::<Value>(&raw_json) else {
        return raw_json;
    };

    let Some(root) = parsed.as_object_mut() else {
        return raw_json;
    };

    let Some(hits) = root.get_mut("hits").and_then(Value::as_array_mut) else {
        return raw_json;
    };

    hits.retain(should_keep_session_entry);
    let kept = hits.len();
    let _ = hits;
    if root.contains_key("count") {
        root.insert("count".to_string(), json!(kept));
    }
    if root.contains_key("total_matches") {
        root.insert("total_matches".to_string(), json!(kept));
    }

    serde_json::to_string(&parsed).unwrap_or(raw_json)
}

fn is_gemini_session_path(path: &str) -> bool {
    path.contains("/.gemini/tmp/") && path.ends_with(".json")
}

fn load_gemini_session_raw(path: &str) -> Option<String> {
    if !is_gemini_session_path(path) {
        return None;
    }

    let contents = fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    let obj = parsed.as_object()?;

    let has_messages = obj.get("messages").is_some_and(Value::is_array);
    let has_session_id = obj.get("sessionId").is_some_and(Value::is_string);
    if has_messages && has_session_id {
        Some(contents)
    } else {
        None
    }
}

fn extract_gemini_session_id(path: &str) -> Option<String> {
    let contents = load_gemini_session_raw(path)?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    parsed
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_gemini_workspace_from_hash(
    source_path: &str,
    workspace: Option<&str>,
) -> Option<String> {
    if let Some(current_workspace) = workspace {
        let trimmed = current_workspace.trim();
        if !trimmed.is_empty() && !is_gemini_hash_workspace(trimmed) {
            return Some(trimmed.to_string());
        }
    }

    let target_hash = extract_gemini_project_hash(source_path)
        .or_else(|| workspace.and_then(extract_gemini_project_hash))?;

    for candidate in load_gemini_trusted_folders() {
        if sha256_hex(&candidate) == target_hash {
            return Some(candidate);
        }
    }

    if let Ok(home) = env::var("HOME") {
        let openclaw = format!("{home}/openclaw");
        if sha256_hex(&openclaw) == target_hash {
            return Some(openclaw);
        }
    }

    None
}

fn find_gemini_resume_index(list_output: &str, session_id: &str) -> Option<String> {
    let target = session_id.trim().to_ascii_lowercase();
    if target.is_empty() {
        return None;
    }

    for line in list_output.lines() {
        let trimmed = line.trim();
        let Some(dot_pos) = trimmed.find(". ") else {
            continue;
        };

        let index = &trimmed[..dot_pos];
        if index.is_empty() || !index.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        let Some(left_bracket) = trimmed.rfind('[') else {
            continue;
        };
        let Some(right_bracket) = trimmed.rfind(']') else {
            continue;
        };
        if right_bracket <= left_bracket {
            continue;
        }

        let listed_id = trimmed[left_bracket + 1..right_bracket]
            .trim()
            .to_ascii_lowercase();
        if listed_id == target {
            return Some(index.to_string());
        }
    }

    None
}

#[tauri::command]
pub async fn cass_search(
    query: String,
    mode: Option<String>,
    agent: Option<String>,
    limit: Option<u32>,
    days: Option<u32>,
) -> Result<String, String> {
    let mut args = vec![
        "search".into(),
        query,
        "--json".into(),
        "--limit".into(),
        limit.unwrap_or(50).to_string(),
    ];
    if let Some(m) = mode {
        if m != "lexical" {
            args.extend(["--mode".into(), m]);
        }
    }
    if let Some(a) = agent {
        args.extend(["--agent".into(), a]);
    }
    if let Some(d) = days {
        args.extend(["--days".into(), d.to_string()]);
    }

    let output = tokio::process::Command::new(cass_bin())
        .args(&args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let raw = output_to_string(output)?;
    Ok(filter_cass_search_hits(raw))
}

#[tauri::command]
pub async fn cass_sessions(days: Option<u32>) -> Result<String, String> {
    let d = days.unwrap_or(90);
    let since = format!("{}d", d);
    let args = vec![
        "timeline",
        "--json",
        "--group-by",
        "none",
        "--since",
        &since,
    ];

    let output = tokio::process::Command::new(cass_bin())
        .args(&args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let raw = output_to_string(output)?;
    Ok(filter_cass_session_list(raw))
}

#[tauri::command]
pub async fn cass_transcript(path: String) -> Result<String, String> {
    let output = tokio::process::Command::new(cass_bin())
        .args(["export", "--format", "json", "--", &path])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    match output_to_string(output) {
        Ok(raw) => {
            if serde_json::from_str::<Value>(&raw).is_ok() {
                return Ok(raw);
            }

            if let Some(fallback) = load_gemini_session_raw(&path) {
                return Ok(fallback);
            }

            Ok(raw)
        }
        Err(error) => {
            if let Some(fallback) = load_gemini_session_raw(&path) {
                return Ok(fallback);
            }

            Err(error)
        }
    }
}

#[tauri::command]
pub async fn cass_index() -> Result<String, String> {
    let output = tokio::process::Command::new(cass_bin())
        .args(["index", "--json"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let base_json = output_to_string(output)?;

    let mut merged: Value = serde_json::from_str(&base_json).unwrap_or_else(|_| {
        json!({
            "ok": true,
            "message": "Indexed (non-JSON output from cass)",
        })
    });

    let mut indexed_extra_roots = Vec::new();
    let mut warnings = Vec::new();

    for root in extra_gemini_scan_dirs() {
        let mut command = tokio::process::Command::new(cass_bin());
        command.args(["index", "--json"]);
        command.env("GEMINI_HOME", &root);

        match command.output().await {
            Ok(extra_output) if extra_output.status.success() => {
                indexed_extra_roots.push(root);
            }
            Ok(extra_output) => {
                let error_text = String::from_utf8_lossy(&extra_output.stderr)
                    .trim()
                    .to_string();
                warnings.push(format!("gemini scan root {}: {}", root, error_text));
            }
            Err(error) => {
                warnings.push(format!("gemini scan root {}: {}", root, error));
            }
        }
    }

    if !indexed_extra_roots.is_empty() {
        merged["extra_gemini_scan_dirs"] = json!(indexed_extra_roots);
    }

    if !warnings.is_empty() {
        merged["warnings"] = json!(warnings);
    }

    Ok(merged.to_string())
}

#[tauri::command]
pub async fn cass_health() -> Result<String, String> {
    let output = tokio::process::Command::new(cass_bin())
        .args(["health", "--json"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        // Return a JSON error instead of Err so frontend can handle gracefully
        Ok(r#"{"healthy":false,"error":"CASS not available"}"#.to_string())
    }
}

#[tauri::command]
pub async fn load_tags() -> Result<String, String> {
    let store = load_tag_store();
    serde_json::to_string(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_tag(
    session_id: String,
    summary: String,
    category: String,
    model: Option<String>,
) -> Result<String, String> {
    let tag = upsert_tag(
        &session_id,
        &summary,
        &category,
        Some(model.unwrap_or_else(|| "manual".to_string())),
    )?;
    serde_json::to_string(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tag_session(
    session_id: String,
    title: String,
    agent: String,
    workspace: Option<String>,
    context: String,
    force: Option<bool>,
    provider: Option<String>,
    model: Option<String>,
    randomness: Option<f64>,
) -> Result<String, String> {
    if !force.unwrap_or(false) {
        if let Some(existing) = get_existing_tag(&session_id) {
            return serde_json::to_string(&existing).map_err(|e| e.to_string());
        }
    }

    let provider = provider
        .as_deref()
        .unwrap_or("claude")
        .trim()
        .to_ascii_lowercase();
    let provider = if provider == "gemini" {
        "gemini"
    } else {
        "claude"
    };

    let randomness = normalize_randomness(randomness);
    let prompt = build_tag_prompt(&agent, &title, workspace.as_deref(), &context, randomness);

    let mut summary = "未分类".to_string();
    let mut category = "misc".to_string();

    let model_override = model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let (output, model_label) = if provider == "gemini" {
        let mut command = tokio::process::Command::new("gemini");
        command.args(["--output-format", "json", "--yolo"]);
        if let Some(model_name) = model_override.as_deref() {
            command.args(["--model", model_name]);
        }
        command.arg(&prompt);
        let label = match model_override.as_deref() {
            Some(model_name) => format!("gemini:{model_name}"),
            None => "gemini:default".to_string(),
        };
        (command.output().await, label)
    } else {
        let model_name = model_override.unwrap_or_else(|| "haiku".to_string());
        let output = tokio::process::Command::new("claude")
            .args([
                "-p",
                "--output-format",
                "json",
                "--model",
                &model_name,
                &prompt,
            ])
            .output()
            .await;
        (output, format!("claude:{model_name}"))
    };

    let log_path = Path::new(&env::var("HOME").unwrap_or_default())
        .join(".agentroom")
        .join("tag-debug.log");
    if let Ok(output) = &output {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let parsed = if provider == "gemini" {
            parse_tag_from_gemini_stdout(&stdout)
        } else {
            parse_tag_from_claude_stdout(&stdout)
        };
        if let Some(parsed) = parsed {
            summary = parsed.summary;
            category = parsed.category;
        } else {
            let msg = format!(
                "[{}] tag parse FAILED for {} | exit={} | stdout_len={} | stderr_len={}\nSTDOUT: {}\nSTDERR: {}\n---\n",
                chrono_now(),
                provider,
                output.status,
                stdout.len(),
                stderr.len(),
                &stdout[..stdout.len().min(500)],
                &stderr[..stderr.len().min(500)]
            );
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
        }
    } else if let Err(e) = &output {
        let msg = format!(
            "[{}] tag command SPAWN FAILED for {}: {}\n---\n",
            chrono_now(),
            provider,
            e
        );
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
    }

    let tag = upsert_tag(&session_id, &summary, &category, Some(model_label))?;
    serde_json::to_string(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_gemini_workspace(
    source_path: String,
    workspace: Option<String>,
) -> Result<String, String> {
    Ok(resolve_gemini_workspace_from_hash(&source_path, workspace.as_deref()).unwrap_or_default())
}

#[tauri::command]
pub async fn resolve_gemini_resume_target(
    source_path: String,
    workspace: Option<String>,
) -> Result<String, String> {
    let Some(session_id) = extract_gemini_session_id(&source_path) else {
        return Ok(String::new());
    };

    let workspace_dir = resolve_gemini_workspace_from_hash(&source_path, workspace.as_deref());

    if let Some(workspace_dir) = workspace_dir {
        let output = tokio::process::Command::new("gemini")
            .args(["--list-sessions"])
            .current_dir(&workspace_dir)
            .output()
            .await;

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(index) = find_gemini_resume_index(&stdout, &session_id) {
                    return Ok(index);
                }
            }
        }
    }

    Ok(session_id)
}

#[tauri::command]
pub async fn resolve_claude_workspace(
    source_path: String,
    workspace: Option<String>,
) -> Result<String, String> {
    if let Some(current_workspace) = workspace.as_deref() {
        let trimmed = current_workspace.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let source = PathBuf::from(&source_path);
    if source.exists() {
        if let Some(found) = extract_workspace_from_jsonl(&source) {
            return Ok(found);
        }
    }

    if let Some(main_session) = resolve_claude_main_session_file(&source) {
        if let Some(found) = extract_workspace_from_jsonl(&main_session) {
            return Ok(found);
        }
    }

    Ok(String::new())
}

#[tauri::command]
pub async fn run_osascript(script: String) -> Result<String, String> {
    let output = tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(r#"{"success":true}"#.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// ── AgentRoom Visual commands ────────────────────────────────────────

#[tauri::command]
pub async fn start_watching(app: tauri::AppHandle, project_dir: String) -> Result<String, String> {
    match crate::file_watcher::start_watching(app, &project_dir) {
        Ok(_shared) => Ok(r#"{"status":"watching"}"#.to_string()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn stop_watching() -> Result<String, String> {
    crate::file_watcher::stop_watching_inner();
    Ok(r#"{"status":"stopped"}"#.to_string())
}

#[tauri::command]
pub async fn get_active_agents() -> Result<String, String> {
    Ok("[]".to_string())
}

#[tauri::command]
pub async fn save_visual_layout(project_id: String, layout: String) -> Result<(), String> {
    let home = env::var("HOME").unwrap_or_default();
    let dir = PathBuf::from(&home).join(".agentroom").join("layouts");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", project_id));
    fs::write(path, layout).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn load_visual_layout(project_id: String) -> Result<String, String> {
    let home = env::var("HOME").unwrap_or_default();
    let path = PathBuf::from(&home)
        .join(".agentroom")
        .join("layouts")
        .join(format!("{}.json", project_id));
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_codexbar_snapshot() -> Result<String, String> {
    let home = env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let path = PathBuf::from(&home)
        .join("Library")
        .join("Group Containers")
        .join("group.com.steipete.codexbar")
        .join("widget-snapshot.json");
    fs::read_to_string(path).map_err(|e| format!("CodexBar++ snapshot not available: {}", e))
}
