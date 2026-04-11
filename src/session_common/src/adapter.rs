use super::types::ActiveSession;
use std::path::PathBuf;

#[derive(Debug, Clone)]
#[doc = "The type of path to watch for session detection."]
pub enum WatchType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
#[doc = "A path to monitor for session activity, with configuration."]
pub struct WatchPath {
    pub path: PathBuf,
    pub watch_type: WatchType,
    pub filter: Option<String>,
    pub recursive: bool,
}

#[doc = "Trait for adapters that detect and track agent sessions."]
pub trait SessionAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn watch_paths(&self) -> Vec<WatchPath>;
    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession>;
    fn session_detail(&self, session_id: &str) -> Option<ActiveSession>;
}
