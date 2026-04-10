# Session Monitoring Design: Split Architecture (Collector/Hub/Frontend)

## Overview

Port session watching/discovery from claude-ville into agentroom using a split collector/hub/frontend architecture. This replaces the current flaky polling-based single-process implementation with a distributed, multi-machine capable system.

## Goals

- Multi-machine support: collectors run on each machine, report to central hub
- Stable event-driven discovery using filesystem watchers (notify crate)
- WebSocket transport between all layers
- Bearer token authentication for collector-to-hub connections
- Real-time session events streamed to frontend clients

## Architecture

```
┌─────────────────┐   WebSocket    ┌─────────────────┐   WebSocket   ┌─────────────────┐
│    Collector    │ ─────────────► │       Hub       │ ◄─────────────│    Frontend     │
│ (per machine,   │   snapshots   │  (central hub)   │  merged state │ (browser/clients)│
│  runs as daemon)│               │                  │               │                  │
└─────────────────┘               └─────────────────┘               └─────────────────┘
         │                                │
         │ notify filesystem watching     │ in-memory state
         ▼                                ▼
   session log files                 session map (latest-wins per sessionId)
```

### Components

1. **Collector** (`session-collector` crate) - runs on each machine
2. **Hub** (`session-hub` crate) - central service receiving snapshots
3. **Frontend** - WebSocket API for clients

### Supported Agents

- openclaw, copilot, codex, claude, opencode, gemini

---

## Component Specifications

### 1. Collector

**Purpose**: Runs on each machine where CLI sessions occur. Discovers sessions via filesystem watching and publishes snapshots to hub.

**Binary**: `session-collector`

**Behavior**:
- Uses `notify` crate to watch agent session log directories
- Maintains local snapshot of current sessions
- Publishes via WebSocket to hub every 2 seconds OR when dirty flag set
- Deduplication via SHA1 fingerprinting (only sends if content changed)
- Bearer token in WebSocket connection URL: `ws://hub:8080?token=<TOKEN>`

**Configuration** (environment variables):
- `HUB_URL` - hub WebSocket endpoint (default: `ws://localhost:8080`)
- `HUB_AUTH_TOKEN` - bearer token for authentication
- `COLLECTOR_ID` - unique identifier for this collector (default: hostname)
- `FLUSH_INTERVAL_MS` - snapshot publish interval (default: 2000)
- `ACTIVE_THRESHOLD_MS` - filter sessions older than this (default: 120000)

**Adapter Pattern**: Each agent CLI has an adapter implementing:

```rust
trait SessionAdapter {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn watch_paths(&self) -> Vec<WatchPath>;
    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession>;
    fn session_detail(&self, session_id: &str) -> Option<SessionDetail>;
}
```

### 2. Hub

**Purpose**: Central service receiving snapshots from all collectors, merging state, and broadcasting to frontends.

**Binary**: `session-hub`

**Behavior**:
- Accepts WebSocket connections from collectors (auth via bearer token)
- Maintains `Map<collector_id, Snapshot>` for per-collector state
- Merges sessions from all collectors, latest-wins per sessionId
- Exposes WebSocket endpoint for frontend clients
- Broadcasts merged state on every incoming snapshot

**Configuration** (environment variables):
- `HUB_PORT` - port for WebSocket server (default: 8080)
- `HUB_AUTH_TOKEN` - bearer token collectors must present
- `ACTIVE_THRESHOLD_MS` - filter sessions older than this (default: 120000)

**Ports**:
- `8080` - collector ingestion WebSocket
- `8081` - frontend API WebSocket (optional: separate port or same with path routing)

### 3. Frontend API

**Purpose**: Real-time session events pushed to browser/clients.

**Protocol**: WebSocket

**Event Types**:

```json
// session_started
{"type":"session_started","session_id":"abc-123","provider":"claude","project":"/Users/me/project","model":"claude-opus-4-6","timestamp":1712700000000,"last_tool":"Edit","last_message":"Fixed the bug...","agent_id":null,"agent_type":"main"}

// activity
{"type":"activity","session_id":"abc-123","provider":"claude","timestamp":1712700001000,"tool":"Edit","message_preview":"Changed foo to bar in main.rs"}

// session_ended
{"type":"session_ended","session_id":"abc-123","provider":"claude","timestamp":1712700010000}
```

---

## Data Structures

### Snapshot (sent from collector to hub)

```rust
struct Snapshot {
    collector_id: String,
    timestamp: i64,
    fingerprint: String,  // SHA1 of serialized sessions
    sessions: Vec<ActiveSession>,
}

struct ActiveSession {
    session_id: String,
    provider: String,           // "claude", "openclaw", etc.
    agent_id: Option<String>,
    agent_type: String,          // "main", "sub-agent", "team-member"
    model: String,
    status: String,              // "active"
    last_activity: i64,         // Unix timestamp ms
    project: Option<String>,
    last_message: Option<String>,
    last_tool: Option<String>,
    last_tool_input: Option<String>,
    parent_session_id: Option<String>,
}
```

### WatchPath (per adapter)

```rust
struct WatchPath {
    path: PathBuf,
    watch_type: WatchType,      // File or Directory
    filter: Option<String>,      // e.g., "*.jsonl"
    recursive: bool,
}
```

---

## Adapter Specifications

### claude (Claude Code)

**Watch paths**:
- `~/.claude/logs/*.jsonl` (recursive)
- `~/.claude/projects/*/sessions/` (recursive, subdirectories)

**Session ID extraction**: Filename without extension

**Session detail**: Parse corresponding `.json` history file

### openclaw

**Watch paths**:
- `~/.openclaw/logs/*.jsonl`
- `~/.openclaw/projects/*/sessions/`

### copilot

**Watch paths**:
- `~/.copilot/logs/*.jsonl`
- `~/.copilot/sessions/`

### codex

**Watch paths**:
- `~/.codex/logs/*.jsonl`
- `~/.codex/sessions/`

### opencode

**Watch paths**:
- `~/.opencode/logs/*.jsonl`
- `~/.opencode/sessions/`

### gemini

**Watch paths**:
- `~/.gemini/logs/*.jsonl`
- `~/.gemini/sessions/`

---

## File Structure

```
src/
├── session_common/              # Shared types and traits
│   ├── lib.rs
│   ├── types.rs                 # Snapshot, ActiveSession, SessionEvent
│   └── adapter.rs               # SessionAdapter trait
├── session_collector/           # Collector daemon
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── watcher.rs           # notify-based filesystem watcher
│       ├── collector.rs         # Snapshot building and deduplication
│       ├── client.rs            # WebSocket client to hub
│       └── adapters/
│           ├── mod.rs
│           ├── claude.rs
│           ├── openclaw.rs
│           ├── copilot.rs
│           ├── codex.rs
│           ├── opencode.rs
│           └── gemini.rs
└── session_hub/                 # Hub service
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── server.rs            # WebSocket server (collector + frontend)
        ├── state.rs             # Session state management
        └── auth.rs              # Bearer token validation
```

---

## Transport Protocol

### Collector → Hub (snapshot ingestion)

**Connection**: `ws://hub:8080/collectors?token=<HUB_AUTH_TOKEN>`

**Messages** (collector → hub):
```json
{"type":"snapshot","collector_id":"machine-1","timestamp":1712700000000,"fingerprint":"abc123","sessions":[...]}
```

**Messages** (hub → collector):
```json
{"type":"ack","fingerprint":"abc123"}
{"type":"error","message":"Invalid token"}
```

### Hub → Frontend (session events)

**Connection**: `ws://hub:8081/sessions` (no auth for simplicity, or optional)

**Messages** (hub → frontend):
```json
{"type":"session_started","session_id":"abc-123","provider":"claude",...}
{"type":"activity","session_id":"abc-123","provider":"claude",...}
{"type":"session_ended","session_id":"abc-123","provider":"claude",...}
{"type":"state_sync","sessions":[...]}
```

---

## Error Handling

### Collector
- Hub connection lost: retry with exponential backoff (1s, 2s, 4s, max 30s)
- Adapter failure: log error, continue with other adapters
- Filesystem errors: log and skip affected paths

### Hub
- Invalid token: close connection with error message
- Malformed snapshot: log error, request re-send
- Collector disconnect: retain its sessions for 60s, then expire

---

## Dependencies

### session_common
- `serde` / `serde_json` - serialization
- `sha1` - fingerprinting

### session_collector
- `tokio` - async runtime
- `notify` - filesystem watching
- `tokio-tungstenite` - WebSocket client
- `futures-util` - stream utilities
- `session_common` - shared types

### session_hub
- `tokio` - async runtime
- `tokio-tungstenite` - WebSocket server
- `futures-util` - stream utilities
- `session_common` - shared types

---

## Configuration Reference

### Collector Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_URL` | `ws://localhost:8080` | Hub WebSocket endpoint |
| `HUB_AUTH_TOKEN` | (required) | Bearer token for hub auth |
| `COLLECTOR_ID` | hostname | Unique collector identifier |
| `FLUSH_INTERVAL_MS` | `2000` | Snapshot publish interval |
| `ACTIVE_THRESHOLD_MS` | `120000` | Filter old sessions (2min) |

### Hub Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_PORT` | `8080` | Collector ingestion port |
| `HUB_FRONTEND_PORT` | `8081` | Frontend API port |
| `HUB_AUTH_TOKEN` | (required) | Bearer token for collectors |
| `ACTIVE_THRESHOLD_MS` | `120000` | Filter old sessions |

---

## Testing Strategy

1. **Adapter unit tests**: Each adapter's `active_sessions()` with mock filesystem
2. **Collector integration**: Mock hub, verify snapshot format and deduplication
3. **Hub integration**: Multiple collectors, verify merge logic
4. **End-to-end**: Full stack test with real filesystem

---

## Open Issues / TODOs

- [ ] Determine if frontend WebSocket should share port with collector ingestion (path routing) or use separate port
- [ ] Decide on reconnection strategy for frontend clients
- [ ] Consider heartbeat/keepalive for WebSocket connections
- [ ] Add collector health monitoring (hub tracks last-seen per collector)
