use super::types::ActiveSession;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum WatchType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct WatchPath {
    pub path: PathBuf,
    pub watch_type: WatchType,
    pub filter: Option<String>,
    pub recursive: bool,
}

pub trait SessionAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn watch_paths(&self) -> Vec<WatchPath>;
    fn active_sessions(&self, threshold_ms: u64) -> Vec<ActiveSession>;
    fn session_detail(&self, session_id: &str) -> Option<ActiveSession>;
}
