mod agent_state;
mod commands;
mod file_watcher;
mod transcript_parser;

use commands::{
    cass_health, cass_index, cass_search, cass_sessions, cass_transcript,
    get_active_agents, load_tags, load_visual_layout, read_codexbar_snapshot,
    resolve_claude_workspace, resolve_gemini_resume_target, resolve_gemini_workspace,
    run_osascript, save_tag, save_visual_layout, start_watching, stop_watching, tag_session,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            cass_search,
            cass_sessions,
            cass_transcript,
            cass_index,
            cass_health,
            load_tags,
            save_tag,
            tag_session,
            resolve_claude_workspace,
            resolve_gemini_resume_target,
            resolve_gemini_workspace,
            run_osascript,
            start_watching,
            stop_watching,
            get_active_agents,
            save_visual_layout,
            load_visual_layout,
            read_codexbar_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
