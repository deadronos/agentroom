# Session Monitoring Design

## Overview

Add full live session monitoring to agentroom's cass CLI to match claude-ville functionality. The goal is to run `cass daemon` and connect via WebSocket to receive real-time session activity events.

## Architecture

### Components

1. **`SessionMonitor` trait** - extends `Connector` with live session methods
2. **`cass daemon` command** - WebSocket server exposing session stream
3. **`SessionWatcher` internals** - orchestration combining filesystem watch + polling

### SessionMonitor Trait

```rust
pub trait SessionMonitor {
    /// Returns true if the agent CLI is installed/accessible
    fn is_available(&self) -> bool;

    /// List currently active sessions (modified within active_threshold_ms)
    fn active_sessions(&self, active_threshold_ms: u64) -> Vec<ActiveSession>;

    /// Get detailed info about a specific session
    fn session_detail(&self, session_id: &str, project: Option<&str>) -> SessionDetail;

    /// Return paths to watch for filesystem changes
    fn watch_paths(&self) -> Vec<WatchPath>;
}

pub struct ActiveSession {
    pub session_id: String,
    pub provider: String,       // "claude", "openclaw", etc.
    pub agent_id: Option<String>,
    pub agent_type: String,     // "main", "sub-agent", "team-member"
    pub model: String,
    pub status: String,         // "active"
    pub last_activity: i64,      // Unix timestamp ms
    pub project: Option<String>,
    pub last_message: Option<String>,
    pub last_tool: Option<String>,
    pub last_tool_input: Option<String>,
    pub parent_session_id: Option<String>,
}

pub struct SessionDetail {
    pub tool_history: Vec<ToolUse>,
    pub messages: Vec<Message>,
    pub token_usage: Option<TokenUsage>,
}

pub struct WatchPath {
    pub path: PathBuf,
    pub watch_type: WatchType,  // File or Directory
    pub filter: Option<String>, // e.g., ".jsonl"
    pub recursive: bool,
}
```

### Event Format (JSONL over WebSocket)

Each line is a JSON object:

```json
{"type":"session_started","session_id":"abc-123","provider":"claude","project":"/Users/me/project","model":"claude-opus-4-6","timestamp":1712700000000,"last_tool":"Edit","last_message":"Fixed the bug..."}
{"type":"activity","session_id":"abc-123","provider":"claude","timestamp":1712700001000,"tool":"Edit","message_preview":"Changed foo to bar in main.rs"}
{"type":"session_ended","session_id":"abc-123","provider":"claude","timestamp":1712700010000}
```

### Daemon Behavior

- `cass daemon --port 8080` starts WebSocket server on specified port
- Combines filesystem watch (`notify` crate) + polling fallback (every 5s)
- Stateless: no persistence, treats all sessions as new after restart
- Filters to only emit events for sessions that were not in the previous poll cycle

## Data Flow

```
cass daemon
  ├── watches filesystem via notify crate
  ├── polls active_sessions() every 5s
  ├── compares current sessions vs previous poll
  ├── emits JSONL events over WebSocket
  └── connectors implement SessionMonitor trait
        ├── is_available()     -> skip if false
        ├── active_sessions()  -> list current sessions
        ├── session_detail()   -> enrich with tool history
        └── watch_paths()       -> filesystem watch targets
```

## Implementation Plan

### Phase 1: Extend Connector Trait
- Add `SessionMonitor` trait to `franken_agent_detection`
- Implement for existing connectors: openclaw, copilot, copilot_cli, claude_code, etc.
- Each connector returns appropriate paths and session metadata

### Phase 2: Daemon Command
- Add `daemon` subcommand to cass CLI
- WebSocket server using `tokio-tungstenite` or similar
- Polling loop with session diffing logic
- JSONL event emission

### Phase 3: Testing
- Unit tests for each connector's session methods
- Integration test with mock WebSocket client
- Test filesystem watch + polling coordination

## File Changes

### New files
- `franken_agent_detection/src/session_monitor.rs` - trait + types
- `franken_agent_detection/src/connectors/session_monitor_impl.rs` - implementations for each connector
- `cass/src/bin/daemon.rs` - daemon WebSocket server

### Modified files
- `franken_agent_detection/src/lib.rs` - export SessionMonitor
- `franken_agent_detection/src/connectors/mod.rs` - re-export SessionMonitor
- `cass/src/main.rs` - add daemon subcommand
- `cass/src/lib.rs` - daemon implementation

## Dependencies

Add to `franken_agent_detection/Cargo.toml`:
- `tokio` for async runtime
- `tokio-tungstenite` for WebSocket server
- `notify` for filesystem watching

Add to `cass/Cargo.toml`:
- Same dependencies, or use existing asupersync runtime

## Testing Strategy

1. **Unit tests**: Each connector's `active_sessions()` and `session_detail()` with mock filesystem
2. **Connector tests**: Test that all connectors compile and return expected shape
3. **Daemon integration**: Start daemon, connect WebSocket client, verify events received
4. **Polling vs watch**: Verify both mechanisms catch session changes
