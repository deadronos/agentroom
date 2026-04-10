# Session Monitoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement full live session monitoring — a `cass session-daemon` WebSocket server that streams real-time session events by combining filesystem watch (notify crate) + 5s polling fallback, driven by a new `SessionMonitor` trait in `franken_agent_detection`.

**Architecture:** Three-phase build: (1) new `SessionMonitor` trait extending `Connector` in franken_agent_detection with implementations for all connectors, (2) new `session_monitor` module in cass bridging franken_agent_detection to a tokio-tungstenite WebSocket server, (3) integration tying filesystem watch events + polling diffing to JSONL event emission.

**Tech Stack:** Rust (tokio async runtime already in cass), tokio-tungstenite for WebSocket server, notify crate (already in cass) for filesystem watching, asupersync runtime (already in cass).

---

## File Structure

```
search-backend/
  franken_agent_detection/src/
    connectors/
      mod.rs                      # extend Connector → add SessionMonitor impls
      openclaw.rs                 # add SessionMonitor impl
      copilot.rs                  # add SessionMonitor impl
      copilot_cli.rs             # add SessionMonitor impl
      claude_code.rs             # add SessionMonitor impl
      aider.rs, amp.rs, ...       # add SessionMonitor impl (all 15)
    session_monitor.rs            # NEW: trait + types (ActiveSession, SessionDetail, WatchPath)

search-backend/cass/src/
  session_daemon/
    mod.rs                       # NEW: top-level module
    watcher.rs                   # NEW: SessionWatcher combining notify + polling
    events.rs                    # NEW: event types + JSONL serialization
    server.rs                    # NEW: WebSocket server logic
  lib.rs                         # add SessionDaemon command variant
  main.rs                        # add "session-daemon" subcommand dispatch
```

---

## Task 1: SessionMonitor Trait and Types

**Files:**
- Create: `search-backend/franken_agent_detection/src/connectors/session_monitor.rs`
- Modify: `search-backend/franken_agent_detection/src/connectors/mod.rs` (export new trait)
- Modify: `search-backend/franken_agent_detection/src/lib.rs` (re-export)

- [ ] **Step 1: Write the failing test**

Create `franken_agent_detection/src/connectors/session_monitor.rs` with the trait and all types. The trait has 4 methods: `is_available()`, `active_sessions()`, `session_detail()`, `watch_paths()`. Add `ActiveSession`, `SessionDetail`, `WatchPath`, `WatchType`, `ToolUse`, `Message`, `TokenUsage` types matching the spec exactly.

```rust
use std::path::PathBuf;

pub enum WatchType {
    File,
    Directory,
}

pub struct WatchPath {
    pub path: PathBuf,
    pub watch_type: WatchType,
    pub filter: Option<String>,
    pub recursive: bool,
}

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

pub struct ToolUse {
    pub tool: String,
    pub detail: Option<String>,
    pub ts: i64,
}

pub struct Message {
    pub role: String,
    pub text: String,
    pub ts: i64,
}

pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

pub struct SessionDetail {
    pub tool_history: Vec<ToolUse>,
    pub messages: Vec<Message>,
    pub token_usage: Option<TokenUsage>,
}

pub trait SessionMonitor {
    fn is_available(&self) -> bool;
    fn active_sessions(&self, active_threshold_ms: u64) -> Vec<ActiveSession>;
    fn session_detail(&self, session_id: &str, project: Option<&str>) -> SessionDetail;
    fn watch_paths(&self) -> Vec<WatchPath>;
}
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd search-backend/franken_agent_detection && cargo check --features connectors`

Expected: compilation succeeds (the trait and types are well-formed)

- [ ] **Step 3: Export the trait from connectors/mod.rs**

Add `pub mod session_monitor;` and re-export: `pub use session_monitor::{SessionMonitor, ActiveSession, SessionDetail, WatchPath, WatchType, ToolUse, Message, TokenUsage};`

Run: `cargo check --features connectors` again — expected: PASS

- [ ] **Step 4: Re-export from lib.rs**

Add to the `#[cfg(feature = "connectors")]` block in lib.rs:
```rust
pub use connectors::session_monitor::{SessionMonitor, ActiveSession, SessionDetail, WatchPath, WatchType, ToolUse, Message, TokenUsage};
```

Run: `cargo check --features connectors` — expected: PASS

- [ ] **Step 5: Commit**

```bash
git add search-backend/franken_agent_detection/src/connectors/session_monitor.rs
git add search-backend/franken_agent_detection/src/connectors/mod.rs
git add search-backend/franken_agent_detection/src/lib.rs
git commit -m "feat(frankensearch): add SessionMonitor trait and session types"
```

---

## Task 2: Implement SessionMonitor for openclaw Connector

**Files:**
- Modify: `search-backend/franken_agent_detection/src/connectors/openclaw.rs`
- Test: `search-backend/franken_agent_detection/src/connectors/openclaw.rs` (add tests)

- [ ] **Step 1: Add SessionMonitor impl to openclaw.rs**

Read the full `openclaw.rs` connector to understand its structure (it parses JSONL sessions already). Add:

```rust
use super::session_monitor::{SessionMonitor, ActiveSession, SessionDetail, WatchPath, WatchType, ToolUse, Message};

impl SessionMonitor for OpenClawConnector {
    fn is_available(&self) -> bool {
        Self::agents_root().map(|p| p.exists()).unwrap_or(false)
    }

    fn active_sessions(&self, active_threshold_ms: u64) -> Vec<ActiveSession> {
        let session_dirs = Self::find_agent_session_dirs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut sessions = Vec::new();
        for sessions_dir in session_dirs {
            if let Ok(entries) = std::fs::read_dir(sessions_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                        continue;
                    }
                    if let Ok(stat) = std::fs::metadata(&path) {
                        let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
                        let mtime_ms = mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                        if now - mtime_ms > active_threshold_ms as i64 {
                            continue;
                        }
                        let detail = self.parse_session_detail(&path);
                        let agent_id = sessions_dir.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string());
                        let session_id = format!("openclaw:{}:{}", agent_id.clone().unwrap_or_default(), path.file_stem().unwrap_or_default().to_string_lossy());
                        sessions.push(ActiveSession {
                            session_id,
                            provider: "openclaw".to_string(),
                            agent_id,
                            agent_type: "main".to_string(),
                            model: detail.0,
                            status: "active".to_string(),
                            last_activity: mtime_ms,
                            project: detail.1,
                            last_message: detail.2,
                            last_tool: detail.3,
                            last_tool_input: detail.4,
                            parent_session_id: None,
                        });
                    }
                }
            }
        }
        sessions
    }

    fn session_detail(&self, session_id: &str, _project: Option<&str>) -> SessionDetail {
        // parse session_id → agent_id and file stem, read file, return tool_history + messages
        let parts: Vec<&str> = session_id.split(':').collect();
        if parts.len() < 3 { return SessionDetail { tool_history: vec![], messages: vec![], token_usage: None }; }
        let agent_id = parts.get(1).unwrap_or(&"");
        let file_id = parts.get(2).unwrap_or(&"");
        let agents_root = Self::agents_root().unwrap_or_default();
        let session_path = agents_root.join(agent_id).join("sessions").join(format!("{}.jsonl", file_id));
        self.parse_session_detail_full(&session_path)
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        Self::find_agent_session_dirs()
            .into_iter()
            .map(|p| WatchPath { path: p, watch_type: WatchType::Directory, filter: Some(".jsonl".to_string()), recursive: false })
            .collect()
    }
}

// Helper methods on OpenClawConnector:
fn parse_session_detail(&self, path: &Path) -> (String, Option<String>, Option<String>, Option<String>, Option<String>) {
    // Read last 80 lines of JSONL file, parse backwards to extract model, project, last_message, last_tool, last_tool_input
    // Returns (model, project, last_message, last_tool, last_tool_input)
}

fn parse_session_detail_full(&self, path: &Path) -> SessionDetail {
    // Read full file, extract all tool_use blocks → tool_history, recent text messages
}
```

**Important:** Implement the helper methods by reading the existing openclaw.rs connector's parsing logic and adapting it. The connector already has JSONL reading and parsing code in its `scan()` method — reuse that pattern.

- [ ] **Step 2: Run tests**

Run: `cd search-backend/franken_agent_detection && cargo test --features connectors openclaw` (or whatever test name pattern exists)

Expected: PASS (existing tests still pass + new impl compiles)

- [ ] **Step 3: Add unit test for SessionMonitor**

Add a `#[cfg(test)]` module to openclaw.rs with a test that calls `is_available()`, `active_sessions(60000)`, and `watch_paths()` and asserts the return types.

- [ ] **Step 4: Commit**

```bash
git add search-backend/franken_agent_detection/src/connectors/openclaw.rs
git commit -m "feat(openclaw): implement SessionMonitor for openclaw connector"
```

---

## Task 3: Implement SessionMonitor for remaining connectors

**Files:**
- Modify: each connector file in `search-backend/franken_agent_detection/src/connectors/`

For each connector (copilot, copilot_cli, claude_code, aider, amp, clawdbot, cline, codex, factory, gemini, kimi, pi_agent, qwen, vibe, cursor, goose, hermes, crush, chatgpt, opencode):

- [ ] **Implement SessionMonitor** — follow the same pattern as openclaw but adapted to each connector's session file format and location

Each connector has different session storage:
- `copilot` — VS Code extension state, look for conversation history files
- `copilot_cli` — `~/.github/copilot/` or similar
- `claude_code` — `~/.claude/projects/` JSONL files
- `aider` — `~/.aider/**/` chat history
- etc.

Use existing `scan()` method patterns as reference. For connectors with SQLite session stores (cursor, goose, hermes, crush, opencode), read sessions from the DB.

**Note:** Focus on openclaw, copilot, copilot_cli, and claude_code first since those are the most commonly used. Others can follow the same pattern.

- [ ] **Run tests per connector**

For each: `cargo test --features connectors <connector_name>`

---

## Task 4: cass session-daemon WebSocket server

**Files:**
- Create: `search-backend/cass/src/session_daemon/mod.rs`
- Create: `search-backend/cass/src/session_daemon/events.rs`
- Create: `search-backend/cass/src/session_daemon/watcher.rs`
- Create: `search-backend/cass/src/session_daemon/server.rs`
- Modify: `search-backend/cass/src/lib.rs` — add `SessionDaemon` command
- Modify: `search-backend/cass/src/main.rs` — route `session-daemon` subcommand

### events.rs — Event types + JSONL

- [ ] **Write event types and JSONL serialization**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionStarted {
        session_id: String,
        provider: String,
        project: Option<String>,
        model: String,
        timestamp: i64,
        last_tool: Option<String>,
        last_message: Option<String>,
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

impl SessionEvent {
    /// Serialize to a JSON line (no trailing newline — caller adds it)
    pub fn to_jsonl(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
```

### watcher.rs — SessionWatcher combining notify + polling

- [ ] **Write SessionWatcher combining notify crate + 5s polling**

```rust
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config as NotifyConfig};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct SessionWatcher {
    /// Active session IDs from last poll (for diffing)
    previous_sessions: Arc<Mutex<HashSet<String>>>,
    /// Channel to emit new/changed sessions
    session_tx: mpsc::Sender<SessionEvent>,
    /// Handles for each connector's watcher
    watchers: Vec<RecommendedWatcher>,
}

impl SessionWatcher {
    pub fn new(session_tx: mpsc::Sender<SessionEvent>) -> Self { ... }

    /// Start filesystem watchers for all available connectors
    pub fn start_watchers(&mut self, connectors: &[Box<dyn SessionMonitor + Send>]) { ... }

    /// Poll all connectors and emit diff events
    pub async fn poll_and_diff(&self) { ... }

    /// Run the combined watch + poll loop
    pub async fn run(&mut self, poll_interval: std::time::Duration) { ... }
}
```

Key logic:
1. On startup, do one full `active_sessions()` poll to populate `previous_sessions`
2. Start notify watchers on all `watch_paths()` from each connector
3. Every `poll_interval` (5s), call `active_sessions()` again
4. Diff: sessions in new but not old → `SessionStarted`; sessions in old but not new → `SessionEnded`; sessions in both but changed (e.g., new mtime) → `Activity`
5. On filesystem events from notify, trigger immediate poll for that connector's paths

### server.rs — WebSocket server

- [ ] **Write WebSocket server using tokio-tungstenite**

```rust
use tokio_tungstenite::{accept_async, tungstenite::Message};
use std::net::SocketAddr;

pub async fn run_websocket_server(
    port: u16,
    session_rx: mpsc::Receiver<SessionEvent>,
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Session daemon WebSocket server listening on port {port}");

    loop {
        let (stream, _) = listener.accept().await?;
        let session_rx = session_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, session_rx).await {
                tracing::warn!("WebSocket connection error: {e}");
            }
        });
    }
}

async fn handle_connection(
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut session_rx: mpsc::Receiver<SessionEvent>,
) -> anyhow::Result<()> {
    let (write, _) = tokio_tungstenite::WebSocketStream::split(stream);
    let mut write = tokio::io::BufWriter::new(write);

    loop {
        tokio::select! {
            Some(event) = session_rx.recv() => {
                let line = event.to_jsonl();
                write.write_all(line.as_bytes()).await?;
                write.write_all(b"\n").await?;
                write.flush().await?;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                // Keep-alive ping — send newline
                write.write_all(b"\n").await?;
                write.flush().await?;
            }
        }
    }
}
```

### mod.rs — top-level module

- [ ] **Write session_daemon module**

```rust
pub mod events;
pub mod watcher;
pub mod server;

pub use events::SessionEvent;
pub use watcher::SessionWatcher;
pub use server::run_websocket_server;
```

### lib.rs — add SessionDaemon command

- [ ] **Add `SessionDaemon` variant to `Commands` enum**

Read the Commands enum in lib.rs around line 140, find where `Daemon` is defined (line 835), and add nearby:

```rust
/// Run the live session monitoring daemon (WebSocket server)
SessionDaemon {
    /// WebSocket port to listen on (default: 8080)
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Active session threshold in milliseconds (default: 60000 = 1 minute)
    #[arg(long, default_value_t = 60000)]
    active_threshold_ms: u64,
    /// Poll interval in seconds (default: 5)
    #[arg(long, default_value_t = 5)]
    poll_interval_secs: u64,
},
```

- [ ] **Handle SessionDaemon in run_with_parsed**

Find the match on `cli.command` in lib.rs around line 3605, add a branch:

```rust
Some(Commands::SessionDaemon { port, active_threshold_ms, poll_interval_secs }) => {
    use cass::session_daemon::{run_websocket_server, SessionWatcher};
    use tokio::sync::mpsc;
    use franken_agent_detection::connectors::{get_connector_factories, SessionMonitor};

    let (tx, rx) = mpsc::channel(1000);
    let mut watcher = SessionWatcher::new(tx);

    // Build connector list
    let factories = get_connector_factories();
    let connectors: Vec<_> = factories
        .into_iter()
        .filter_map(|(_, create)| {
            let conn = create();
            if conn.is_available() { Some(conn) } else { None }
        })
        .collect();

    watcher.start_watchers(&connectors);
    watcher.run(tokio::time::Duration::from_secs(poll_interval_secs)).await;
    Ok(())
}
```

Actually this needs more thought — the watcher runs forever, so it needs to spawn the WebSocket server separately. Refactor so `SessionWatcher::run()` spawns the server internally, or have `run_with_parsed` spawn both as separate tasks.

Simplest approach: `SessionWatcher::spawn_server()` that starts the WebSocket server in a background task, and `SessionWatcher::run()` that runs the watch/poll loop. Both spawned via `tokio::spawn`.

- [ ] **Run cargo check**

Run: `cd search-backend/cass && cargo check`

Expected: compilation succeeds with new module

- [ ] **Commit**

```bash
git add search-backend/cass/src/session_daemon/
git add search-backend/cass/src/lib.rs
git add search-backend/cass/src/main.rs
git commit -m "feat(cass): add session-daemon WebSocket command for live session monitoring"
```

---

## Task 5: Integration test

**Files:**
- Create: `search-backend/cass/tests/session_daemon.rs`

- [ ] **Write integration test**

```rust
use std::time::Duration;

#[tokio::test]
async fn session_daemon_smoke_test() {
    // Start daemon in background
    // Connect WebSocket client to port
    // Wait for initial sessions or timeout after 2s
    // Send ping, expect pong
    // Verify JSONL parseable
}
```

- [ ] **Commit**

```bash
git add search-backend/cass/tests/session_daemon.rs
git commit -m "test(cass): add session-daemon integration test"
```

---

## Verification Checklist

After all tasks:
- `cargo check --features connectors` in franken_agent_detection — PASS
- `cargo check` in cass — PASS
- `cargo test --features connectors` in franken_agent_detection — PASS
- `cargo test` in cass — PASS
- Manual test: `cass session-daemon --port 8080` and connect with `wscat -c ws://localhost:8080` or similar