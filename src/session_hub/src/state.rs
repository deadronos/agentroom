use session_common::{ActiveSession, HubMessage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Clone)]
pub struct HubState {
    collector_snapshots: Arc<RwLock<HashMap<String, session_common::Snapshot>>>,
    merged_sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
    /// Broadcast channel for sending state updates to all connected frontends
    frontend_broadcast: broadcast::Sender<HubMessage>,
}

impl HubState {
    pub fn new() -> Self {
        let (bcast, _) = broadcast::channel(16);
        Self {
            collector_snapshots: Arc::new(RwLock::new(HashMap::new())),
            merged_sessions: Arc::new(RwLock::new(HashMap::new())),
            frontend_broadcast: bcast,
        }
    }

    /// Subscribe to frontend broadcast messages. Returns a receiver handle.
    pub fn subscribe_frontend(&self) -> broadcast::Receiver<HubMessage> {
        self.frontend_broadcast.subscribe()
    }

    /// Broadcast current session state to all connected frontends.
    pub async fn broadcast_state(&self) {
        let sessions = self.get_all_sessions().await;
        let sync = HubMessage::StateSync { sessions };
        let _ = self.frontend_broadcast.send(sync);
    }

    pub async fn apply_snapshot(&self, snapshot: session_common::Snapshot) -> SessionDiff {
        let old_session_ids = self.merged_sessions.read().await.keys().cloned().collect::<HashSet<_>>();

        self.collector_snapshots.write().await.insert(
            snapshot.collector_id.clone(),
            snapshot.clone(),
        );

        let mut write = self.merged_sessions.write().await;
        for session in snapshot.sessions {
            let should_update = match write.get(&session.session_id) {
                Some(existing) => session.last_activity > existing.last_activity,
                None => true,
            };
            if should_update {
                write.insert(session.session_id.clone(), session);
            }
        }

        let new_session_ids = write.keys().cloned().collect::<HashSet<_>>();

        let started: Vec<_> = new_session_ids.difference(&old_session_ids).cloned().collect();
        let ended: Vec<_> = old_session_ids.difference(&new_session_ids).cloned().collect();
        let existing: Vec<_> = old_session_ids.intersection(&new_session_ids).cloned().collect();

        // Broadcast session_started events for new sessions
        for session_id in &started {
            if let Some(session) = write.get(session_id) {
                let msg = HubMessage::SessionStarted {
                    session_id: session.session_id.clone(),
                    provider: session.provider.clone(),
                    project: session.project.clone(),
                    model: session.model.clone(),
                    timestamp: session.last_activity,
                    last_tool: session.last_tool.clone(),
                    last_message: session.last_message.clone(),
                    agent_id: session.agent_id.clone(),
                    agent_type: session.agent_type.clone(),
                };
                let _ = self.frontend_broadcast.send(msg);
            }
        }

        // Broadcast session_ended events for ended sessions
        for session_id in &ended {
            let provider = write.get(session_id)
                .map(|s| s.provider.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let msg = HubMessage::SessionEnded {
                session_id: session_id.clone(),
                provider,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64,
            };
            let _ = self.frontend_broadcast.send(msg);
        }

        SessionDiff { started, ended, existing }
    }

    pub async fn get_all_sessions(&self) -> Vec<ActiveSession> {
        self.merged_sessions.read().await.values().cloned().collect()
    }

    pub async fn remove_collector(&self, collector_id: &str) {
        self.collector_snapshots.write().await.remove(collector_id);
        
        let remaining: Vec<_> = self.collector_snapshots.read().await.values().cloned().collect();
        let mut write = self.merged_sessions.write().await;
        write.clear();
        for snapshot in remaining {
            for session in snapshot.sessions {
                let should_update = match write.get(&session.session_id) {
                    Some(existing) => session.last_activity > existing.last_activity,
                    None => true,
                };
                if should_update {
                    write.insert(session.session_id.clone(), session);
                }
            }
        }
    }
}

pub struct SessionDiff {
    pub started: Vec<String>,
    pub ended: Vec<String>,
    #[allow(dead_code)]
    pub existing: Vec<String>,
}
