use session_common::ActiveSession;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct HubState {
    collector_snapshots: Arc<RwLock<HashMap<String, session_common::Snapshot>>>,
    merged_sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            collector_snapshots: Arc::new(RwLock::new(HashMap::new())),
            merged_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
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
