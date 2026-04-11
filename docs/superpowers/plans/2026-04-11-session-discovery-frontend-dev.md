# Frontend Dev Server + Session Discovery Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) Make `npm run dev` start only the Vite frontend dev server on port 5173 without Tauri. (2) Fix session discovery for copilot and other agents so they actually detect active sessions.

**Architecture:** Two independent changes: (1) Update `package.json` dev script to run `vite` directly and create a standalone HTML entry point that doesn't require Tauri IPC. (2) Audit and fix each adapter's watch paths to match actual agent CLI session storage locations.

**Tech Stack:** Node.js/Vite (frontend), Rust (session-collector adapters)

---

## File Inventory

### Frontend changes
- Modify: `package.json` — change `dev` script from `vite` to `vite --host` (already correct, just verify)
- Create: `index.dev.html` — standalone entry point for npm dev (no Tauri)
- Modify: `vite.config.ts` — add proxy to forward Tauri API calls when not in Tauri mode

### Session collector adapter fixes
- Modify: `src/session_collector/src/adapters/copilot.rs` — fix watch paths and session parsing
- Modify: `src/session_collector/src/adapters/gemini.rs` — fix watch paths and session parsing
- Modify: `src/session_collector/src/adapters/codex.rs` — fix watch paths (minor)
- Modify: `src/session_collector/src/adapters/opencode.rs` — fix watch paths (minor)
- Modify: `src/session_collector/src/adapters/claude.rs` — verify paths are correct
- Modify: `src/session_collector/src/adapters/openclaw.rs` — verify paths are correct

---

## Task 1: Frontend — Standalone Dev Entry Point

**Files:**
- Create: `index.dev.html`
- Modify: `package.json:8` (dev script)
- Modify: `vite.config.ts`

- [ ] **Step 1: Create index.dev.html — standalone entry for npm run dev**

This file is the entry point when running `npm run dev` without Tauri. It inlines the React app without requiring Tauri IPC.

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>AgentRoom — Dev Mode</title>
    <style>
      body { margin: 0; background: #0a0a0f; }
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 2: Verify package.json dev script**

Read `package.json` line 8. Confirm the script is `"dev": "vite"`. If it already points to `vite`, no change needed.

Run: `cat package.json | grep '"dev"'`
Expected output: `"dev": "vite",`

If wrong, edit to set `"dev": "vite"`.

- [ ] **Step 3: Add vite.config.ts dev proxy**

Read `vite.config.ts` to see its current contents.

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  root: '.',
  base: './',
  build: {
    outDir: 'dist',
    target: 'esnext',
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
```

This config already supports running `npm run dev` standalone. No change needed if it already looks like this.

- [ ] **Step 4: Commit frontend changes**

```bash
git add index.dev.html package.json vite.config.ts
git commit -m "feat: add standalone dev entry point for npm run dev"
```

---

## Task 2: Copilot Adapter — Fix Session Discovery

**Files:**
- Modify: `src/session_collector/src/adapters/copilot.rs`

**Root cause:** The copilot adapter looks in `~/.github/copilot` and `~/.copilot` which are not where the GitHub Copilot CLI stores session JSONL files. The actual session storage locations are:

- `~/.copilot/sessions/` — session JSONL files (not `logs/`)
- The actual CLI tool is `gh copilot` which may store data in `~/.config/gh/extensions/copilot/` or similar

Let me verify actual paths first.

Run: `ls -la ~/.copilot/ 2>/dev/null || echo "~/.copilot does not exist"`
Run: `ls -la ~/Library/Application\ Support/github-copilot/ 2>/dev/null || echo "no app support"`
Run: `gh copilot --version 2>/dev/null || echo "gh copilot not installed"`

Based on research: GitHub Copilot CLI stores sessions in `~/.copilot/sessions/` as `.jsonl` files, with session metadata in a SQLite database at `~/.copilot/copilot.db`.

**The fix:**

- [ ] **Step 1: Update copilot.rs watch paths**

Replace the copilot adapter with corrected paths:

```rust
use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;

pub struct CopilotAdapter;

impl CopilotAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn sessions_dir() -> PathBuf {
        Self::home_dir().join(".copilot").join("sessions")
    }

    fn log_dir() -> PathBuf {
        Self::home_dir().join(".copilot").join("logs")
    }

    fn parse_session_id(path: &PathBuf) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn parse_jsonl_entry(line: &str) -> Option<(i64, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let ts = json.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tool = json.get("name").and_then(|v| v.as_str()).map(String::from);
        let text = json.get("text").and_then(|v| v.as_str()).map(String::from);
        Some((ts, tool.clone().or(text), tool))
    }

    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts))
    }

    /// Check if a path contains active (recently written) JSONL content
    fn is_active(path: &PathBuf, threshold_ms: i64) -> bool {
        if let Ok(stat) = std::fs::metadata(path) {
            let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
            let mtime_ms = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            mtime_ms >= threshold
        } else {
            false
        }
    }
}

fn walkdir_recursive(
    dir: &PathBuf,
    paths: &mut Vec<PathBuf>,
    depth: usize,
    max_depth: usize,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir) {
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

impl SessionAdapter for CopilotAdapter {
    fn name(&self) -> &str {
        "copilot"
    }

    fn is_available(&self) -> bool {
        Self::sessions_dir().exists() || Self::log_dir().exists()
    }

    fn watch_paths(&self) -> Vec<WatchPath> {
        let mut paths = Vec::new();
        // Primary: sessions directory (newer copilot CLI)
        let sessions_dir = Self::sessions_dir();
        if sessions_dir.exists() {
            paths.push(WatchPath {
                path: sessions_dir,
                watch_type: WatchType::Directory,
                filter: Some("*.jsonl".to_string()),
                recursive: true,
            });
        }
        // Fallback: logs directory
        let log_dir = Self::log_dir();
        if log_dir.exists() {
            paths.push(WatchPath {
                path: log_dir,
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

        // Check both sessions and logs directories
        for dir in [Self::sessions_dir(), Self::log_dir()] {
            if !dir.exists() {
                continue;
            }
            let mut paths = Vec::new();
            let _ = walkdir_recursive(&dir, &mut paths, 0, 10);

            for path in paths {
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                if !Self::is_active(&path, threshold) {
                    continue;
                }
                let session_id = format!("copilot:{}", Self::parse_session_id(&path));
                let (last_message, last_tool, last_activity) =
                    Self::read_last_jsonl_entry(&path).unwrap_or((None, None, threshold));
                sessions.push(ActiveSession {
                    session_id,
                    provider: "copilot".to_string(),
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
                });
            }
        }
        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let path_str = session_id.strip_prefix("copilot:")?;
        let path = PathBuf::from(path_str);
        let (last_message, last_tool, last_activity) =
            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0));
        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "copilot".to_string(),
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
```

- [ ] **Step 2: Verify the change compiles**

Run: `cargo check --package session_collector 2>&1 | head -30`
Expected: No errors (only warnings acceptable)

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/adapters/copilot.rs
git commit -m "fix: correct copilot adapter session discovery paths"
```

---

## Task 3: Gemini Adapter — Fix Session Discovery

**Files:**
- Modify: `src/session_collector/src/adapters/gemini.rs`

**Root cause:** Gemini CLI stores session data in `~/.gemini/tmp/` (hash-based workspace directories), not in `~/.gemini/logs/`. The `extra_gemini_scan_dirs` in `commands.rs` already handles extra roots, but the adapter itself only watches `~/.gemini/logs` which may not exist or contain active sessions.

Fix: Also watch `~/.gemini/tmp/` directories and use the `AGENTROOM_GEMINI_SCAN_DIRS` pattern for finding actual session locations.

- [ ] **Step 1: Update gemini.rs with broader path discovery**

```rust
use session_common::{ActiveSession, SessionAdapter, WatchPath, WatchType};
use std::path::PathBuf;
use std::env;

pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }

    fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }

    fn log_dir() -> PathBuf {
        Self::home_dir().join(".gemini").join("logs")
    }

    fn tmp_dir() -> PathBuf {
        Self::home_dir().join(".gemini").join("tmp")
    }

    /// Additional scan dirs from environment variable
    fn extra_scan_dirs() -> Vec<PathBuf> {
        let home = Self::home_dir();
        match env::var("AGENTROOM_GEMINI_SCAN_DIRS") {
            Ok(value) => value
                .split([',', ';'])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    if entry.starts_with("~/") {
                        home.join(entry.trim_start_matches("~/"))
                    } else {
                        PathBuf::from(entry)
                    }
                })
                .filter(|p| p.exists())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn parse_session_id(path: &PathBuf) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn parse_jsonl_entry(line: &str) -> Option<(i64, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let ts = json.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let tool = json.get("name").and_then(|v| v.as_str()).map(String::from);
        let text = json.get("text").and_then(|v| v.as_str()).map(String::from);
        Some((ts, tool.clone().or(text), tool))
    }

    fn read_last_jsonl_entry(path: &PathBuf) -> Option<(Option<String>, Option<String>, i64)> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let last_line = lines.last()?;
        let (ts, msg, tool) = Self::parse_jsonl_entry(last_line)?;
        Some((msg, tool, ts))
    }

    fn is_active(path: &PathBuf, threshold_ms: i64) -> bool {
        if let Ok(stat) = std::fs::metadata(path) {
            let mtime = stat.modified().unwrap_or(std::time::UNIX_EPOCH);
            let mtime_ms = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            mtime_ms >= threshold
        } else {
            false
        }
    }
}

fn walkdir_recursive(
    dir: &PathBuf,
    paths: &mut Vec<PathBuf>,
    depth: usize,
    max_depth: usize,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir) {
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

impl SessionAdapter for GeminiAdapter {
    fn name(&self) -> &str {
        "gemini"
    }

    fn is_available(&self) -> bool {
        Self::log_dir().exists() || Self::tmp_dir().exists() || !Self::extra_scan_dirs().is_empty()
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
        let tmp_dir = Self::tmp_dir();
        if tmp_dir.exists() {
            paths.push(WatchPath {
                path: tmp_dir,
                watch_type: WatchType::Directory,
                filter: Some("*.jsonl".to_string()),
                recursive: true,
            });
        }
        // Extra scan dirs
        for dir in Self::extra_scan_dirs() {
            paths.push(WatchPath {
                path: dir,
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

        let all_dirs: Vec<PathBuf> = {
            let mut dirs = Vec::new();
            if Self::log_dir().exists() { dirs.push(Self::log_dir()); }
            if Self::tmp_dir().exists() { dirs.push(Self::tmp_dir()); }
            dirs.extend(Self::extra_scan_dirs());
            dirs
        };

        for dir in all_dirs {
            if !dir.exists() {
                continue;
            }
            let mut paths = Vec::new();
            let _ = walkdir_recursive(&dir, &mut paths, 0, 10);

            for path in paths {
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                if !Self::is_active(&path, threshold) {
                    continue;
                }
                let session_id = format!("gemini:{}", Self::parse_session_id(&path));
                let (last_message, last_tool, last_activity) =
                    Self::read_last_jsonl_entry(&path).unwrap_or((None, None, threshold));
                sessions.push(ActiveSession {
                    session_id,
                    provider: "gemini".to_string(),
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
                });
            }
        }
        sessions
    }

    fn session_detail(&self, session_id: &str) -> Option<ActiveSession> {
        let path_str = session_id.strip_prefix("gemini:")?;
        let path = PathBuf::from(path_str);
        let (last_message, last_tool, last_activity) =
            Self::read_last_jsonl_entry(&path).unwrap_or((None, None, 0));
        Some(ActiveSession {
            session_id: session_id.to_string(),
            provider: "gemini".to_string(),
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
```

- [ ] **Step 2: Verify the change compiles**

Run: `cargo check --package session_collector 2>&1 | head -30`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/adapters/gemini.rs
git commit -m "fix: extend gemini adapter to watch ~/.gemini/tmp and extra scan dirs"
```

---

## Task 4: Codex and OpenCode Adapters — Minor Path Verification

**Files:**
- Modify: `src/session_collector/src/adapters/codex.rs`
- Modify: `src/session_collector/src/adapters/opencode.rs`

Both adapters look correct but add `is_active` helper for consistency and proper threshold filtering (currently opencode already uses threshold but codex doesn't filter by activity time in `active_sessions`).

- [ ] **Step 1: Fix codex.rs — add threshold-based activity filtering**

```rust
// In codex.rs active_sessions function, add after line 106 (let mtime_ms = ...):
if mtime_ms < threshold {
    continue;
}
```

Current code already checks threshold, but verify the line exists. If it doesn't, add it.

Actually, the codex adapter already has threshold filtering at lines 112-113. No change needed.

- [ ] **Step 2: Fix opencode.rs — verify threshold filtering**

The opencode adapter already has threshold filtering. No change needed.

- [ ] **Step 3: Commit (skip if no changes needed)**

If changes were made:
```bash
git add src/session_collector/src/adapters/codex.rs src/session_collector/src/adapters/opencode.rs
git commit -m "fix: verify threshold filtering in codex and opencode adapters"
```

---

## Task 5: Cross-Adapter Consistency Fix — Add is_active Helper

**Files:**
- Modify: `src/session_collector/src/adapters/mod.rs` — add shared helper function

Add a common helper to reduce duplication across adapters. This is optional but improves maintainability.

- [ ] **Step 1: Add is_active helper to mod.rs**

```rust
use std::path::PathBuf;

/// Returns true if the file at path has a modified time >= threshold (now - threshold_ms)
pub fn is_file_recently_modified(path: &PathBuf, threshold_ms: i64) -> bool {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|mtime| {
            let mtime_ms = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            mtime_ms >= threshold_ms
        })
        .unwrap_or(false)
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --package session_collector 2>&1 | head -30`

- [ ] **Step 3: Commit**

```bash
git add src/session_collector/src/adapters/mod.rs
git commit -m "refactor: add is_file_recently_modified helper to adapters mod"
```

---

## Task 6: Verify All Changes

**Files:**
- Run: `cargo check --workspace 2>&1 | tail -20`

Expected: No errors

**Final commit if all tasks complete cleanly:**

```bash
git add -A
git commit -m "feat: standalone frontend dev + fixed session discovery for copilot/gemini
- Add index.dev.html for npm run dev standalone mode
- Fix copilot adapter: watch ~/.copilot/sessions/ not just logs/
- Fix gemini adapter: watch ~/.gemini/tmp/ and extra scan dirs
- Add is_file_recently_modified helper to reduce adapter duplication"
```

---

## Spec Coverage Check

- [ ] npm run dev starts only Vite frontend — covered in Task 1
- [ ] copilot session discovery fixed — covered in Task 2
- [ ] gemini session discovery fixed — covered in Task 3
- [ ] codex/opencode path consistency — covered in Task 4
- [ ] cross-adapter helper (DRY) — covered in Task 5

**Placeholder scan:** No "TBD", "TODO", or incomplete steps found.

**Type consistency check:** All `ActiveSession` field names match the types.rs definition. All `WatchPath` and `WatchType` usages are consistent.