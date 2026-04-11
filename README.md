# AgentRoom

A desktop app that turns your AI coding agents into animated pixel art characters in a virtual office — with full session search, transcript browsing, and real-time activity monitoring across Claude Code, Copilot, Codex, Gemini, and more.

![AgentRoom screenshot](docs/screenshots/hero-office-tagged.png)

---

## Supported Agents & Session Paths

Each agent CLI stores session history in different locations. The session collector watches these paths to detect active agents.

| Agent | Session Path | File Format |
|-------|-------------|-------------|
| **Claude Code** | `~/.claude/history.jsonl` | JSONL (primary index) |
| | `~/.claude/projects/{encoded}/` | JSONL per session |
| | `~/.claude/projects/{encoded}/{session}/subagents/` | Sub-agent session files |
| **OpenClaw** | `~/.openclaw/agents/{agentId}/sessions/*.jsonl` | JSONL |
| **Copilot** | `~/.copilot/session-state/{uuid}/events.jsonl` | JSONL |
| **Codex** | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | JSONL |
| **Gemini** | `~/.gemini/tmp/{projectHash}/chats/session-*.json` | JSON (with messages array) |

> **Note:** Project paths for Claude Code are encoded: `/` becomes `-` in directory names (e.g., `/Users/me/project` → `-Users-me-project`). Gemini stores SHA-256 hashes of project paths, which are reverse-resolved by hashing candidate paths.

---

## Running AgentRoom

AgentRoom supports three run modes depending on your needs.

### Mode 1: Tauri Integrated (Default)

Runs the frontend, session hub, and session collector all in one process via Tauri. Best for single-machine use.

```bash
npm install
npm run tauri dev
```

The pixel office window opens. Active coding agents appear as animated characters at desks.

---

### Mode 2: Split Services (Multi-Machine)

Runs the session collector as a standalone daemon that connects to a central hub service. Best for monitoring agents across multiple machines.

```
┌─────────────────┐   WebSocket    ┌─────────────────┐   WebSocket   ┌─────────────────┐
│    Collector    │ ─────────────► │       Hub       │ ◄─────────────│    Frontend     │
│ (per machine,   │   snapshots   │  (central hub)   │  merged state │  (Tauri app or   │
│  runs as daemon)│               │                  │               │  browser)       │
└─────────────────┘               └─────────────────┘               └─────────────────┘
         │                                │
         │ notify filesystem watching     │ in-memory state
         ▼                                ▼
   session log files                 session map
```

**Terminal 1 — Start the Hub:**

```bash
cargo run --package session_hub
```

The hub requires `HUB_AUTH_TOKEN`. Set it in `.env` or as an environment variable:

```bash
echo "HUB_AUTH_TOKEN=your-secret-token" > .env
```

Or inline:
```bash
HUB_AUTH_TOKEN=secret cargo run --package session_hub
```

**Terminal 2 — Start the Collector (on each machine):**

```bash
cargo run --package session_collector
```

The collector needs `HUB_URL` and `HUB_AUTH_TOKEN`:

```bash
HUB_URL=ws://localhost:8080 \
HUB_AUTH_TOKEN=secret \
COLLECTOR_ID=my-machine \
cargo run --package session_collector
```

**Terminal 3 — Run the Tauri Frontend:**

```bash
npm run tauri dev
```

The frontend connects to the hub at `ws://localhost:8081/sessions`.

---

### Mode 3: Frontend Only (Development)

Runs only the Vite dev server on port 5173 without Tauri. Useful for frontend iteration when you don't need the desktop shell or session monitoring.

```bash
npm install
npm run dev
```

Then open `http://localhost:5173` in your browser.

> **Note:** When running in frontend-only mode, Tauri API calls (session watching, CASS search) will fail gracefully — the office will show no agents until connected to a hub.

---

## Environment Configuration

Copy `.env.example` to `.env` and configure:

```bash
cp .env.example .env
```

### Collector Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_URL` | `ws://localhost:8080` | Hub WebSocket endpoint for collector ingestion |
| `HUB_AUTH_TOKEN` | (required) | Bearer token matching the hub's token |
| `COLLECTOR_ID` | hostname | Unique identifier for this machine's collector |
| `FLUSH_INTERVAL_MS` | `2000` | How often to send snapshots to hub (milliseconds) |
| `ACTIVE_THRESHOLD_MS` | `120000` | Skip sessions inactive for longer than this (milliseconds) |

### Hub Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HUB_PORT` | `8080` | WebSocket port for collector ingestion |
| `HUB_FRONTEND_PORT` | `8081` | WebSocket port for frontend clients |
| `HUB_AUTH_TOKEN` | (required) | Bearer token collectors must present |

### Gemini Extra Paths

| Variable | Default | Description |
|----------|---------|-------------|
| `AGENTROOM_GEMINI_SCAN_DIRS` | (none) | Extra comma-separated directories to scan for Gemini sessions. Paths starting with `~/` are expanded relative to home. |

Example:
```bash
AGENTROOM_GEMINI_SCAN_DIRS="~/openclaw/.gemini/tmp,/mnt/other-machine/.gemini/tmp"
```

---

## Session Monitoring Architecture

### Collector

The collector runs on each machine and watches agent session files using filesystem events (via the `notify` crate). Every `FLUSH_INTERVAL_MS` milliseconds (default 2s), it builds a snapshot of all active sessions and sends it to the hub via WebSocket. If no sessions have changed, it skips sending (deduplication via SHA-1 fingerprint).

### Hub

The hub receives snapshots from all collectors, merges them (latest-wins per `sessionId`), and broadcasts merged state to connected frontend clients via WebSocket. Sessions not refreshed within `ACTIVE_THRESHOLD_MS` are considered stale and expire.

### Frontend

The frontend receives real-time events: `session_started`, `activity`, `session_ended`, and `state_sync` (full snapshot on connect). It renders agents as animated pixel characters in the office view.

---

## Supported Session Event Types

```json
{"type":"session_started","session_id":"abc-123","provider":"claude","project":"/Users/me/project","model":"claude-opus-4-6","timestamp":1712700000000,"last_tool":"Edit","last_message":"Fixed the bug...","agent_id":null,"agent_type":"main"}

{"type":"activity","session_id":"abc-123","provider":"claude","timestamp":1712700001000,"tool":"Edit","message_preview":"Changed foo to bar in main.rs"}

{"type":"session_ended","session_id":"abc-123","provider":"claude","timestamp":1712700010000}

{"type":"state_sync","sessions":[...]}
```

---

## Features

- **Real-time agent visualization** — each active coding agent gets its own animated character that types when writing code, reads when searching files, and idles when waiting for input
- **Multi-agent support** — Claude Code, Copilot, Codex, Gemini, OpenClaw, and OpenCode agents displayed simultaneously with distinct visual styles
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
| **Session Monitoring** | session_collector + session_hub |

---

## File Structure

```
agentroom/
├── src/
│   ├── session_common/           # Shared session types (Rust)
│   ├── session_collector/        # Collector daemon (Rust)
│   └── session_hub/              # Hub service (Rust)
├── src-tauri/                    # Tauri desktop shell
├── search-backend/               # CASS search engine (git submodule)
│   ├── cass/                     # Search binary + TUI
│   ├── asupersync/
│   └── frankensearch/
└── skills/                       # Claude Code skills
```

---

## Prerequisites

- **Rust** — [rustup.rs](https://rustup.rs)
- **Node.js** 18+
- **macOS**: Xcode CLI tools
- **Linux**: webkit2gtk

```bash
# Clone with submodules
git clone --recursive https://github.com/deedronos/agentroom.git
cd agentroom

# Initialize submodules
git submodule update --init --recursive

# Install frontend dependencies
npm install
```

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `command not found: cargo` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Collector can't connect to hub | Verify `HUB_AUTH_TOKEN` matches on both; check firewall rules |
| No agents appearing | Run collector manually to see debug output; check session paths exist |
| Empty session list | Verify agent session files exist at expected paths |
| Search not working | Run `cass index --full` to rebuild the index |
| Gemini sessions not found | Set `AGENTROOM_GEMINI_SCAN_DIRS` if Gemini stores sessions in a non-standard location |

---

## License

MIT