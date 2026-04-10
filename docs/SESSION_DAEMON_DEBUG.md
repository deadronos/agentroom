# Session Monitoring (session-daemon) Debug Report

**Date:** 2026-04-10
**Status:** Partially working — SessionMonitor implementations complete; `session-daemon` command exits immediately with code 0 instead of running the WebSocket server.

## What Was Implemented

### SessionMonitor Trait for 5 Connectors

The `SessionMonitor` trait was implemented for the following connectors in `franken_agent_detection/src/connectors/`:

| Connector | Storage | Notes |
|-----------|---------|-------|
| `opencode.rs` | SQLite (v1.2+) with JSON file fallback | Uses `load_last_message_sqlite` helper in `impl OpenCodeConnector` |
| `gemini.rs` | JSONL at `~/.gemini/tmp/<hash>/chats/session-*.json` | |
| `codex.rs` | `rollout-*.jsonl` at `~/.codex/sessions/` | |
| `vibe.rs` | `messages.jsonl` at `~/.vibe/logs/session/*/` | |
| `clawdbot.rs` | JSONL at `~/.clawdbot/sessions/*.jsonl` | |

### SessionWatcher Integration

`cass/src/session_daemon/watcher.rs` was updated to poll all 10 connectors (the 5 new ones plus the 5 pre-existing: openclaw, copilot, copilot_cli, claude_code, aider).

### WebSocket Server

`cass/src/session_daemon/server.rs` implements `run_websocket_server()` which binds a TCP port and streams `SessionEvent` as JSONL to connected WebSocket clients.

## What Works

### Compilation and Tests
- `cargo check --features connectors` in `franken_agent_detection`: **passes** (606 tests pass, 1 pre-existing failure in `claude_code.rs`)
- `cargo check` in `cass`: **passes** with warnings only
- `cargo build --release` in `cass`: **passes**

### Binary Verification
- Binary contains all expected strings from the SessionMonitor implementations
- `cass diag` correctly detects all connector storage paths
- `cass sessions` correctly lists sessions from all connectors
- `cass --help` shows `session-daemon` command with correct options
- `cass session-daemon --help` shows correct help text

### Command Parsing
- `cass session-daemon --port 9876` is accepted without argument parsing errors
- `cass --trace-file /tmp/trace.jsonl session-daemon --port 9876` produces a valid trace record with `"cmd": "session-daemon"` and `"exit_code": 0`

## What Doesn't Work

### `session-daemon` Command Exits Immediately

**Symptom:** Running `./target/release/cass session-daemon --port 9876` (or any port) exits with code 0 immediately, without binding the port or entering the WebSocket event loop.

**Evidence:**
- Process exits within <1 second of spawning
- `lsof -i :9876` shows no listening socket
- No output to stdout or stderr (tracing subscriber is initialized)
- Trace file shows `duration_ms: 0` and `exit_code: 0`
- `kill -0 $pid` shows the process dies within 1 second

**Debugging Attempts (all failed to reveal root cause):**

1. **Added `eprintln!` statements in `run_websocket_server`**: The strings are present in the binary (`strings ./target/release/cass | grep "DEBUG:"` shows them), but nothing is printed when the command runs. This implies `run_websocket_server` is never called.

2. **Added file-write markers (`std::fs::write("/tmp/marker", ...)`)**: The strings are in the binary, but files are never created. This implies the code path containing them is never reached.

3. **Added `panic!("TEST")` before the `use crate::session_daemon::...` statement**: The panic string is in the binary, but the panic is never triggered. This implies the `#[cfg(unix)]` block for `Commands::SessionDaemon` is never entered.

4. **Verified with `RUST_BACKTRACE`, `RUST_LOG=trace`, `RUST_LOG=debug`**: No additional output.

5. **Ran with `env -i` (clean environment)**: Same behavior.

6. **Ran via Python `subprocess.Popen` with separate stdout/stderr capture**: Both are empty, return code 0.

7. **Ran with `script` command to get a PTY**: Same behavior.

8. **Ran under `strace` (not available on macOS)**: Not attempted.

9. **Confirmed binary timestamp is newer than sources**: Binary is from latest build.

10. **Compared with working commands**: `cass status`, `cass search`, `cass sessions` all work correctly and produce output.

### Investigation Path Not Pursued

The code path looks correct in `execute_cli()` at lines 3620-3647:
```rust
#[cfg(unix)]
Commands::SessionDaemon {
    port,
    active_threshold_ms,
    poll_interval_secs,
} => {
    use crate::session_daemon::{SessionWatcher, run_websocket_server};
    use tokio::sync::broadcast;
    use std::time::Duration;

    let (tx, rx) = broadcast::channel::<crate::session_daemon::SessionEvent>(100);
    let mut watcher = SessionWatcher::new(tx).with_threshold(active_threshold_ms);
    let poll_interval = Duration::from_secs(poll_interval_secs);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let server = run_websocket_server(port, rx);
            tokio::select! {
                result = server => { let _ = result; },
                _ = watcher.run(poll_interval) => {},
            }
        });
    });
}
```

The `#[cfg(unix)]` attribute is present and macOS is Unix (Darwin). The binary contains the `SessionDaemon` command variant and all help strings. The trace file proves the command is recognized.

## Open Questions for Next Investigator

1. Is the `#[cfg(unix)]` block actually being compiled? (Could verify by adding a compile-error inside the block and seeing if the build fails.)
2. Is `execute_cli` actually being called for this command? (The trace file suggests yes, but the behavior suggests no.)
3. Is there a early-exit path in the CLI parsing or execution that short-circuits before `execute_cli`?
4. Could there be a second instance of the binary being run (e.g., from `PATH`) that exits immediately?
5. Would running under a debugger (lldb) reveal where the process exits?

## Files Modified

### franken_agent_detection/src/connectors/
- `opencode.rs` — Added `SessionMonitor` impl + `load_last_message_sqlite` helper method
- `gemini.rs` — Added `SessionMonitor` impl
- `codex.rs` — Added `SessionMonitor` impl
- `vibe.rs` — Added `SessionMonitor` impl
- `clawdbot.rs` — Added `SessionMonitor` impl

### cass/src/session_daemon/
- `watcher.rs` — Updated `poll_and_diff()` to poll all 10 connectors including the 5 new ones

### cass/src/ (pre-existing, unchanged)
- `lib.rs` — Contains `execute_cli()` with the `SessionDaemon` command arm at lines 3620-3647
- `main.rs` — Entry point calling `run_with_parsed()`

## Key Code Locations

- SessionMonitor trait definition: `franken_agent_detection/src/connectors/session_monitor.rs`
- Watcher polls all connectors: `cass/src/session_daemon/watcher.rs:35-203`
- WebSocket server: `cass/src/session_daemon/server.rs:18-60`
- Command dispatch: `cass/src/lib.rs:3620-3647`
