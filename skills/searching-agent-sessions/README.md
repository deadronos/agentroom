# Searching Agent Sessions — Claude Code Skill

A drop-in Claude Code skill that lets you search, browse, and resume past coding agent sessions across Claude Code, Codex, and Gemini using natural language.

## What It Does

Ask Claude Code things like:
- "find my session about authentication middleware"
- "what did I discuss with gemini about rate limiting?"
- "show recent codex sessions from this week"
- "find where I worked on the API refactor"

The skill searches your local session history via [CASS](../search-backend/cass/) and returns results with ready-to-copy resume commands.

## Install

### 1. Build CASS (if not already done)

```bash
# From the agentroom-visual repo root:
./scripts/install-cass.sh
source ~/.zshrc

# Build the search index
cass index --full
```

### 2. Copy the skill into your Claude Code config

```bash
cp -r skills/searching-agent-sessions ~/.claude/skills/
```

### 3. Use it

Open Claude Code in any project and ask:

```
find sessions where I worked on database migrations
```

Claude Code will automatically invoke the skill, search your indexed sessions, and return results like:

```
### Session 1 — Claude Code — Mar 3

**Topic:** PostgreSQL migration scripts for user auth tables

**Resume:**
cd ~/Projects/my-app && claude --resume a1b2c3d4-... --dangerously-skip-permissions
```

## Supported Agents

| Agent | Session Path | Resume Command |
|-------|-------------|----------------|
| Claude Code | `~/.claude/projects/` | `claude --resume <uuid>` |
| Codex | `~/.codex/sessions/` | `codex resume <uuid>` |
| Gemini CLI | `~/.gemini/tmp/` | `gemini --resume <uuid>` |

Plus 8 more agents indexed by CASS (Aider, Cline, Cursor, ChatGPT, etc.).

## How It Works

1. **CASS CLI** indexes all your local agent session files (JSONL, JSON, SQLite)
2. **The skill** tells Claude Code when and how to invoke `cass search` / `cass timeline`
3. **Workspace resolution** reads the actual `cwd` from session files so resume commands work correctly
4. **Deduplication** groups subagent hits under their parent session

## Key Features

- **Cross-agent search** — one query searches Claude Code, Codex, Gemini, and 8+ more agents simultaneously
- **Semantic search** — `--mode hybrid` combines keyword matching with ML embeddings for conceptual queries
- **Ready-to-paste resume** — every result includes `cd <workspace> && <agent> --resume <uuid>` that you can copy straight into your terminal
- **Workspace-aware** — resolves the correct project directory even for Gemini's SHA256-hashed paths
- **Subagent dedup** — groups Task tool subagent sessions under their parent, so you see one result per conversation

## Requirements

- CASS binary on PATH (`cass --version`)
- Indexed sessions (`cass index --full`)
- Claude Code with skills support
