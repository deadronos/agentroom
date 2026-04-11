# Session Adapter Alignment Plan — Align agentroom to claude-ville

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Align agentroom's Rust session collector adapters to match the actual session formats and paths used by claude-ville's JavaScript adapters.

**Root cause:** Agentroom's adapters use wrong paths and wrong JSONL parsing logic. They were built from assumptions, not from inspecting actual session files.

---

## File Inventory

Each task rewrites one adapter to match claude-ville's implementation exactly.

- Modify: `src/session_collector/src/adapters/copilot.rs`
- Modify: `src/session_collector/src/adapters/gemini.rs`
- Modify: `src/session_collector/src/adapters/claude.rs`
- Modify: `src/session_collector/src/adapters/openclaw.rs`
- Modify: `src/session_collector/src/adapters/codex.rs`

---

## Task 1: Rewrite Copilot Adapter

**Files:**
- Modify: `src/session_collector/src/adapters/copilot.rs`

**Actual copilot session format** (from claude-ville):
```
~/.copilot/session-state/{uuid}/events.jsonl

Entry types:
- {"type":"session.start","data":{"sessionId":"...","selectedModel":"gpt-5-mini","context":{"cwd":"/path","gitRoot":"...","branch":"main",...}},...}
- {"type":"user.message","data":{"content":"..."}}
- {"type":"assistant.message","data":{"content":[{"type":"text","text":"..."}],"selectedModel":"...","toolCalls":[...]}}
- {"type":"tool_call","data":{"name":"...","input":{...}}}
```

**Key changes needed:**
1. Watch path: `~/.copilot/session-state/` (not `sessions/` or `logs/`)
2. Filter: `events.jsonl` (not `*.jsonl`)
3. Session ID format: `copilot:{uuid}` (not `copilot:filename`)
4. Model: from `session.start.data.selectedModel` or `assistant.message.data.selectedModel`
5. Project: from `session.start.data.context.cwd`
6. Parse `type` field for session.start, user.message, assistant.message, tool_call

**Implementation:**

```rust
use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;
use std::fs;

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

    fn parse_session_id(path: &PathBuf) -> String {
        // path is ~/.copilot/session-state/{uuid}/events.jsonl
        // UUID is the directory name
        path.iter()
            .skip_while(|p| p.to_string_lossy() != "session-state")
            .nth(1)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn read_lines(path: &PathBuf, from_end: bool, count: usize) -> Vec<String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let all_lines: Vec<&str> = content.trim().split('\n').collect();
        if from_end {
            all_lines.into_iter().rev().take(count).collect::<Vec<_>>().into_iter().rev().map(String::from).collect()
        } else {
            all_lines.into_iter().take(count).map(String::from).collect()
        }
    }

    fn parse_jsonl_entry(line: &str) -> Option<serde_json::Value> {
        serde_json::from_str(line).ok()
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
        let dir = Self::session_state_dir();
        if dir.exists() {
            vec![WatchPath {
                path: dir,
                watch_type: WatchType::Directory,
                filter: Some("events.jsonl".to_string()),
                recursive: true,
            }]
        } else {
            Vec::new()
        }
    }

    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession> {
        let dir = Self::session_state_dir();
        if !dir.exists() {
            return Vec::new();
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - threshold_ms as i64;
        let mut sessions = Vec::new();

        // Read each session directory
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let session_dir = entry.path();
                if !session_dir.is_dir() {
                    continue;
                }
                let events_file = session_dir.join("events.jsonl");
                if !events_file.exists() {
                    continue;
                }

                // Check file mtime
                if let Ok(stat) = fs::metadata(&events_file) {
                    let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                    let mtime_ms = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;
                    if mtime_ms < threshold {
                        continue;
                    }

                    let session_id = Self::parse_session_id(&events_file);
                    if session_id.is_empty() {
                        continue;
                    }

                    // Parse the session file for detail
                    let lines = Self::read_lines(&events_file, true, 80);
                    let mut model: Option<String> = None;
                    let mut project: Option<String> = None;
                    let mut last_tool: Option<String> = None;
                    let mut last_tool_input: Option<String> = None;
                    let mut last_message: Option<String> = None;

                    // First pass: get model and project from session.start
                    for line in &lines {
                        if let Some(json) = Self::parse_jsonl_entry(line) {
                            if json.get("type").and_then(|v| v.as_str()) == Some("session.start") {
                                if let Some(data) = json.get("data").as_object() {
                                    if model.is_none() {
                                        model = data.get("selectedModel").and_then(|v| v.as_str()).map(String::from);
                                    }
                                    if project.is_none() {
                                        if let Some(ctx) = data.get("context").as_object() {
                                            project = ctx.get("cwd").and_then(|v| v.as_str()).map(String::from);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Second pass (from end): get last tool and message
                    for line in lines.iter().rev() {
                        if let Some(json) = Self::parse_jsonl_entry(line) {
                            let entry_type = json.get("type").and_then(|v| v.as_str());

                            if entry_type == Some("assistant.message") {
                                if let Some(data) = json.get("data").as_object() {
                                    if model.is_none() {
                                        model = data.get("selectedModel").and_then(|v| v.as_str()).map(String::from);
                                    }
                                    if let Some(content) = data.get("content").as_array() {
                                        for block in content {
                                            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                                if last_message.is_none() {
                                                    last_message = block.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
                                                }
                                            }
                                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                                if last_tool.is_none() {
                                                    last_tool = block.get("name").and_then(|v| v.as_str()).map(String::from);
                                                    if let Some(input) = block.get("input").as_object() {
                                                        let s = serde_json::to_string(input).unwrap_or_default();
                                                        last_tool_input = Some(s.chars().take(60).collect());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if entry_type == Some("tool_call") {
                                if last_tool.is_none() {
                                    if let Some(data) = json.get("data").as_object() {
                                        last_tool = data.get("name").and_then(|v| v.as_str()).map(String::from);
                                        if let Some(input) = data.get("input") {
                                            let s = serde_json::to_string(&input).unwrap_or_default();
                                            last_tool_input = Some(s.chars().take(60).collect());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    sessions.push(ActiveSession {
                        session_id: format!("copilot:{}", session_id),
                        provider: "copilot".to_string(),
                        agent_id: None,
                        agent_type: "main".to_string(),
                        model: model.unwrap_or_else(|| "unknown".to_string()),
                        status: "active".to_string(),
                        last_activity: mtime_ms,
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
        let uuid = session_id.strip_prefix("copilot:")?;
        let events_file = Self::session_state_dir().join(uuid).join("events.jsonl");
        if !events_file.exists() {
            return None;
        }

        let lines = Self::read_lines(&events_file, true, 80);
        let mut model: Option<String> = None;
        let mut project: Option<String> = None;
        let mut last_tool: Option<String> = None;
        let mut last_tool_input: Option<String> = None;
        let mut last_message: Option<String> = None;

        for line in lines.iter().rev() {
            if let Some(json) = Self::parse_jsonl_entry(line) {
                let entry_type = json.get("type").and_then(|v| v.as_str());

                if entry_type == Some("session.start") {
                    if let Some(data) = json.get("data").as_object() {
                        if model.is_none() {
                            model = data.get("selectedModel").and_then(|v| v.as_str()).map(String::from);
                        }
                        if project.is_none() {
                            if let Some(ctx) = data.get("context").as_object() {
                                project = ctx.get("cwd").and_then(|v| v.as_str()).map(String::from);
                            }
                        }
                    }
                }

                if entry_type == Some("assistant.message") {
                    if let Some(data) = json.get("data").as_object() {
                        if model.is_none() {
                            model = data.get("selectedModel").and_then(|v| v.as_str()).map(String::from);
                        }
                        if let Some(content) = data.get("content").as_array() {
                            for block in content {
                                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                                    if last_message.is_none() {
                                        last_message = block.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }

                if entry_type == Some("tool_call") {
                    if last_tool.is_none() {
                        if let Some(data) = json.get("data").as_object() {
                            last_tool = data.get("name").and_then(|v| v.as_str()).map(String::from);
                            if let Some(input) = data.get("input") {
                                let s = serde_json::to_string(&input).unwrap_or_default();
                                last_tool_input = Some(s.chars().take(60).collect());
                            }
                        }
                    }
                }
            }
        }

        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "copilot".to_string(),
            agent_id: None,
            agent_type: "main".to_string(),
            model: model.unwrap_or_else(|| "unknown".to_string()),
            status: "active".to_string(),
            last_activity: 0,
            project,
            last_message,
            last_tool,
            last_tool_input,
            parent_session_id: None,
        })
    }
}
```

---

## Task 2: Rewrite Gemini Adapter

**Files:**
- Modify: `src/session_collector/src/adapters/gemini.rs`

**Actual gemini session format** (from claude-ville):
```
~/.gemini/tmp/{project_hash}/chats/session-{sessionId}.json

JSON structure:
{
  "sessionId": "...",
  "projectHash": "...",  // SHA-256 of cwd
  "messages": [
    {"type": "user", "content": "Hello", "timestamp": "..."},
    {"type": "gemini", "content": "Hi!", "model": "gemini-2.5-flash", "toolCalls": [{"name": "...", "args": {...}}], "timestamp": "..."},
    {"type": "tool_call", "name": "...", "input": {...}, "timestamp": "..."}
  ]
}
```

**Key changes:**
1. Watch `~/.gemini/tmp/{project_hash}/chats/` directories (not `logs/`)
2. Filter: `session-*.json` (not `*.jsonl`)
3. Project hash resolution via SHA-256 reverse mapping
4. Parse `type` field: user, gemini, tool_call
5. Model from `gemini` type message's `model` field
6. Session ID format: `gemini:{sessionId}`

---

## Task 3: Rewrite Claude Adapter

**Files:**
- Modify: `src/session_collector/src/adapters/claude.rs`

**Actual claude session format** (from claude-ville):
- Primary: `~/.claude/history.jsonl` — indexed session metadata with project path
- Session files: `~/.claude/projects/{encoded}/sessions/` for main sessions
- Subagents: `~/.claude/projects/{encoded}/{session}/subagents/agent-{id}.jsonl`
- Teams: `~/.claude/teams/`

**Key changes:**
1. Add `history.jsonl` scanning as primary data source
2. Parse `entry.project`, `entry.sessionId`, `entry.agentId`, `entry.model`, `entry.timestamp`, `entry.display`
3. Subagent detection via `subagents/` directory
4. Project path encoding: `/` -> `-` (so `/Users/me/project` becomes `-Users-me-project`)

---

## Task 4: Rewrite OpenClaw Adapter

**Files:**
- Modify: `src/session_collector/src/adapters/openclaw.rs`

**Actual openclaw session format** (from claude-ville):
```
~/.openclaw/agents/{agentId}/sessions/{sessionId}.jsonl

Entry format:
{"type":"session","version":3,"id":"...","timestamp":"...","cwd":"..."}
{"type":"model_change","provider":"github-copilot","modelId":"gpt-5-mini",...}
{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"..."}],"model":"...","usage":{...}},...}
```

**Key changes:**
1. Watch path: `~/.openclaw/agents/{agentId}/sessions/` (not `logs/` or `projects/`)
2. Parse `type` field: session, model_change, message
3. Session ID format: `openclaw:{agentId}:{sessionId}`
4. Model from `message.model` or `model_change.modelId`
5. Project from `session.cwd`

---

## Task 5: Rewrite Codex Adapter

**Files:**
- Modify: `src/session_collector/src/adapters/codex.rs`

**Actual codex session format** (from claude-ville):
```
~/.codex/sessions/YYYY/MM/DD/rollout-{timestamp}.jsonl

Entry format:
{"type":"session_meta","payload":{"id":"...","cwd":"/path","cli_version":"..."}}
{"type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"ls"}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[...]}}
{"type":"event_msg","payload":{"type":"turn_complete","usage":{...}}}
```

**Key changes:**
1. Watch path: `~/.codex/sessions/` (not `logs/`)
2. Recursive scanning with YYYY/MM/DD date structure
3. Parse `type` field: session_meta, response_item, event_msg
4. Session ID format: `codex:{timestamp}` (from filename)
5. Model from `session_meta.payload.model` or `turn_context.payload.model`
6. Filter: `rollout-*.jsonl` (not `*.jsonl`)

---

## Testing

For each adapter, verify:
1. `cargo check --package session_collector` passes
2. Adapter correctly identifies active sessions on your machine
3. Session detail parsing works for real session files

Run after each adapter fix:
```bash
cargo check --package session_collector 2>&1 | tail -10
```