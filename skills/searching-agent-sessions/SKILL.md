---
name: searching-agent-sessions
description: >-
  Search and browse past coding agent sessions (Claude Code, Codex, Gemini) via CASS CLI.
  Use when the user asks to "find a session", "search my sessions", "what did I discuss with
  gemini about X", "find that conversation about Y", "show recent sessions", "session history",
  "find where I worked on project Z", or wants to retrieve past agent conversations by topic,
  agent, workspace, or time range. Also handles resuming sessions in iTerm2.
---

# Searching Agent Sessions — Execution Rules

## Goal & Scope
Help the user quickly find past coding agent sessions by topic, project, agent, or time.
Uses CASS CLI as the search backend.
Returns concise results with ready-to-copy resume commands.

## CASS Binary

The skill expects `cass` to be on PATH. If not found, check these locations:
```bash
# Installed via install-cass.sh (added to shell profile)
which cass

# Or use the binary directly from the build
CASS="$(git rev-parse --show-toplevel 2>/dev/null)/search-backend/cass/target/release/cass"
```

If CASS is not installed, direct the user to run:
```bash
./scripts/install-cass.sh && source ~/.zshrc
cass index --full
```

## Core Commands

### 1. Search by keyword/topic
```bash
cass search "<query>" --json --limit 20 --highlight
```
Optional filters:
- `--agent <agent>` — filter by agent: `claude-code`, `codex`, `gemini`
- `--days <N>` — limit to last N days
- `--workspace <path>` — filter by workspace/project path
- `--mode semantic` — use semantic search (better for conceptual queries)
- `--mode hybrid` — combine lexical + semantic

### 2. Recent sessions / timeline
```bash
cass timeline --json --group-by none --since 7d
cass timeline --json --group-by none --agent gemini --since 30d
```

### 3. Export a session transcript
```bash
cass export "<source_path>" --format markdown
cass export "<source_path>" --format json
```

### 4. Find related sessions
```bash
cass context "<source_path>" --json --limit 5
```

### 5. Session analytics
```bash
cass analytics tokens --days 7 --group-by day --json
cass analytics tools --days 7 --json
```

## Procedure

1. **Parse the user's intent** — extract: search terms, agent filter, time range, project/workspace hint.

2. **Choose search strategy:**
   - Specific keywords → `cass search "<query>" --json --limit 20`
   - Conceptual/vague query → `cass search "<query>" --mode hybrid --json --limit 20`
   - "Recent sessions" / "what did I do today" → `cass timeline --json --group-by none --since <range>`
   - "Sessions about project X" → `cass search "<project>" --json` or `--workspace` filter if path known

3. **Run the command** and parse JSON output.

4. **Deduplicate to parent sessions** — multiple hits often come from subagent files under the same parent session. Extract the parent UUID from each `source_path` (for subagent paths like `<uuid>/subagents/agent-*.jsonl`, the parent is `<uuid>.jsonl`). Group by parent UUID and show each unique parent session once.

5. **Resolve workspace from parent session** — for each unique parent session, read the `cwd` from the parent JSONL file (see Workspace Resolution). Do NOT use the `workspace` field from CASS search hits — it may come from subagents and point to a subdirectory.

6. **Present results** as a compact list per session (see Output Format below).

7. **Always include resume command** — for every session result, provide a ready-to-copy command block with `cd <cwd>` from step 5.

## Workspace Resolution

**CRITICAL:** Every resume command MUST include `cd <workspace> && ` before the resume command. The user opens a fresh terminal and pastes the command — without `cd` the agent resumes in the wrong directory and loses project context.

**WARNING:** Do NOT trust the `workspace` field from CASS search hits. CASS hits often come from subagent files whose `workspace` points to a subdirectory (e.g., `project/src-tauri`), not the directory where `claude` was originally invoked. Using the wrong directory means `--resume` will fail with "No conversation found" because Claude Code maps sessions to project slugs derived from the launch cwd.

After getting search results, **always** resolve workspace by reading the **parent** session file directly.

### Claude Code sessions
Path pattern: `~/.claude/projects/<project-slug>/<uuid>.jsonl` or `.../<uuid>/subagents/agent-*.jsonl`

1. **Deduplicate to parent session first.** For subagent files (`<uuid>/subagents/agent-xxx.jsonl`), extract the parent UUID from the path and use `<project-slug>/<uuid>.jsonl` as the main session file.
2. **Read the `cwd` from the parent session JSONL** (not subagent files):
   ```bash
   head -5 "<parent_session_file>.jsonl" | python3 -c "
   import sys, json
   for line in sys.stdin:
       try:
           d = json.loads(line.strip())
           cwd = d.get('cwd') or d.get('workspace')
           if cwd:
               print(cwd)
               break
       except: pass
   "
   ```
3. **Use the `cwd` value as the `cd` target** in the resume command. This is the directory where `claude` was launched, and `--resume` only works from a directory that maps to the same project slug.
4. **Cross-check with project slug:** The `<project-slug>` in the path is the cwd with `/` replaced by `-`. If cwd extraction fails, reverse the slug: strip leading `-`, replace remaining `-` with `/`, prepend `/` — that's the workspace.

### Codex sessions
Path pattern: `~/.codex/sessions/YYYY/MM/DD/rollout-...-<uuid>.jsonl`

1. Read the first line (type=`session_meta`), workspace is in `payload.cwd`:
   ```bash
   head -1 "<session_file>" | python3 -c "
   import sys, json
   d = json.loads(sys.stdin.readline())
   print(d.get('payload', {}).get('cwd', ''))
   "
   ```

### Gemini sessions
Path pattern: `~/.gemini/tmp/<project-dir>/chats/session-*.json`

The `<project-dir>` can be either:
- A **human-readable project name** (e.g., `my-project`)
- A **SHA256 hash** of the real project directory (legacy format)

To resolve workspace:
1. **Named folders** — if the folder name is human-readable (not a 64-char hex string), it's the project name. Check common project locations:
   - `~/Projects/<name>` or `~/<name>`
   - Read the session JSON's `projectHash` field, then check `~/.gemini/trustedFolders.json` — keys are real folder paths; SHA256-hash each key and compare against `projectHash`.
2. **SHA256 hash folders** — read `~/.gemini/trustedFolders.json` — keys are real folder paths. SHA256-hash each key and compare against the hash in the session path.

## Resume Command Construction

Extract session ID (UUID) from `source_path` using regex: `[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}`

### Gemini UUID — IMPORTANT
Gemini session filenames only contain a **short 8-char ID** (e.g., `session-2026-03-03T03-16-90638149.json`), but `gemini --resume` requires the **full UUID**. You MUST read the session JSON and extract the `sessionId` field:
```bash
python3 -c "
import json
with open('<session_file>') as f:
    data = json.load(f)
print(data.get('sessionId', ''))
"
```
Do NOT use the short ID from the filename — it will fail with "Invalid session identifier".

Resume commands by agent:
- **Claude Code**: `claude --resume <uuid> --dangerously-skip-permissions`
- **Codex**: `codex resume <uuid>` (NOTE: user's shell may alias `codex` to already include `--dangerously-bypass-approvals-and-sandbox` — do NOT add the flag by default to avoid doubling)
- **Gemini**: `gemini --resume <full-uuid> --yolo` (MUST be full UUID from `sessionId` field, NOT the short ID from filename)

## Output Format — CRITICAL

For each unique session found, output in this exact format:

```
### Session N — <Agent> — <relative time>

**Topic:** <title or first meaningful snippet, ~1-2 lines>

**Resume:**
```bash
cd <workspace> && <resume_command>
```
```

Rules:
- **Always include the resume command block** with `cd <workspace> &&` prepended when workspace is known. If workspace cannot be resolved, just show the resume command alone.
- The user will copy-paste these commands into their own terminal. Do NOT offer to run them. Do NOT use osascript.
- Group hits by unique session — do NOT show the same session file multiple times.
- Show top 5-8 unique sessions max.
- Keep topic/title concise — one or two lines, not a wall of text.
- Show the source path abbreviated (last 2-3 components) on a separate line if useful.
- If session ID cannot be extracted (rare), note "resume unavailable" instead of the command block.

### Example output

```
### Session 1 — Gemini — Feb 18

**Topic:** Headless agent infra — cron scheduling, auth profiles, CLI runner architecture

**Resume:**
```bash
cd ~/my-project && gemini --resume 6737dd0a-1b2c-4d5e-8f9a-0b1c2d3e4f5a --yolo
```

---

### Session 2 — Claude Code — Feb 17

**Topic:** Desktop app Tauri setup and CASS integration

**Resume:**
```bash
cd ~/Projects/my-app && claude --resume 513d51d0-033e-4583-afe4-7eb448e8a3b2 --dangerously-skip-permissions
```
```

## Quality Checklist
- [ ] Used --json flag for machine-parseable output
- [ ] Deduplicated by source_path — each session appears once
- [ ] Every session has a resume command block (or explicit "unavailable" note)
- [ ] Workspace resolved when possible (especially for Gemini hash paths)
- [ ] Did not dump raw JSON to the user
- [ ] Did not offer to "run" or "execute" — user copies commands themselves
- [ ] Respected agent/time/workspace filters from user query
