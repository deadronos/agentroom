# AgentRoom

A desktop app that turns your AI coding agents into animated pixel art characters in a virtual office — with full session search, transcript browsing, and real-time activity monitoring across Claude Code, Codex, and Gemini.

![AgentRoom screenshot](docs/screenshots/hero-office.png)

## Quick Start

```bash
# Prerequisites: Rust (rustup.rs), Node.js 18+, Xcode CLI tools (macOS) or webkit2gtk (Linux)

git clone --recursive https://github.com/liuyixin-louis/agentroom-visual.git
cd agentroom-visual

# Build the CASS search backend (~5 min, one-time)
./scripts/install-cass.sh
source ~/.zshrc

# Index your agent sessions
cass index --full

# Launch the app
npm install
npm run tauri dev
```

## Features

- **Real-time agent visualization** — each active coding agent gets its own animated character that types when writing code, reads when searching files, and idles when waiting for input
- **Multi-agent support** — Claude Code, Codex, and Gemini agents displayed simultaneously with distinct visual styles
- **Work & idle rooms** — active agents sit at desks in the Work Room; idle agents walk to the Break Room and hang out on couches
- **Per-project focus** — switch the office view to show only agents working on a specific project
- **Session search & browsing** — search across all agent sessions with full-text search powered by [CASS](https://github.com/Dicklesworthstone/coding_agent_session_search), grouped by project
- **Transcript viewer** — click any session to read the full conversation with "Open in iTerm2" to resume
- **Sub-agent visualization** — Task tool sub-agents spawn as separate characters linked to their parent
- **Speech bubbles** — visual indicators when an agent is waiting for input or needs permission approval
- **Sound notifications** — chime when an agent finishes its turn
- **Token usage dashboard** — real-time spend and rate limit tracking
- **AI-powered session tagging** — auto-summarize and categorize sessions
- **Persistent layouts** — office design is saved per project

## How It Works

AgentRoom watches the JSONL transcript files that coding agents write to disk. A Rust file watcher (`notify` crate) detects new lines in real time and emits structured events to the frontend via Tauri's event system. The React frontend drives a Canvas 2D game engine with BFS pathfinding and a character state machine.

```
JSONL files (Claude/Codex/Gemini)
  -> Rust file watcher (notify + tokio)
    -> AgentStateManager (event dedup + state tracking)
      -> Tauri event bus
        -> React useAgentEvents hook
          -> OfficeState (Canvas 2D game engine)
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Shell** | [Tauri v2](https://tauri.app/) |
| **Backend** | Rust (tokio, notify, serde_json) |
| **Frontend** | React 18 + TypeScript + Vite |
| **Rendering** | Canvas 2D -- pixel-perfect at integer zoom levels |
| **Search** | [CASS](search-backend/cass/) -- bundled as submodule |
| **Tilesets** | 32x32px tiles from [SkyOffice](https://github.com/kevinshen56714/SkyOffice) |

---

## Installation (End-to-End)

Complete setup from a fresh machine to a running app.

### Step 0 -- System Prerequisites

**macOS:**
```bash
xcode-select --install
```

**Linux (Debian/Ubuntu):**
```bash
sudo apt update && sudo apt install -y \
  build-essential curl wget git \
  libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev \
  librsvg2-dev patchelf libssl-dev
```

**Linux (Fedora):**
```bash
sudo dnf install -y \
  gcc gcc-c++ make curl wget git \
  webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel openssl-devel
```

### Step 1 -- Install Rust

Skip if `cargo --version` already works.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version   # 1.85+ required (for Rust edition 2024)
```

### Step 2 -- Install Node.js

Skip if `node --version` shows 18+.

```bash
# via nvm (recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.0/install.sh | bash
source "$HOME/.nvm/nvm.sh"
nvm install 22

# or download directly from https://nodejs.org/
```

### Step 3 -- Clone with Submodules

```bash
git clone --recursive https://github.com/liuyixin-louis/agentroom-visual.git
cd agentroom-visual
```

> Already cloned without `--recursive`? Run `git submodule update --init --recursive`

### Step 4 -- Build the CASS Search Backend

CASS indexes and searches your coding agent session histories. It compiles from source into a single binary.

**Option A -- Automated install** (builds + adds to PATH):
```bash
./scripts/install-cass.sh
source ~/.zshrc   # or: source ~/.bashrc
```

**Option B -- Manual build:**
```bash
cd search-backend/cass
cargo build --release
# Binary: search-backend/cass/target/release/cass

# Install to PATH (pick one):
sudo cp target/release/cass /usr/local/bin/cass   # system-wide
# or: export PATH="$(pwd)/target/release:$PATH"   # session-only
cd ../..
```

**Option C -- System-wide install** (uses sudo):
```bash
./scripts/install-cass.sh --system
```

Verify: `cass --version`

> The release build takes 3-8 minutes (LTO enabled). Subsequent builds are fast.

### Step 5 -- Build the Search Index

```bash
cass index --full       # scans all agent session directories
cass health --json      # verify index health
```

CASS auto-detects sessions from these agents:

| Agent | Session Path |
|-------|-------------|
| Claude Code | `~/.claude/projects/` |
| Codex | `~/.codex/sessions/` |
| Gemini CLI | `~/.gemini/tmp/` |
| Cline | VS Code extension storage |
| ChatGPT | `~/Library/Application Support/com.openai.chat/` |
| Aider | `~/.aider.chat.history.md` |
| Cursor | VS Code state SQLite files |
| OpenCode | VS Code extension storage |
| Pi-Agent | `~/.pi/agent/sessions/` |
| Factory/Droid | `~/.factory/sessions/` |
| Amp | Local Sourcegraph cache |

### Step 6 -- Run the App

```bash
npm install
npm run tauri dev
```

The pixel office window opens within seconds. Active coding agents appear as animated characters at desks.

### Production Build

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/` (`.app`/`.dmg` on macOS, `.deb`/`.AppImage` on Linux).

### Troubleshooting

| Problem | Fix |
|---------|-----|
| `cass: command not found` | Run `source ~/.zshrc` or open a new terminal |
| `cargo build` linker errors | Install system prerequisites (Step 0) |
| `git submodule update` fails | Ensure submodule repos are accessible (see note below) |
| `cass index` finds 0 sessions | Install and use at least one coding agent first |
| `npm run tauri dev` webkit errors | Install Tauri system deps (Step 0, Linux only) |
| CASS build is slow | Normal for first build -- release mode with LTO takes 3-8 min |

> **Note on submodule access:** The search backend submodules are hosted on GitHub. If they are private repos, you'll need SSH access configured. Public repos work with HTTPS out of the box.

---

## Search Backend (CASS)

[CASS](https://github.com/Dicklesworthstone/coding_agent_session_search) (Coding Agent Session Search) is the search engine powering AgentRoom's session search. It's bundled as git submodules under `search-backend/`.

### Architecture

Sub-10ms interactive search across 11+ coding agent histories via a multi-layer search stack:

| Layer | Latency | Technique |
|-------|---------|-----------|
| Prefix cache (LRU + Bloom) | <5ms | Hot-path cache hits |
| Tantivy full-text (BM25) | 5-100ms | Inverted index + edge n-grams |
| Semantic search (FastEmbed) | 100-1000ms | MiniLM-384 embeddings + HNSW |
| Hybrid RRF fusion | 100-1500ms | Reciprocal Rank Fusion (K=60) |

### Submodule Layout

```
search-backend/
|-- cass/                       # Search engine + interactive TUI
|-- asupersync/                 # Async runtime
|-- frankentui/                 # Terminal UI framework
|-- frankensearch/              # Lexical, semantic, and fusion search
|-- franken_agent_detection/    # Agent auto-detection (11 connectors)
+-- toon_rust/                  # Shared utilities
```

### CASS Usage

```bash
cass index --full                              # rebuild index
cass search "auth error"                       # keyword search
cass search "rate limiting" --mode hybrid      # lexical + semantic
cass timeline --since 7d                       # recent activity
cass tui                                       # interactive TUI
cass health --json                             # index health check
```

For programmatic / AI agent integration (JSON output):
```bash
cass search "query" --json --limit 20 --highlight
cass export "/path/to/session.jsonl" --format markdown
```

---

## Project Structure

```
agentroom-visual/
|-- src/                        # React frontend
|   |-- office/                 # Game engine
|   |   |-- engine/             # Renderer, characters, pathfinding
|   |   |-- tilesets/           # TilesetManager, background gid map
|   |   |-- sprites/            # Character sprite data
|   |   +-- layout/             # Office layout serialization
|   |-- components/             # UI panels (SearchBar, SessionList, etc.)
|   |-- hooks/                  # useAgentEvents (core event bridge)
|   |-- services/               # CASS client, tag service
|   +-- bridge.ts               # Tauri invoke/listen bridge
|-- src-tauri/                  # Rust backend
|   +-- src/
|       |-- file_watcher.rs     # JSONL file watching + initial scan
|       |-- agent_state.rs      # Agent state machine + event emission
|       |-- commands.rs         # Tauri commands (CASS, tags, layout)
|       +-- transcript_parser.rs # JSONL line parsing
|-- search-backend/             # CASS search engine (git submodules)
|   |-- cass/                   # Main search binary + TUI
|   |-- asupersync/             # Async runtime
|   |-- frankentui/             # TUI framework
|   |-- frankensearch/          # Lexical + semantic + fusion search
|   |-- franken_agent_detection/ # Agent connector detection
|   +-- toon_rust/              # Utilities
|-- scripts/
|   |-- build-cass.sh           # Build CASS from source
|   +-- install-cass.sh         # Full install (build + PATH setup)
+-- public/assets/              # Tilesets, character sprites
```

## Acknowledgments

- **[CASS](https://github.com/Dicklesworthstone/coding_agent_session_search)** by Jeffrey Emanuel -- unified search over local coding agent histories
- **[Pixel Agents](https://github.com/pablodelucca/pixel-agents)** by Pablo de Lucca -- the original pixel art agent visualization (VS Code extension) from which the game engine is ported
- **[SkyOffice](https://github.com/kevinshen56714/SkyOffice)** by Kevin Shen -- tileset assets (FloorAndGround, Modern_Office, Generic, Basement)
- **[LimeZu](https://limezu.itch.io/)** -- pixel art assets used in SkyOffice's tilesets
- **[JIK-A-4 (Metro City)](https://jik-a-4.itch.io/metrocity-free-topdown-character-pack)** -- character sprite base

## License

[MIT](LICENSE)
