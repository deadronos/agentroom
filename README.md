# AgentRoom Visual

Pixel-art visual layer for AgentRoom — real-time agent monitoring with per-project offices.

## What is this?

A Tauri v2 desktop app that visualizes your coding agents (Claude Code, Codex, Gemini) as animated pixel art characters in virtual office rooms. Each project gets its own office, and characters animate in real-time based on what agents are actually doing.

Built on top of:
- [AgentRoom](https://github.com/liuyixin-louis/agentroom-desktop) — session search + resume (CASS-powered)
- [Pixel Agents](https://github.com/pablodelucca/pixel-agents) — pixel art game engine (Canvas 2D + character FSM)

## Features (Planned)

- **Real-time agent visualization** — characters type when writing code, read when searching files, idle when waiting
- **Per-project offices** — sidebar to switch between project rooms
- **Multi-agent support** — Claude / Codex / Gemini with distinct visual styles
- **Search → visual** — CASS search results highlight matching agents
- **Office editor** — customize each project's office layout
- **Session replay** — replay historical sessions as character animations

## Tech Stack

- **Shell**: Tauri v2
- **Backend**: Rust (tokio, notify, serde_json)
- **Frontend**: React 18 + TypeScript + Vite
- **Rendering**: Canvas 2D (ported from Pixel Agents)
- **Search/Index**: CASS CLI

## Development

```bash
npm install
npm run tauri dev
```

## License

MIT
