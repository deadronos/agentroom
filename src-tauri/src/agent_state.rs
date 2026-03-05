//! Agent state manager — tracks per-agent tool activity and emits Tauri events.
//!
//! Ports timerManager.ts + agent state tracking from Pixel Agents to Rust.
//! Each agent has tool tracking, permission timers, and text-idle detection.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;
use tauri::AppHandle;
use tauri::Emitter;

use crate::transcript_parser::AgentEvent;

/// Timing constants (matching Pixel Agents).
const TOOL_DONE_DELAY_MS: u64 = 300;
const PERMISSION_TIMER_DELAY_MS: u64 = 7000;
const TEXT_IDLE_DELAY_MS: u64 = 5000;

/// Tools exempt from permission detection.
fn is_permission_exempt(tool_name: &str) -> bool {
    matches!(tool_name, "Task" | "AskUserQuestion")
}

/// Payload emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize)]
pub struct AgentStatePayload {
    pub agent_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subagent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

/// Per-agent state.
pub struct AgentState {
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub active_tool_ids: HashSet<String>,
    pub active_tool_names: HashMap<String, String>,
    /// Sub-agent tool tracking: parent_tool_id → Set<sub_tool_id>
    pub subagent_tool_ids: HashMap<String, HashSet<String>>,
    pub subagent_tool_names: HashMap<String, HashMap<String, String>>,
    pub is_waiting: bool,
    pub permission_sent: bool,
    pub had_tools_in_turn: bool,
    /// Timestamp of last activity (for timers).
    pub last_activity: Instant,
    /// Timestamp when permission timer started (None = no timer).
    pub permission_timer_start: Option<Instant>,
    /// Timestamp when text-idle timer started.
    pub text_idle_timer_start: Option<Instant>,
}

impl AgentState {
    pub fn new(agent_id: String, agent_type: Option<String>) -> Self {
        Self {
            agent_id,
            agent_type,
            active_tool_ids: HashSet::new(),
            active_tool_names: HashMap::new(),
            subagent_tool_ids: HashMap::new(),
            subagent_tool_names: HashMap::new(),
            is_waiting: false,
            permission_sent: false,
            had_tools_in_turn: false,
            last_activity: Instant::now(),
            permission_timer_start: None,
            text_idle_timer_start: None,
        }
    }
}

/// Manages all agent states and emits events to the frontend.
pub struct AgentStateManager {
    pub agents: HashMap<String, AgentState>,
    app_handle: AppHandle,
}

impl AgentStateManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            agents: HashMap::new(),
            app_handle,
        }
    }

    /// Ensure an agent exists in the map.
    fn ensure_agent(&mut self, agent_id: &str, agent_type: Option<&str>) -> &mut AgentState {
        if !self.agents.contains_key(agent_id) {
            self.agents.insert(
                agent_id.to_string(),
                AgentState::new(agent_id.to_string(), agent_type.map(|s| s.to_string())),
            );
        } else if agent_type.is_some() {
            let agent = self.agents.get_mut(agent_id).unwrap();
            if agent.agent_type.is_none() {
                agent.agent_type = agent_type.map(|s| s.to_string());
            }
        }
        self.agents.get_mut(agent_id).unwrap()
    }

    /// Emit an event to the frontend, injecting agent_type from stored state.
    fn emit(&self, agent_id: &str, mut payload: AgentStatePayload) {
        if let Some(agent) = self.agents.get(agent_id) {
            payload.agent_type = agent.agent_type.clone();
        }
        let _ = self.app_handle.emit("agent-state-changed", &payload);
    }

    /// Process a batch of AgentEvents from parsing a JSONL line.
    pub fn process_events(&mut self, agent_id: &str, agent_type: Option<&str>, events: Vec<AgentEvent>) {
        // Ensure agent exists with type before processing events
        self.ensure_agent(agent_id, agent_type);
        for event in events {
            self.process_event(agent_id, event);
        }
    }

    fn process_event(&mut self, agent_id: &str, event: AgentEvent) {
        match event {
            AgentEvent::ToolStart {
                tool_id,
                tool_name,
                status,
                is_subagent,
                parent_tool_id,
            } => {
                let agent = self.ensure_agent(agent_id, None);
                agent.last_activity = Instant::now();
                agent.is_waiting = false;
                agent.text_idle_timer_start = None;

                if is_subagent {
                    if let Some(ref parent_id) = parent_tool_id {
                        agent
                            .subagent_tool_ids
                            .entry(parent_id.clone())
                            .or_default()
                            .insert(tool_id.clone());
                        agent
                            .subagent_tool_names
                            .entry(parent_id.clone())
                            .or_default()
                            .insert(tool_id.clone(), tool_name.clone());
                    }
                } else {
                    agent.had_tools_in_turn = true;
                    agent.active_tool_ids.insert(tool_id.clone());
                    agent.active_tool_names.insert(tool_id.clone(), tool_name.clone());
                }

                // Start permission timer for non-exempt tools
                if !is_permission_exempt(&tool_name) {
                    agent.permission_timer_start = Some(Instant::now());
                }

                self.emit(agent_id, AgentStatePayload {
                    agent_id: agent_id.to_string(),
                    status: "tool_start".to_string(),
                    tool_name: Some(tool_name),
                    tool_id: Some(tool_id),
                    tool_status: Some(status),
                    is_subagent: if is_subagent { Some(true) } else { None },
                    parent_tool_id,
                    agent_type: None,
                });
            }

            AgentEvent::ToolDone {
                tool_id,
                is_subagent,
                parent_tool_id,
            } => {
                let agent = self.ensure_agent(agent_id, None);
                agent.last_activity = Instant::now();

                if is_subagent {
                    if let Some(ref parent_id) = parent_tool_id {
                        if let Some(sub_tools) = agent.subagent_tool_ids.get_mut(parent_id) {
                            sub_tools.remove(&tool_id);
                        }
                        if let Some(sub_names) = agent.subagent_tool_names.get_mut(parent_id) {
                            sub_names.remove(&tool_id);
                        }
                    }
                } else {
                    // If completed tool was a Task, clear its subagent tracking
                    if agent.active_tool_names.get(&tool_id).map(|n| n.as_str()) == Some("Task") {
                        agent.subagent_tool_ids.remove(&tool_id);
                        agent.subagent_tool_names.remove(&tool_id);
                    }
                    agent.active_tool_ids.remove(&tool_id);
                    agent.active_tool_names.remove(&tool_id);

                    // If all tools completed, allow text-idle detection
                    if agent.active_tool_ids.is_empty() {
                        agent.had_tools_in_turn = false;
                    }
                }

                // Emit tool_done (in production, this would be delayed by TOOL_DONE_DELAY_MS)
                // For simplicity, emit immediately — the frontend handles the visual transition
                let _ = TOOL_DONE_DELAY_MS; // acknowledge constant exists
                self.emit(agent_id, AgentStatePayload {
                    agent_id: agent_id.to_string(),
                    status: "tool_done".to_string(),
                    tool_name: None,
                    tool_id: Some(tool_id),
                    tool_status: None,
                    is_subagent: if is_subagent { Some(true) } else { None },
                    parent_tool_id,
                    agent_type: None,
                });
            }

            AgentEvent::TurnEnd => {
                let agent = self.ensure_agent(agent_id, None);
                agent.last_activity = Instant::now();

                // Clear all tools
                agent.active_tool_ids.clear();
                agent.active_tool_names.clear();
                agent.subagent_tool_ids.clear();
                agent.subagent_tool_names.clear();
                agent.is_waiting = true;
                agent.permission_sent = false;
                agent.had_tools_in_turn = false;
                agent.permission_timer_start = None;
                agent.text_idle_timer_start = None;

                self.emit(agent_id, AgentStatePayload {
                    agent_id: agent_id.to_string(),
                    status: "turn_end".to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_status: None,
                    is_subagent: None,
                    parent_tool_id: None,
                    agent_type: None,
                });
            }

            AgentEvent::Active => {
                let agent = self.ensure_agent(agent_id, None);
                agent.last_activity = Instant::now();
                agent.is_waiting = false;
                agent.text_idle_timer_start = None;

                // Clear permission state on new activity
                if agent.permission_sent {
                    agent.permission_sent = false;
                    self.emit(agent_id, AgentStatePayload {
                        agent_id: agent_id.to_string(),
                        status: "permission_clear".to_string(),
                        tool_name: None,
                        tool_id: None,
                        tool_status: None,
                        is_subagent: None,
                        parent_tool_id: None,
                        agent_type: None,
                    });
                }

                self.emit(agent_id, AgentStatePayload {
                    agent_id: agent_id.to_string(),
                    status: "active".to_string(),
                    tool_name: None,
                    tool_id: None,
                    tool_status: None,
                    is_subagent: None,
                    parent_tool_id: None,
                    agent_type: None,
                });
            }

            AgentEvent::TextIdle => {
                let agent = self.ensure_agent(agent_id, None);
                agent.text_idle_timer_start = Some(Instant::now());
                // The actual idle emission happens in tick_timers()
            }
        }
    }

    /// Called periodically to check timer expirations.
    pub fn tick_timers(&mut self) {
        let now = Instant::now();
        let mut emissions: Vec<AgentStatePayload> = Vec::new();

        for agent in self.agents.values_mut() {
            // Permission timer check
            if let Some(start) = agent.permission_timer_start {
                if now.duration_since(start).as_millis() >= PERMISSION_TIMER_DELAY_MS as u128 {
                    agent.permission_timer_start = None;

                    // Check if there are still active non-exempt tools
                    let has_non_exempt = agent
                        .active_tool_names
                        .values()
                        .any(|name| !is_permission_exempt(name));

                    // Check sub-agent tools too
                    let has_non_exempt_sub = agent.subagent_tool_names.values().any(|sub_names| {
                        sub_names.values().any(|name| !is_permission_exempt(name))
                    });

                    if has_non_exempt || has_non_exempt_sub {
                        agent.permission_sent = true;
                        emissions.push(AgentStatePayload {
                            agent_id: agent.agent_id.clone(),
                            status: "permission".to_string(),
                            tool_name: None,
                            tool_id: None,
                            tool_status: None,
                            is_subagent: None,
                            parent_tool_id: None,
                            agent_type: None,
                        });
                    }
                }
            }

            // Text-idle timer check
            if let Some(start) = agent.text_idle_timer_start {
                if now.duration_since(start).as_millis() >= TEXT_IDLE_DELAY_MS as u128 {
                    agent.text_idle_timer_start = None;
                    agent.is_waiting = true;
                    emissions.push(AgentStatePayload {
                        agent_id: agent.agent_id.clone(),
                        status: "text_idle".to_string(),
                        tool_name: None,
                        tool_id: None,
                        tool_status: None,
                        is_subagent: None,
                        parent_tool_id: None,
                        agent_type: None,
                    });
                }
            }
        }

        for payload in emissions {
            let aid = payload.agent_id.clone();
            self.emit(&aid, payload);
        }
    }

    /// Called when new JSONL data arrives — cancels permission/waiting timers.
    pub fn on_data_received(&mut self, agent_id: &str) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.last_activity = Instant::now();
            agent.permission_timer_start = None;
            agent.text_idle_timer_start = None;

            if agent.permission_sent {
                agent.permission_sent = false;
                let agent_type = agent.agent_type.clone();
                let _ = self.app_handle.emit(
                    "agent-state-changed",
                    &AgentStatePayload {
                        agent_id: agent_id.to_string(),
                        status: "permission_clear".to_string(),
                        tool_name: None,
                        tool_id: None,
                        tool_status: None,
                        is_subagent: None,
                        parent_tool_id: None,
                        agent_type,
                    },
                );
            }
        }
    }
}
