use session_common::{ActiveSession, Snapshot};
use sha1::{Sha1, Digest};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct HubClient {
    url: String,
    sender: mpsc::Sender<Snapshot>,
}

impl HubClient {
    pub fn new(url: String, sender: mpsc::Sender<Snapshot>) -> Self {
        Self { url, sender }
    }

    pub async fn send_snapshot(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        self.sender.send(snapshot).await?;
        Ok(())
    }
}

pub struct Collector {
    adapters: Vec<Box<dyn session_common::SessionAdapter + Send + Sync>>,
    watcher: SessionWatcher,
    hub_client: HubClient,
    collector_id: String,
    last_fingerprint: Option<String>,
}

use crate::watcher::SessionWatcher;

impl Collector {
    pub fn new(
        adapters: Vec<Box<dyn session_common::SessionAdapter + Send + Sync>>,
        hub_client: HubClient,
        collector_id: String,
    ) -> Self {
        Self {
            adapters,
            watcher: SessionWatcher::new(),
            hub_client,
            collector_id,
            last_fingerprint: None,
        }
    }

    pub fn setup_watchers(&mut self) -> anyhow::Result<()> {
        for adapter in &self.adapters {
            if !adapter.is_available() {
                continue;
            }
            for wp in adapter.watch_paths() {
                self.watcher.watch(wp.path, wp.recursive)?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self, flush_interval_ms: u64) -> anyhow::Result<()> {
        loop {
            tokio::time::sleep(Duration::from_millis(flush_interval_ms)).await;
            self.flush_if_needed().await?;
        }
    }

    pub async fn flush_if_needed(&mut self) -> anyhow::Result<()> {
        if !self.watcher.is_dirty() {
            return Ok(());
        }

        let sessions = self.collect_sessions();
        let fingerprint = self.compute_fingerprint(&sessions);

        if Some(&fingerprint) == self.last_fingerprint.as_ref() {
            self.watcher.clear_dirty();
            return Ok(());
        }

        let snapshot = Snapshot {
            collector_id: self.collector_id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            fingerprint: fingerprint.clone(),
            sessions,
        };

        self.hub_client.send_snapshot(snapshot).await?;
        self.last_fingerprint = Some(fingerprint);
        self.watcher.clear_dirty();
        Ok(())
    }

    fn collect_sessions(&self) -> Vec<ActiveSession> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let threshold = now - 120000;
        let mut all_sessions = Vec::new();

        for adapter in &self.adapters {
            if !adapter.is_available() {
                continue;
            }
            let sessions = adapter.active_sessions(120000);
            for session in sessions {
                if session.last_activity >= threshold {
                    all_sessions.push(session);
                }
            }
        }
        all_sessions
    }

    fn compute_fingerprint(&self, sessions: &[ActiveSession]) -> String {
        let json = serde_json::to_string(sessions).unwrap_or_default();
        let mut hasher = Sha1::new();
        hasher.update(json.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}