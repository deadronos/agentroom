//! File watcher — monitors Claude Code JSONL transcript files.
//!
//! Uses the `notify` crate for filesystem events, plus a polling fallback.
//! Detects new/modified .jsonl files in the project's Claude directory,
//! reads new lines incrementally, and feeds them to the transcript parser.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::AppHandle;

use crate::agent_state::AgentStateManager;
use crate::transcript_parser;

/// Global cancel flag store: the active watcher's cancel flag.
/// Setting this flag causes both background tasks to exit their loops.
static CANCEL: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn cancel_store() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    CANCEL.get_or_init(|| Mutex::new(None))
}

/// Stop the currently running watcher by setting its cancel flag.
pub fn stop_watching_inner() {
    let mut guard = cancel_store().lock().unwrap();
    if let Some(flag) = guard.take() {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Per-file read state for incremental JSONL tailing.
struct FileReadState {
    offset: u64,
    line_buffer: String,
}

/// Shared state for the file watcher background task.
pub struct WatcherState {
    pub state_manager: AgentStateManager,
    file_states: HashMap<PathBuf, FileReadState>,
    known_jsonl_files: HashMap<PathBuf, (String, Option<String>)>, // path → (agent_id, agent_type)
}

impl WatcherState {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            state_manager: AgentStateManager::new(app_handle),
            file_states: HashMap::new(),
            known_jsonl_files: HashMap::new(),
        }
    }
}

/// Compute agent_id from a JSONL filename.
/// Claude Code uses UUIDs as session IDs in filenames.
fn agent_id_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Infer agent type from JSONL file path.
fn agent_type_from_path(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    if s.contains("/.claude/") {
        Some("claude-code".to_string())
    } else if s.contains("/.gemini/") {
        Some("gemini".to_string())
    } else if s.contains("/.codex/") {
        Some("codex".to_string())
    } else {
        None
    }
}

/// Find the Claude Code project directory for a given workspace path.
/// Claude Code stores sessions at ~/.claude/projects/<hash>/
fn find_claude_project_dir(project_dir: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let claude_dir = PathBuf::from(&home).join(".claude").join("projects");

    if !claude_dir.exists() {
        return None;
    }

    // If project_dir is empty, try to find any active project
    if project_dir.is_empty() {
        // Scan all project directories for recent .jsonl files
        if let Ok(entries) = fs::read_dir(&claude_dir) {
            let mut best_dir: Option<(PathBuf, std::time::SystemTime)> = None;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                // Check for recent .jsonl files
                if let Ok(jsonl_entries) = fs::read_dir(&path) {
                    for jentry in jsonl_entries.flatten() {
                        let jpath = jentry.path();
                        if jpath.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            if let Ok(meta) = jpath.metadata() {
                                if let Ok(modified) = meta.modified() {
                                    if best_dir
                                        .as_ref()
                                        .map(|(_, t)| modified > *t)
                                        .unwrap_or(true)
                                    {
                                        best_dir = Some((path.clone(), modified));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return best_dir.map(|(p, _)| p);
        }
        return None;
    }

    // Compute the project hash (same as Claude Code):
    // Replace :, \, / with - in the workspace path
    let hash = project_dir
        .replace(':', "-")
        .replace('\\', "-")
        .replace('/', "-");

    let project_path = claude_dir.join(&hash);
    if project_path.exists() {
        Some(project_path)
    } else {
        None
    }
}

/// Read new lines from a JSONL file incrementally.
fn read_new_lines(
    path: &Path,
    state: &mut FileReadState,
) -> Vec<String> {
    let mut lines = Vec::new();
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return lines,
    };

    if meta.len() <= state.offset {
        return lines;
    }

    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return lines,
    };

    if file.seek(SeekFrom::Start(state.offset)).is_err() {
        return lines;
    }

    let mut buf = vec![0u8; (meta.len() - state.offset) as usize];
    match file.read_exact(&mut buf) {
        Ok(()) => {}
        Err(_) => return lines,
    }

    state.offset = meta.len();

    let text = state.line_buffer.clone() + &String::from_utf8_lossy(&buf);
    let mut parts: Vec<&str> = text.split('\n').collect();
    state.line_buffer = parts.pop().unwrap_or("").to_string();

    for part in parts {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    lines
}

/// Scan a directory for .jsonl files and process any new ones.
fn scan_and_process(
    dir: &Path,
    shared: &Arc<Mutex<WatcherState>>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let mut state = shared.lock().unwrap();

        if !state.known_jsonl_files.contains_key(&path) {
            let agent_id = agent_id_from_path(&path);
            let agent_type = agent_type_from_path(&path);
            state
                .known_jsonl_files
                .insert(path.clone(), (agent_id.clone(), agent_type));
            state.file_states.insert(
                path.clone(),
                FileReadState {
                    offset: 0,
                    line_buffer: String::new(),
                },
            );
        }

        // Read new lines — extract values before mutable borrow
        let (agent_id, agent_type) = state.known_jsonl_files.get(&path).unwrap().clone();
        let had_tools = state
            .state_manager
            .agents
            .get(&agent_id)
            .map(|a| a.had_tools_in_turn)
            .unwrap_or(false);

        let file_state = state.file_states.get_mut(&path).unwrap();
        let lines = read_new_lines(&path, file_state);

        if !lines.is_empty() {
            state.state_manager.on_data_received(&agent_id);
        }

        for line in lines {
            let events = transcript_parser::parse_jsonl_line(&line, had_tools);
            state.state_manager.process_events(&agent_id, agent_type.as_deref(), events);
        }
    }
}

/// Process a single file change event from the filesystem watcher.
fn process_file_event(
    path: &Path,
    shared: &Arc<Mutex<WatcherState>>,
) {
    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return;
    }

    let mut state = shared.lock().unwrap();

    if !state.known_jsonl_files.contains_key(path) {
        let agent_id = agent_id_from_path(path);
        let agent_type = agent_type_from_path(path);
        state
            .known_jsonl_files
            .insert(path.to_path_buf(), (agent_id.clone(), agent_type));
        state.file_states.insert(
            path.to_path_buf(),
            FileReadState {
                offset: 0,
                line_buffer: String::new(),
            },
        );
    }

    let (agent_id, agent_type) = state.known_jsonl_files.get(path).unwrap().clone();
    let had_tools = state
        .state_manager
        .agents
        .get(&agent_id)
        .map(|a| a.had_tools_in_turn)
        .unwrap_or(false);

    let file_state = state.file_states.get_mut(path).unwrap();
    let lines = read_new_lines(path, file_state);

    if !lines.is_empty() {
        state.state_manager.on_data_received(&agent_id);
    }

    for line in lines {
        let events = transcript_parser::parse_jsonl_line(&line, had_tools);
        state.state_manager.process_events(&agent_id, agent_type.as_deref(), events);
    }
}

/// Start watching a project directory for JSONL changes.
/// Cancels any previously running watcher first.
/// Returns the shared state handle for cleanup.
pub fn start_watching(
    app_handle: AppHandle,
    project_dir: &str,
) -> Result<Arc<Mutex<WatcherState>>, String> {
    // Cancel previous watcher before starting a new one
    stop_watching_inner();

    let watch_dir = find_claude_project_dir(project_dir)
        .ok_or_else(|| "Could not find Claude project directory".to_string())?;

    println!(
        "[AgentRoom] Watching directory: {}",
        watch_dir.display()
    );

    let shared = Arc::new(Mutex::new(WatcherState::new(app_handle)));

    // Create a fresh cancel flag for this watcher session
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = cancel_store().lock().unwrap();
        *guard = Some(Arc::clone(&cancel));
    }

    // Initial scan to pick up existing JSONL files
    scan_and_process(&watch_dir, &shared);

    // Start filesystem watcher
    let shared_notify = Arc::clone(&shared);
    let cancel_notify = Arc::clone(&cancel);
    let watch_dir_notify = watch_dir.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| {
            if cancel_notify.load(Ordering::Relaxed) {
                return;
            }
            if let Ok(event) = result {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in &event.paths {
                            process_file_event(path, &shared_notify);
                        }
                    }
                    _ => {}
                }
            }
        },
        notify::Config::default(),
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    watcher
        .watch(&watch_dir_notify, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to start watching: {}", e))?;

    // Spawn background tasks:
    // 1. Polling fallback (1s interval) — catches events the watcher misses
    // 2. Timer tick (500ms) — checks permission/text-idle timers
    let shared_poll = Arc::clone(&shared);
    let shared_timer = Arc::clone(&shared);
    let cancel_poll = Arc::clone(&cancel);
    let cancel_timer = Arc::clone(&cancel);
    let watch_dir_poll = watch_dir.clone();

    tauri::async_runtime::spawn(async move {
        // Keep watcher alive until cancelled
        let _watcher = watcher;
        loop {
            if cancel_poll.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            if cancel_poll.load(Ordering::Relaxed) {
                break;
            }
            scan_and_process(&watch_dir_poll, &shared_poll);
        }
    });

    tauri::async_runtime::spawn(async move {
        loop {
            if cancel_timer.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            if cancel_timer.load(Ordering::Relaxed) {
                break;
            }
            let mut state = shared_timer.lock().unwrap();
            state.state_manager.tick_timers();
        }
    });

    Ok(shared)
}
