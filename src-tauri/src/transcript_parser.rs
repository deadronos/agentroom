//! JSONL transcript parser — ports transcriptParser.ts logic to Rust.
//!
//! Each JSONL line from a Claude Code session is parsed into an AgentEvent
//! that drives the frontend's character state machine.

use serde_json::Value;
use std::path::Path;

/// Events emitted from parsing a single JSONL line.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    ToolStart {
        tool_id: String,
        tool_name: String,
        status: String,
        is_subagent: bool,
        parent_tool_id: Option<String>,
    },
    ToolDone {
        tool_id: String,
        is_subagent: bool,
        parent_tool_id: Option<String>,
    },
    TurnEnd,
    /// Active status (new user prompt starting a turn)
    Active,
    /// Text-only response with no tools — may trigger idle timer
    TextIdle,
}

const BASH_COMMAND_DISPLAY_MAX_LENGTH: usize = 30;
const TASK_DESCRIPTION_DISPLAY_MAX_LENGTH: usize = 40;

/// Truncate a string to at most `max_chars` characters (UTF-8 safe).
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Format a tool invocation into a human-readable status string.
fn format_tool_status(tool_name: &str, input: &Value) -> String {
    let basename = |v: &Value| -> String {
        v.as_str()
            .map(|s| {
                Path::new(s)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.to_string())
            })
            .unwrap_or_default()
    };

    match tool_name {
        "Read" => format!("Reading {}", basename(&input["file_path"])),
        "Edit" => format!("Editing {}", basename(&input["file_path"])),
        "Write" => format!("Writing {}", basename(&input["file_path"])),
        "Bash" => {
            let cmd = input["command"].as_str().unwrap_or("");
            if cmd.chars().count() > BASH_COMMAND_DISPLAY_MAX_LENGTH {
                format!("Running: {}\u{2026}", truncate_chars(cmd, BASH_COMMAND_DISPLAY_MAX_LENGTH))
            } else {
                format!("Running: {}", cmd)
            }
        }
        "Glob" => "Searching files".to_string(),
        "Grep" => "Searching code".to_string(),
        "WebFetch" => "Fetching web content".to_string(),
        "WebSearch" => "Searching the web".to_string(),
        "Task" => {
            let desc = input["description"].as_str().unwrap_or("");
            if desc.is_empty() {
                "Running subtask".to_string()
            } else if desc.chars().count() > TASK_DESCRIPTION_DISPLAY_MAX_LENGTH {
                format!(
                    "Subtask: {}\u{2026}",
                    truncate_chars(desc, TASK_DESCRIPTION_DISPLAY_MAX_LENGTH)
                )
            } else {
                format!("Subtask: {}", desc)
            }
        }
        "AskUserQuestion" => "Waiting for your answer".to_string(),
        "EnterPlanMode" => "Planning".to_string(),
        "NotebookEdit" => "Editing notebook".to_string(),
        _ => format!("Using {}", tool_name),
    }
}

/// Parse a single JSONL line and return zero or more AgentEvents.
pub fn parse_jsonl_line(line: &str, had_tools_in_turn: bool) -> Vec<AgentEvent> {
    let record: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let record_type = record["type"].as_str().unwrap_or("");
    let mut events = Vec::new();

    match record_type {
        "assistant" => {
            if let Some(content) = record["message"]["content"].as_array() {
                let has_tool_use = content.iter().any(|b| b["type"].as_str() == Some("tool_use"));

                if has_tool_use {
                    // Emit Active first
                    events.push(AgentEvent::Active);

                    for block in content {
                        if block["type"].as_str() == Some("tool_use") {
                            if let Some(id) = block["id"].as_str() {
                                let tool_name =
                                    block["name"].as_str().unwrap_or("").to_string();
                                let input = &block["input"];
                                let status = format_tool_status(&tool_name, input);
                                events.push(AgentEvent::ToolStart {
                                    tool_id: id.to_string(),
                                    tool_name,
                                    status,
                                    is_subagent: false,
                                    parent_tool_id: None,
                                });
                            }
                        }
                    }
                } else if !had_tools_in_turn
                    && content.iter().any(|b| b["type"].as_str() == Some("text"))
                {
                    // Text-only turn — may trigger text-idle timer
                    events.push(AgentEvent::TextIdle);
                }
            }
        }

        "user" => {
            if let Some(content) = record["message"]["content"].as_array() {
                let has_tool_result =
                    content.iter().any(|b| b["type"].as_str() == Some("tool_result"));

                if has_tool_result {
                    for block in content {
                        if block["type"].as_str() == Some("tool_result") {
                            if let Some(tool_use_id) = block["tool_use_id"].as_str() {
                                events.push(AgentEvent::ToolDone {
                                    tool_id: tool_use_id.to_string(),
                                    is_subagent: false,
                                    parent_tool_id: None,
                                });
                            }
                        }
                    }
                } else {
                    // New user prompt — new turn starting
                    events.push(AgentEvent::Active);
                }
            } else if record["message"]["content"].is_string() {
                // New user text prompt
                events.push(AgentEvent::Active);
            }
        }

        "system" => {
            if record["subtype"].as_str() == Some("turn_duration") {
                events.push(AgentEvent::TurnEnd);
            }
        }

        "progress" => {
            // Sub-agent tool events
            let parent_tool_id = record["parentToolUseID"].as_str();
            let data = &record["data"];

            if let Some(parent_id) = parent_tool_id {
                let data_type = data["type"].as_str().unwrap_or("");

                // bash_progress / mcp_progress: tool actively executing
                if data_type == "bash_progress" || data_type == "mcp_progress" {
                    // Just indicates tool is running, no event needed
                    return events;
                }

                // agent_progress: sub-agent tool start/done
                if let Some(msg) = data["message"].as_object() {
                    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let inner_content = msg
                        .get("message")
                        .and_then(|v| v.get("content"))
                        .and_then(|v| v.as_array());

                    if let Some(content) = inner_content {
                        if msg_type == "assistant" {
                            for block in content {
                                if block["type"].as_str() == Some("tool_use") {
                                    if let Some(id) = block["id"].as_str() {
                                        let tool_name =
                                            block["name"].as_str().unwrap_or("").to_string();
                                        let input = &block["input"];
                                        let status = format_tool_status(&tool_name, input);
                                        events.push(AgentEvent::ToolStart {
                                            tool_id: id.to_string(),
                                            tool_name,
                                            status,
                                            is_subagent: true,
                                            parent_tool_id: Some(parent_id.to_string()),
                                        });
                                    }
                                }
                            }
                        } else if msg_type == "user" {
                            for block in content {
                                if block["type"].as_str() == Some("tool_result") {
                                    if let Some(tool_use_id) = block["tool_use_id"].as_str() {
                                        events.push(AgentEvent::ToolDone {
                                            tool_id: tool_use_id.to_string(),
                                            is_subagent: true,
                                            parent_tool_id: Some(parent_id.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        _ => {}
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tool_status() {
        let input = serde_json::json!({"file_path": "/home/user/project/src/main.rs"});
        assert_eq!(format_tool_status("Read", &input), "Reading main.rs");

        let input = serde_json::json!({"command": "ls -la"});
        assert_eq!(format_tool_status("Bash", &input), "Running: ls -la");

        assert_eq!(format_tool_status("Glob", &Value::Null), "Searching files");
    }

    #[test]
    fn test_parse_turn_end() {
        let line = r#"{"type":"system","subtype":"turn_duration","duration_ms":1234}"#;
        let events = parse_jsonl_line(line, false);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AgentEvent::TurnEnd));
    }

    #[test]
    fn test_parse_tool_use() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":"Read","input":{"file_path":"/tmp/test.rs"}}]}}"#;
        let events = parse_jsonl_line(line, false);
        assert!(events.len() >= 2); // Active + ToolStart
        assert!(matches!(events[0], AgentEvent::Active));
        assert!(matches!(events[1], AgentEvent::ToolStart { .. }));
    }
}
