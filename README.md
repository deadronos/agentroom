# AgentRoom

A desktop app that turns your AI coding agents into animated pixel art characters in a virtual office — with full session search, transcript browsing, and real-time activity monitoring across Claude Code, Codex, Gemini, and more.

![AgentRoom screenshot](docs/screenshots/hero-office-tagged.png)

## What's New: Split Architecture Session Monitoring

AgentRoom now uses a **collector/hub/frontend** architecture for session monitoring that supports multiple machines:

```
┌─────────────────┐   WebSocket    ┌─────────────────┐   WebSocket   ┌─────────────────┐
│    Collector    │ ─────────────► │       Hub       │ ◄─────────────│    Frontend     │
│ (per machine,   │   snapshots   │  (central hub)   │  merged state │ (Tauri app or   │
│  runs as daemon)│               │                  │               │  browser)       │
└─────────────────┘               └─────────────────┘               └─────────────────┘
         │                                │
         │ notify filesystem watching     │ in-memory state
         ▼                                ▼
   session log files                 session map
```

- **Collector**: Runs on each machine, watches agent session files using filesystem events (not polling), sends snapshots to hub every 2 seconds or on change
- **Hub**: Central service receiving snapshots from all collectors, merges state (latest-wins per session), broadcasts to frontends
- **Frontend**: WebSocket client receiving real-time session events

This replaces the old flaky polling-based single-process `session-daemon`.

---

## Quick Start

### Prerequisites

- Rust (rustup.rs)
- Node.js 18+
- Xcode CLI tools (macOS) or webkit2gtk (Linux)

### Clone and Setup

```bash
git clone --recursive https://github.com/deadronos/agentroom.git
cd agentroom

# Initialize submodules
git submodule update --init --recursive

# Install frontend dependencies
npm install
```

### Build the Session Monitoring System

The session monitoring system is written in Rust. The crates are in `src/` (planned for relocation to `search-backend/`):

```bash
# Build session-common (shared types)
cargo build --package session_common

# Build session-hub (central service)
cargo build --package session_hub

# Build session-collector (per-machine daemon)
cargo build --package session_collector
```

Or build all at once from the workspace root if configured.

### Environment Setup

Copy the example env file and modify values as needed:

```bash
cp .env.example .env
```

Edit `.env` to set your `HUB_AUTH_TOKEN` and other values. The hub and collector will automatically load variables from `.env` when run from the repo root.

### Run the System

**Terminal 1 — Start the Hub:**

```bash
cd agentroom
HUB_AUTH_TOKEN=secret \
HUB_PORT=8080 \
HUB_FRONTEND_PORT=8081 \
cargo run --package session_hub
```

**Terminal 2 — Start the Collector (on each machine):**

```bash
cd agentroom
HUB_URL=ws://localhost:8080 \
HUB_AUTH_TOKEN=secret \
COLLECTOR_ID=my-machine \
cargo run --package session_collector
```

**Terminal 3 — Run the Tauri App:**

```bash
npm run tauri dev
```

The pixel office window opens. Active coding agents appear as animated characters at desks.

---

## Architecture Details

### Supported Agents

| Agent | Session Paths |
|-------|--------------|
| Claude Code | `~/.claude/logs/*.jsonl`, `~/.claude/projects/*/sessions/` |
| OpenClaw | `~/.openclaw/logs/*.jsonl`, `~/.openclaw/projects/*/sessions/` |
| Copilot | `~/.github/copilot/*.jsonl` |
| Codex | `~/.codex/logs/*.jsonl` |
| OpenCode | `~/.opencode/logs/*.jsonl` |
| Gemini | `~/.gemini/logs/*.jsonl` |

### Transport Protocol

**Collector → Hub:**
- WebSocket connection: `ws://hub:8080/collectors?token=<HUB_AUTH_TOKEN>`
- Collector sends snapshots every 2 seconds or when filesystem changes detected
- Deduplication via SHA1 fingerprinting

**Hub → Frontend:**
- WebSocket connection: `ws://hub:8081/sessions`
- Frontend receives `StateSync` on connect, then incremental events

### Event Types

```json
{"type":"session_started","session_id":"abc-123","provider":"claude","project":"/Users/me/project",...}
{"type":"activity","session_id":"abc-123","provider":"claude","timestamp":1712700001000,"tool":"Edit",...}
{"type":"session_ended","session_id":"abc-123","provider":"claude","timestamp":1712700010000}
{"type":"state_sync","sessions":[...]}
```

### Configuration

**Collector environment variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_URL` | `ws://localhost:8080` | Hub WebSocket endpoint |
| `HUB_AUTH_TOKEN` | (required) | Bearer token for hub auth |
| `COLLECTOR_ID` | hostname | Unique collector identifier |
| `FLUSH_INTERVAL_MS` | `2000` | Snapshot publish interval |
| `ACTIVE_THRESHOLD_MS` | `120000` | Filter sessions older than 2min |

**Hub environment variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_PORT` | `8080` | Collector ingestion port |
| `HUB_FRONTEND_PORT` | `8081` | Frontend API port |
| `HUB_AUTH_TOKEN` | (required) | Bearer token for collectors |

---

## File Structure

```
agentroom/
├── src/                           # React frontend
│   ├── session_common/           # Shared session types (Rust)
│   ├── session_collector/        # Collector daemon (Rust)
│   └── session_hub/              # Hub service (Rust)
├── src-tauri/                    # Tauri desktop shell
├── search-backend/               # CASS search engine (git submodule)
│   ├── cass/                     # Search binary + TUI
│   ├── asupersync/
│   ├── frankensearch/
│   └── franken_agent_detection/
└── skills/                       # Claude Code skills
```

**Note:** The session monitoring Rust crates (`session_common`, `session_collector`, `session_hub`) are currently in `src/`. Future work may relocate them to `search-backend/` to integrate with the existing Rust workspace.

---

## Features

- **Real-time agent visualization** — each active coding agent gets its own animated character that types when writing code, reads when searching files, and idles when waiting for input
- **Multi-agent support** — Claude Code, Codex, Gemini, OpenClaw, Copilot, and OpenCode agents displayed simultaneously with distinct visual styles
- **Multi-machine support** — collectors run on each machine, feed into a central hub
- **Work & idle rooms** — active agents sit at desks in the Work Room; idle agents walk to the Break Room
- **Per-project focus** — switch the office view to show only agents working on a specific project
- **Session search & browsing** — search across all agent sessions with full-text search powered by CASS
- **Transcript viewer** — click any session to read the full conversation in-app
- **Open in Terminal** — one-click "Open in iTerm2" button to jump straight into a session's working directory
- **Sub-agent visualization** — Task tool sub-agents spawn as separate characters linked to their parent
- **Speech bubbles** — visual indicators when an agent is waiting for input or needs approval
- **Sound notifications** — chime when an agent finishes its turn
- **Token usage dashboard** — real-time spend and rate limit tracking

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| **Shell** | Tauri v2 |
| **Backend** | Rust (tokio, notify, tokio-tungstenite) |
| **Frontend** | React 18 + TypeScript + Vite |
| **Rendering** | Canvas 2D |
| **Search** | CASS (git submodule) |
| **Session Monitoring** | session_collector + session_hub (new) |

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `command not found: cargo` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Collector can't connect to hub | Verify `HUB_AUTH_TOKEN` matches on both; check firewall rules |
| No agents appearing | Ensure collector is running; check `COLLECTOR_ID` appears in hub logs |
| Empty session list | Verify agent session files exist at expected paths |
| Search not working | Run `cass index --full` to rebuild the index |

---

## License

MIT
