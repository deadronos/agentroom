use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Auth {
    token: String,
    connected_collectors: Arc<RwLock<HashMap<String, std::time::Instant>>>,
}

impl Auth {
    pub fn new(token: String) -> Self {
        Self {
            token,
            connected_collectors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn validate_token(&self, token: &str) -> bool {
        self.token == token
    }

    pub async fn register_collector(&self, collector_id: String) {
        self.connected_collectors.write().await.insert(
            collector_id,
            std::time::Instant::now(),
        );
    }

    pub async fn heartbeat_collector(&self, collector_id: &str) {
        self.connected_collectors.write().await.insert(
            collector_id.to_string(),
            std::time::Instant::now(),
        );
    }

    pub async fn cleanup_stale_collectors(&self, timeout_secs: u64) -> Vec<String> {
        let now = std::time::Instant::now();
        let mut stale = Vec::new();
        let mut write = self.connected_collectors.write().await;
        
        write.retain(|id, last_seen| {
            if now.duration_since(*last_seen).as_secs() > timeout_secs {
                stale.push(id.clone());
                false
            } else {
                true
            }
        });
        stale
    }
}
