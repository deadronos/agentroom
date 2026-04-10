# Session Monitoring Implementation Plan (Split Architecture)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a distributed session monitoring system with collector/hub/frontend split architecture: collectors run on each machine watching session files, hub receives snapshots and broadcasts merged state, frontend streams events to clients via WebSocket.

**Architecture:** Three binaries: `session-collector` (daemon per machine), `session-hub` (central service), sharing `session_common` crate. Collectors use `notify` crate for filesystem watching, publish snapshots via WebSocket every 2s (or on dirty flag). Hub merges snapshots with latest-wins per sessionId, broadcasts to frontends. Bearer token auth between collector↔hub.

**Tech Stack:** Rust with tokio async runtime, tokio-tungstenite for WebSocket, notify crate for filesystem watching, sha1 for fingerprinting, serde/serde_json for serialization.

---

## File Structure

```
src/
├── session_common/                    # Shared types (no dependencies)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs                   # Snapshot, ActiveSession, SessionEvent
│       └── adapter.rs                 # SessionAdapter trait
├── session_collector/                # Collector binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── collector.rs              # Snapshot building + deduplication
│       ├── client.rs                 # WebSocket client to hub
│       ├── watcher.rs                # notify-based filesystem watcher
│       └── adapters/
│           ├── mod.rs
│           ├── claude.rs             # Claude Code adapter
│           ├── openclaw.rs
│           ├── copilot.rs
│           ├── codex.rs
│           ├── opencode.rs
│           └── gemini.rs
└── session_hub/                       # Hub binary
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── server.rs                 # WebSocket server (collector + frontend)
        ├── state.rs                  # Session state management + merge
        └── auth.rs                   # Bearer token validation
```

---

## Task 1: Create session_common crate

**Files:**
- Create: `src/session_common/Cargo.toml`
- Create: `src/session_common/src/lib.rs`
- Create: `src/session_common/src/types.rs`
- Create: `src/session_common/src/adapter.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "session_common"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha1 = "0.10"
```

- [ ] **Step 2: Create types.rs**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct Snapshot {
    pub collector_id: String,
    pub timestamp: i64,
    pub fingerprint: String,
    pub sessions: Vec<ActiveSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
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
pub enum HubMessage {
    Ack { fingerprint: String },
    Error { message: String },
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
    StateSync { sessions: Vec<ActiveSession> },
}
```

- [ ] **Step 3: Create adapter.rs**

```rust
use std::path::PathBuf;
use super::types::ActiveSession;

#[derive(Debug, Clone)]
pub enum WatchType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct WatchPath {
    pub path: PathBuf,
    pub watch_type: WatchType,
    pub filter: Option<String>,
    pub recursive: bool,
}

pub trait SessionAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn watch_paths(&self) -> Vec<WatchPath>;
    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession>;
    fn session_detail(&self, session_id: &str) -> Option<super::types::ActiveSession>;
}
```

- [ ] **Step 4: Create lib.rs**

```rust
pub mod types;
pub mod adapter;

pub use types::{ActiveSession, Snapshot, SessionEvent, CollectorMessage, HubMessage};
pub use adapter::{SessionAdapter, WatchPath, WatchType};
```

- [ ] **Step 5: Verify compilation**

Run: `cd src/session_common && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/session_common/
git commit -m "feat(session_common): add shared types and SessionAdapter trait"
```

---

## Task 2: Create session_collector crate scaffolding

**Files:**
- Create: `src/session_collector/Cargo.toml`
- Create: `src/session_collector/src/main.rs` (minimal main)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "session_collector"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "session-collector"
path = "src/main.rs"

[dependencies]
session_common = { path = "../session_common" }
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.21"
futures-util = "0.3"
notify = "6"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha1 = "0.10"
url = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
hostname = "0.4"
```

- [ ] **Step 2: Create minimal main.rs**

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    tracing::info!("session-collector starting...");
    // TODO: implement
    Ok(())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd src/session_collector && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/session_collector/Cargo.toml src/session_collector/src/
git commit -m "feat(session_collector): scaffold collector binary"
```

---

## Task 3: Implement SessionAdapter for claude adapter

**Files:**
- Create: `src/session_collector/src/adapters/claude.rs`
- Modify: `src/session_collector/src/adapters/mod.rs`

- [ ] **Step 1: Create adapters/mod.rs**

```rust
mod claude;
mod openclaw;
mod copilot;
mod codex;
mod opencode;
mod gemini;

pub use claude::ClaudeAdapter;
pub use openclaw::OpenClawAdapter;
pub use copilot::CopilotAdapter;
pub use codex::CodexAdapter;
pub use opencode::OpenCodeAdapter;
pub use gemini::GeminiAdapter;
```

- [ ] **Step 2: Create claude.rs**

```rust
use std::path::PathBuf;
use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use sha1::{Sha1, Digest};

pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn log_dir() -> PathBuf {
        Self::home_dir().join(".claude").join("logs")
    }

    fn projects_dir() -> PathBuf {
        Self::home_dir().join(".claude").join("projects")
    }

    fn parse_session_id(path: &PathBuf) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn parse_jsonl_entry(line: &str) -> Option<(i64, Option<String>, Option<String>)> {
        // Parse JSONL line: extract last_tool, last_message, timestamp
        // Expected format: {"type":"tool","name":"Edit",...} or {"type":"user","text":"..."}
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let ts = json.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tool = json.get("name").and_then(|v| v.as_str()).map(String::from);
        let text = json.get("text").and_then(|v| v.as_str()).map(String::from);
        Some((ts, tool.or(text), tool))
    }

    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts))
    }
}

impl SessionAdapter for ClaudeAdapter {
    fn name(&self) -> &str {
        "claude"
    }

    fn is_available(&self) -> bool {
        Self::log_dir().exists() || Self::projects_dir().exists()
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - threshold_ms as i64;
        let mut sessions = Vec::new();

        for dir in [Self::log_dir(), Self::projects_dir()] {
            if !dir.exists() {
                continue;
            }
            if let Ok(entries) = walkdir(&dir, true) {
                for path in entries {
                    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                        continue;
                    }
                    if let Ok(stat) = std::fs::metadata(&path) {
                        let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                        let mtime_ms = mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                        if mtime_ms < threshold {
                            continue;
                        }
                        let session_id = format!("claude:{}", Self::parse_session_id(&path));
                        let (last_message, last_tool, _) = Self::read_last_jsonl_entry(&path).unwrap_or((None, None, mtime_ms));
                        sessions.push(ActiveSession {
                            session_id,
                            provider: "claude".to_string(),
                            agent_id: None,
                            agent_type: "main".to_string(),
                            model: "unknown".to_string(),
                            status: "active".to_string(),
                            last_activity: mtime_ms,
                            project: path.parent().map(|p| p.to_string_lossy().to_string()),
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
        // Extract path from session_id (format: "claude:<path>")
        let path_str = session_id.strip_prefix("claude:")?;
        let path = PathBuf::from(path_str);
        let (last_message, last_tool, last_activity) = Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0));
        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "claude".to_string(),
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

fn walkdir(dir: &PathBuf, recursive: bool) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if recursive {
        walkdir_recursive(dir, &mut paths, 0, 10)?;
    } else {
        for entry in std::fs::read_dir(dir)? {
            paths.push(entry?.path());
        }
    }
    Ok(paths)
}

fn walkdir_recursive(dir: &PathBuf, paths: &mut Vec<PathBuf>, depth: usize, max_depth: usize) -> std::io::Result<()> {
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
```

**Note:** The `walkdir_recursive` helper is needed because we can't use external crates beyond what we specified. Alternatively, use `notify::RecommendedWatcher` event-driven approach in Task 4.

- [ ] **Step 3: Verify compilation**

Run: `cd src/session_collector && cargo check`
Expected: PASS (need to add `dirs` crate to Cargo.toml)

Add `dirs = "5"` to session_collector Cargo.toml dependencies.

Run: `cargo check` again
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/session_collector/src/adapters/
git commit -m "feat(collector): add claude adapter"
```

---

## Task 4: Implement remaining adapters

**Files:**
- Create: `src/session_collector/src/adapters/openclaw.rs`
- Create: `src/session_collector/src/adapters/copilot.rs`
- Create: `src/session_collector/src/adapters/codex.rs`
- Create: `src/session_collector/src/adapters/opencode.rs`
- Create: `src/session_collector/src/adapters/gemini.rs`

For each adapter, follow the pattern from claude.rs. Each adapter's `watch_paths()` returns paths where its session logs live:

| Adapter | Watch paths |
|---------|-------------|
| openclaw | `~/.openclaw/logs/*.jsonl`, `~/.openclaw/projects/*/sessions/` |
| copilot | `~/.github/copilot/*.jsonl` |
| codex | `~/.codex/logs/*.jsonl` |
| opencode | `~/.opencode/logs/*.jsonl` |
| gemini | `~/.gemini/logs/*.jsonl` |

Each adapter parses its specific JSONL format to extract `last_tool`, `last_message`, `last_activity`.

- [ ] **Implement openclaw.rs** — similar to claude.rs but with openclaw's path structure

- [ ] **Implement copilot.rs** — similar pattern

- [ ] **Implement codex.rs** — similar pattern

- [ ] **Implement opencode.rs** — similar pattern

- [ ] **Implement gemini.rs** — similar pattern

- [ ] **Verify all compile**

Run: `cargo check`
Expected: PASS

- [ ] **Commit**

```bash
git add src/session_collector/src/adapters/
git commit -m "feat(collector): implement remaining adapters (openclaw, copilot, codex, opencode, gemini)"
```

---

## Task 5: Implement collector's watcher (notify-based)

**Files:**
- Modify: `src/session_collector/src/main.rs`
- Create: `src/session_collector/src/watcher.rs`
- Create: `src/session_collector/src/collector.rs`

- [ ] **Step 1: Create watcher.rs**

```rust
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config as NotifyConfig, Event, EventKind};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SessionWatcher {
    watchers: Vec<RecommendedWatcher>,
    dirty: Arc<Mutex<bool>>,
    watch_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl SessionWatcher {
    pub fn new() -> Self {
        Self {
            watchers: Vec::new(),
            dirty: Arc::new(Mutex::new(false)),
            watch_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn watch(&mut self, path: PathBuf, recursive: bool) -> anyhow::Result<()> {
        let dirty = self.dirty.clone();
        let wp = self.watch_paths.clone();
        
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            *dirty.lock().unwrap() = true;
                        }
                        _ => {}
                    }
                }
            },
            NotifyConfig::default().with_poll_interval(Duration::from_secs(1)),
        )?;
        
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        watcher.watch(&path, mode)?;
        
        self.watchers.push(watcher);
        self.watch_paths.lock().unwrap().insert(path);
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        *self.dirty.lock().unwrap()
    }

    pub fn clear_dirty(&self) {
        *self.dirty.lock().unwrap() = false;
    }
}
```

- [ ] **Step 2: Create collector.rs**

```rust
use crate::watcher::SessionWatcher;
use crate::client::HubClient;
use session_common::{ActiveSession, Snapshot, SessionAdapter};
use sha1::{Sha1, Digest};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct Collector {
    adapters: Vec<Box<dyn SessionAdapter + Send + Sync>>,
    watcher: SessionWatcher,
    hub_client: HubClient,
    collector_id: String,
    last_fingerprint: Option<String>,
    last_sessions: Vec<ActiveSession>,
}

impl Collector {
    pub fn new(
        adapters: Vec<Box<dyn SessionAdapter + Send + Sync>>,
        hub_client: HubClient,
        collector_id: String,
    ) -> Self {
        Self {
            adapters,
            watcher: SessionWatcher::new(),
            hub_client,
            collector_id,
            last_fingerprint: None,
            last_sessions: Vec::new(),
        }
    }

    pub fn setup_watchers(&mut self) -> anyhow::Result<()> {
        for adapter in &self.adapters {
            if !adapter.is_available() {
                continue;
            }
            for wp in adapter.watch_paths() {
                self.watcher.watch(wp.path, wp.recursive)?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self, flush_interval_ms: u64) -> anyhow::Result<()> {
        loop {
            tokio::time::sleep(Duration::from_millis(flush_interval_ms)).await;
            self.flush_if_needed().await?;
        }
    }

    pub async fn flush_if_needed(&mut self) -> anyhow::Result<()> {
        let is_dirty = self.watcher.is_dirty();
        if !is_dirty {
            return Ok(());
        }

        let sessions = self.collect_sessions();
        let fingerprint = self.compute_fingerprint(&sessions);

        if fingerprint == self.last_fingerprint {
            self.watcher.clear_dirty();
            return Ok(());
        }

        let snapshot = Snapshot {
            collector_id: self.collector_id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            fingerprint: fingerprint.clone(),
            sessions: sessions.clone(),
        };

        self.hub_client.send_snapshot(snapshot).await?;
        self.last_fingerprint = Some(fingerprint);
        self.last_sessions = sessions;
        self.watcher.clear_dirty();
        Ok(())
    }

    fn collect_sessions(&self) -> Vec<ActiveSession> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - 120000; // ACTIVE_THRESHOLD_MS
        let mut all_sessions = Vec::new();

        for adapter in &self.adapters {
            if !adapter.is_available() {
                continue;
            }
            let sessions = adapter.active_sessions(120000);
            for session in sessions {
                if session.last_activity >= threshold {
                    all_sessions.push(session);
                }
            }
        }
        all_sessions
    }

    fn compute_fingerprint(&self, sessions: &[ActiveSession]) -> String {
        let json = serde_json::to_string(sessions).unwrap_or_default();
        let mut hasher = Sha1::new();
        hasher.update(json.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd src/session_collector && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/session_collector/src/watcher.rs src/session_collector/src/collector.rs
git commit -m "feat(collector): implement watcher and collector core"
```

---

## Task 6: Implement collector's WebSocket client

**Files:**
- Create: `src/session_collector/src/client.rs`

- [ ] **Step 1: Create client.rs**

```rust
use session_common::{CollectorMessage, HubMessage, Snapshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use url::Url;

pub struct HubClient {
    url: String,
    token: String,
}

impl HubClient {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }

    pub async fn connect(&self) -> anyhow::Result<()> {
        let url_with_token = format!("{}?token={}", self.url, self.token);
        let url = Url::parse(&url_with_token)?;
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();
        
        tracing::info!("Connected to hub");
        
        // Read acknowledgments in background
        let mut read = read;
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(hub_msg) = serde_json::from_str::<HubMessage>(&text) {
                        match hub_msg {
                            HubMessage::Ack { fingerprint } => {
                                tracing::debug!("Hub acknowledged snapshot {}", fingerprint);
                            }
                            HubMessage::Error { message } => {
                                tracing::error!("Hub error: {}", message);
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn send_snapshot(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        // Note: This is a simplified version. In practice, you'd want to store
        // the write half and use it here. For now, reconnect per snapshot.
        let url_with_token = format!("{}?token={}", self.url, self.token);
        let url = Url::parse(&url_with_token)?;
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, _read) = ws_stream.split();
        
        let msg = CollectorMessage::Snapshot {
            collector_id: snapshot.collector_id,
            timestamp: snapshot.timestamp,
            fingerprint: snapshot.fingerprint,
            sessions: snapshot.sessions,
        };
        
        let text = serde_json::to_string(&msg)?;
        write.send(Message::Text(text)).await?;
        write.close().await?;
        
        Ok(())
    }
}
```

**Note:** This is a simplified client. The actual implementation should maintain a persistent connection. The reconnect logic will be added in Task 8.

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/client.rs
git commit -m "feat(collector): add WebSocket client to hub"
```

---

## Task 7: Implement session_hub crate

**Files:**
- Create: `src/session_hub/Cargo.toml`
- Create: `src/session_hub/src/main.rs`
- Create: `src/session_hub/src/server.rs`
- Create: `src/session_hub/src/state.rs`
- Create: `src/session_hub/src/auth.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "session_hub"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "session-hub"
path = "src/main.rs"

[dependencies]
session_common = { path = "../session_common" }
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.21"
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

- [ ] **Step 2: Create auth.rs**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Auth {
    token: String,
    connected_collectors: Arc<RwLock<HashMap<String, std::time::Instant>>>,
}

impl Auth {
    pub fn new(token: String) -> Self {
        Self {
            token,
            connected_collectors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn validate_token(&self, token: &str) -> bool {
        self.token == token
    }

    pub async fn register_collector(&self, collector_id: String) {
        self.connected_collectors.write().await.insert(
            collector_id,
            std::time::Instant::now(),
        );
    }

    pub async fn heartbeat_collector(&self, collector_id: &str) {
        self.connected_collectors.write().await.insert(
            collector_id.to_string(),
            std::time::Instant::now(),
        );
    }

    pub async fn cleanup_stale_collectors(&self, timeout_secs: u64) -> Vec<String> {
        let now = std::time::Instant::now();
        let mut stale = Vec::new();
        let mut write = self.connected_collectors.write().await;
        
        write.retain(|id, last_seen| {
            if now.duration_since(*last_seen).as_secs() > timeout_secs {
                stale.push(id.clone());
                false
            } else {
                true
            }
        });
        stale
    }
}
```

- [ ] **Step 3: Create state.rs**

```rust
use session_common::ActiveSession;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct HubState {
    // collector_id -> snapshot
    collector_snapshots: Arc<RwLock<HashMap<String, session_common::Snapshot>>>,
    // session_id -> merged session
    merged_sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            collector_snapshots: Arc::new(RwLock::new(HashMap::new())),
            merged_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn apply_snapshot(&self, snapshot: session_common::Snapshot) -> SessionDiff {
        let mut old_session_ids = self.merged_sessions.read().await.keys().cloned().collect::<HashSet<_>>();
        
        // Update collector snapshot
        self.collector_snapshots.write().await.insert(
            snapshot.collector_id.clone(),
            snapshot.clone(),
        );
        
        // Merge sessions: latest-wins per sessionId
        let mut write = self.merged_sessions.write().await;
        for session in snapshot.sessions {
            let should_update = match write.get(&session.session_id) {
                Some(existing) => session.last_activity > existing.last_activity,
                None => true,
            };
            if should_update {
                write.insert(session.session_id.clone(), session);
            }
        }
        
        let new_session_ids = write.keys().cloned().collect::<HashSet<_>>();
        
        let started: Vec<_> = new_session_ids.difference(&old_session_ids).cloned().collect();
        let ended: Vec<_> = old_session_ids.difference(&new_session_ids).cloned().collect();
        let existing: Vec<_> = old_session_ids.intersection(&new_session_ids).cloned().collect();
        
        SessionDiff { started, ended, existing }
    }

    pub async fn get_all_sessions(&self) -> Vec<ActiveSession> {
        self.merged_sessions.read().await.values().cloned().collect()
    }

    pub async fn remove_collector(&self, collector_id: &str) {
        // Remove collector's snapshots
        if let Some(snapshot) = self.collector_snapshots.write().await.remove(collector_id) {
            // Remove sessions only from this collector
            let mut write = self.merged_sessions.write().await;
            let session_ids: Vec<_> = write.keys()
                .filter(|id| {
                    // Check if this session came from the removed collector
                    // This requires tracking source, simplified here
                    false
                })
                .cloned()
                .collect();
            for id in session_ids {
                write.remove(&id);
            }
        }
    }
}

pub struct SessionDiff {
    pub started: Vec<String>,
    pub ended: Vec<String>,
    pub existing: Vec<String>,
}
```

- [ ] **Step 4: Create server.rs**

```rust
use crate::auth::Auth;
use crate::state::HubState;
use session_common::{CollectorMessage, HubMessage, ActiveSession};
use tokio_tungstenite::{accept_async, tungstenite::{Message, Error}};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct HubServer {
    state: HubState,
    auth: Auth,
    collector_port: u16,
    frontend_port: u16,
}

impl HubServer {
    pub fn new(
        auth_token: String,
        collector_port: u16,
        frontend_port: u16,
    ) -> Self {
        Self {
            state: HubState::new(),
            auth: Auth::new(auth_token),
            collector_port,
            frontend_port,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let collector_addr = format!("0.0.0.0:{}", self.collector_port);
        let collector_listener = TcpListener::bind(&collector_addr).await?;
        tracing::info!("Collector WebSocket server listening on {}", collector_addr);
        
        let frontend_addr = format!("0.0.0.0:{}", self.frontend_port);
        let frontend_listener = TcpListener::bind(&frontend_addr).await?;
        tracing::info!("Frontend WebSocket server listening on {}", frontend_addr);
        
        let state = self.state.clone();
        let auth = self.auth.clone();
        
        // Collector connections handler
        let collector_state = self.state.clone();
        let collector_auth = self.auth.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, addr)) = collector_listener.accept().await {
                    let state = collector_state.clone();
                    let auth = collector_auth.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_collector_connection(stream, addr, state, auth).await {
                            tracing::warn!("Collector connection error: {}", e);
                        }
                    });
                }
            }
        });
        
        // Frontend connections handler
        let frontend_state = self.state.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, addr)) = frontend_listener.accept().await {
                    let state = frontend_state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_frontend_connection(stream, addr, state).await {
                            tracing::warn!("Frontend connection error: {}", e);
                        }
                    });
                }
            }
        });
        
        // Cleanup task for stale collectors
        let cleanup_state = self.state.clone();
        let cleanup_auth = self.auth.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let stale = cleanup_auth.cleanup_stale_collectors(60).await;
                for collector_id in stale {
                    cleanup_state.remove_collector(&collector_id).await;
                    tracing::info!("Removed stale collector: {}", collector_id);
                }
            }
        });
        
        // Keep main future alive
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

async fn handle_collector_connection(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    state: HubState,
    auth: Auth,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // Handle incoming messages
    while let Some(msg) = read.next().await {
        let msg = msg?;
        
        match msg {
            Message::Text(text) => {
                if let Ok(collector_msg) = serde_json::from_str::<CollectorMessage>(&text) {
                    match collector_msg {
                        CollectorMessage::Snapshot { collector_id, timestamp, fingerprint, sessions } => {
                            let snapshot = session_common::Snapshot {
                                collector_id: collector_id.clone(),
                                timestamp,
                                fingerprint,
                                sessions,
                            };
                            
                            let diff = state.apply_snapshot(snapshot).await;
                            let sessions = state.get_all_sessions().await;
                            
                            // Send ack
                            let ack = HubMessage::Ack { fingerprint: snapshot.fingerprint };
                            write.send(Message::Text(serde_json::to_string(&ack)?)).await?;
                            
                            // Broadcast to all frontends (TODO: track frontend connections)
                            tracing::debug!("Applied snapshot from {}: {} started, {} ended", 
                                collector_id, diff.started.len(), diff.ended.len());
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    
    Ok(())
}

async fn handle_frontend_connection(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    state: HubState,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, _read) = ws_stream.split();
    
    // Send initial state sync
    let sessions = state.get_all_sessions().await;
    let sync = HubMessage::StateSync { sessions };
    write.send(Message::Text(serde_json::to_string(&sync)?)).await?;
    
    // Keep connection alive (frontends don't send messages, just receive)
    // In a real implementation, you'd want to track these connections for broadcasting
    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    
    Ok(())
}
```

- [ ] **Step 5: Create main.rs**

```rust
mod auth;
mod server;
mod state;

use server::HubServer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let auth_token = std::env::var("HUB_AUTH_TOKEN")
        .expect("HUB_AUTH_TOKEN must be set");
    let collector_port: u16 = std::env::var("HUB_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("HUB_PORT must be a valid port");
    let frontend_port: u16 = std::env::var("HUB_FRONTEND_PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse()
        .expect("HUB_FRONTEND_PORT must be a valid port");

    let server = HubServer::new(auth_token, collector_port, frontend_port);
    server.run().await?;

    Ok(())
}
```

- [ ] **Step 6: Verify compilation**

Run: `cd src/session_hub && cargo check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/session_hub/
git commit -m "feat(session_hub): add hub service with WebSocket server and state management"
```

---

## Task 8: Wire up collector main.rs with full flow

**Files:**
- Modify: `src/session_collector/src/main.rs`

- [ ] **Step 1: Update main.rs**

```rust
mod adapters;
mod client;
mod collector;
mod watcher;

use adapters::{ClaudeAdapter, OpenClawAdapter, CopilotAdapter, CodexAdapter, OpenCodeAdapter, GeminiAdapter};
use client::HubClient;
use collector::Collector;
use session_common::SessionAdapter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let hub_url = std::env::var("HUB_URL")
        .unwrap_or_else(|_| "ws://localhost:8080".to_string());
    let auth_token = std::env::var("HUB_AUTH_TOKEN")
        .expect("HUB_AUTH_TOKEN must be set");
    let collector_id = std::env::var("COLLECTOR_ID")
        .unwrap_or_else(|| hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()));
    let flush_interval_ms: u64 = std::env::var("FLUSH_INTERVAL_MS")
        .unwrap_or_else(|_| "2000".to_string())
        .parse()
        .unwrap_or(2000);

    tracing::info!("Starting session-collector {} -> {}", collector_id, hub_url);

    // Build adapters
    let adapters: Vec<Box<dyn SessionAdapter + Send + Sync>> = vec![
        Box::new(ClaudeAdapter::new()),
        Box::new(OpenClawAdapter::new()),
        Box::new(CopilotAdapter::new()),
        Box::new(CodexAdapter::new()),
        Box::new(OpenCodeAdapter::new()),
        Box::new(GeminiAdapter::new()),
    ];

    let hub_client = HubClient::new(hub_url, auth_token);
    let mut collector = Collector::new(adapters, hub_client, collector_id);
    
    collector.setup_watchers()?;
    collector.run(flush_interval_ms).await?;

    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src/session_collector && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/main.rs
git commit -m "feat(collector): wire up main loop with adapters and hub client"
```

---

## Task 9: Add reconnection logic to collector client

**Files:**
- Modify: `src/session_collector/src/client.rs`

The current client implementation reconnects per snapshot. Add proper reconnection with exponential backoff.

- [ ] **Step 1: Implement persistent connection with retry**

Replace the HubClient with one that maintains a persistent connection and handles reconnection:

```rust
use session_common::{CollectorMessage, HubMessage, Snapshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use url::Url;
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct HubClient {
    url: String,
    token: String,
    state: Arc<RwLock<HubClientState>>,
}

struct HubClientState {
    connected: bool,
    retry_count: u32,
    max_retries: u32,
}

impl HubClient {
    pub fn new(url: String, token: String) -> Self {
        Self {
            url,
            token,
            state: Arc::new(RwLock::new(HubClientState {
                connected: false,
                retry_count: 0,
                max_retries: 10,
            })),
        }
    }

    pub async fn connect(&self) -> anyhow::Result<()> {
        loop {
            let url_with_token = format!("{}?token={}", self.url, self.token);
            match connect_async(Url::parse(&url_with_token)?).await {
                Ok((ws_stream, _)) => {
                    let (mut write, mut read) = ws_stream.split();
                    {
                        let mut state = self.state.write().await;
                        state.connected = true;
                        state.retry_count = 0;
                    }
                    tracing::info!("Connected to hub");
                    
                    // Handle incoming messages
                    let mut read = read;
                    tokio::spawn(async move {
                        while let Some(msg) = read.next().await {
                            if let Ok(Message::Text(text)) = msg {
                                if let Ok(hub_msg) = serde_json::from_str::<HubMessage>(&text) {
                                    match hub_msg {
                                        HubMessage::Ack { .. } => {}
                                        HubMessage::Error { message } => {
                                            tracing::error!("Hub error: {}", message);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    });
                    
                    return Ok(());
                }
                Err(e) => {
                    let retry_count = {
                        let mut state = self.state.write().await;
                        state.retry_count += 1;
                        state.retry_count
                    };
                    
                    let backoff = Duration::from_secs(2u64.pow(retry_count.min(5))).min(Duration::from_secs(30));
                    tracing::warn!("Failed to connect to hub (attempt {}), retrying in {:?}: {}", 
                        retry_count, backoff, e);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    pub async fn send_snapshot(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        let url_with_token = format!("{}?token={}", self.url, self.token);
        let (ws_stream, _) = connect_async(Url::parse(&url_with_token)?).await?;
        let (mut write, _read) = ws_stream.split();
        
        let msg = CollectorMessage::Snapshot {
            collector_id: snapshot.collector_id,
            timestamp: snapshot.timestamp,
            fingerprint: snapshot.fingerprint,
            sessions: snapshot.sessions,
        };
        
        let text = serde_json::to_string(&msg)?;
        write.send(Message::Text(text)).await?;
        write.close().await?;
        
        Ok(())
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/client.rs
git commit -m "feat(collector): add reconnection logic with exponential backoff"
```

---

## Task 10: Integration testing

**Files:**
- Create: `src/session_hub/tests/integration.rs`
- Create: `src/session_collector/tests/integration.rs`

- [ ] **Step 1: Write hub integration test**

```rust
use session_hub::{HubServer, HubState};
use session_common::{Snapshot, ActiveSession};

#[tokio::test]
async fn test_snapshot_merge() {
    let state = HubState::new();
    
    let snapshot1 = Snapshot {
        collector_id: "machine-1".to_string(),
        timestamp: 1000,
        fingerprint: "abc".to_string(),
        sessions: vec![
            ActiveSession {
                session_id: "s1".to_string(),
                provider: "claude".to_string(),
                agent_id: None,
                agent_type: "main".to_string(),
                model: "opus".to_string(),
                status: "active".to_string(),
                last_activity: 1000,
                project: None,
                last_message: None,
                last_tool: None,
                last_tool_input: None,
                parent_session_id: None,
            },
        ],
    };
    
    state.apply_snapshot(snapshot1).await;
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    
    // Snapshot from another collector with same session (newer activity wins)
    let snapshot2 = Snapshot {
        collector_id: "machine-2".to_string(),
        timestamp: 2000,
        fingerprint: "def".to_string(),
        sessions: vec![
            ActiveSession {
                session_id: "s1".to_string(),
                provider: "claude".to_string(),
                agent_id: None,
                agent_type: "main".to_string(),
                model: "opus".to_string(),
                status: "active".to_string(),
                last_activity: 2000, // newer
                project: None,
                last_message: Some("newer".to_string()),
                last_tool: None,
                last_tool_input: None,
                parent_session_id: None,
            },
        ],
    };
    
    state.apply_snapshot(snapshot2).await;
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].last_activity, 2000);
    assert_eq!(sessions[0].last_message.as_deref(), Some("newer"));
}

#[tokio::test]
async fn test_latest_wins() {
    let state = HubState::new();
    
    let s1 = ActiveSession {
        session_id: "s1".to_string(),
        provider: "claude".to_string(),
        agent_id: None,
        agent_type: "main".to_string(),
        model: "opus".to_string(),
        status: "active".to_string(),
        last_activity: 1000,
        project: None,
        last_message: None,
        last_tool: None,
        last_tool_input: None,
        parent_session_id: None,
    };
    
    state.apply_snapshot(Snapshot {
        collector_id: "c1".to_string(),
        timestamp: 1000,
        fingerprint: "f1".to_string(),
        sessions: vec![s1],
    }).await;
    
    // Same session from different collector, older timestamp
    let s1_old = ActiveSession {
        session_id: "s1".to_string(),
        provider: "claude".to_string(),
        agent_id: None,
        agent_type: "main".to_string(),
        model: "opus".to_string(),
        status: "active".to_string(),
        last_activity: 500, // older
        project: None,
        last_message: Some("older".to_string()),
        last_tool: None,
        last_tool_input: None,
        parent_session_id: None,
    };
    
    state.apply_snapshot(Snapshot {
        collector_id: "c2".to_string(),
        timestamp: 2000,
        fingerprint: "f2".to_string(),
        sessions: vec![s1_old],
    }).await;
    
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].last_activity, 1000); // newer one wins
}
```

- [ ] **Step 2: Verify tests pass**

Run: `cd src/session_hub && cargo test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/session_hub/tests/
git commit -m "test(hub): add integration tests for snapshot merge and latest-wins"
```

---

## Verification Checklist

After all tasks:
- `cargo check` in session_common — PASS
- `cargo check` in session_collector — PASS
- `cargo check` in session_hub — PASS
- `cargo test` in session_hub — PASS
- Manual test:
  1. `session-hub` (with `HUB_AUTH_TOKEN=secret`)
  2. `session-collector` (with `HUB_AUTH_TOKEN=secret`)
  3. Connect to `ws://localhost:8081/sessions` with WebSocket client
  4. Verify session events received

---

## Open Issues

- Frontend broadcast: Hub needs to track frontend connections and broadcast state changes. Currently frontends get initial StateSync but not updates.
- Auth for frontend connections: Currently open, may want token auth.
- Collector heartbeat: Hub should expect periodic pings from collectors to detect disconnection.
