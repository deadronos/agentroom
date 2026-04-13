use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct SessionWatcher {
    dirty: Arc<Mutex<bool>>,
    watch_paths: Arc<Mutex<HashSet<PathBuf>>>,
    watchers: Vec<RecommendedWatcher>, // Must store to keep alive
}

impl SessionWatcher {
    pub fn new() -> Self {
        Self {
            dirty: Arc::new(Mutex::new(true)), // Start dirty so initial flush collects sessions
            watch_paths: Arc::new(Mutex::new(HashSet::new())),
            watchers: Vec::new(),
        }
    }

    pub fn watch(&mut self, path: PathBuf, recursive: bool) -> anyhow::Result<()> {
        let dirty = self.dirty.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            *dirty.lock() = true;
                        }
                        _ => {}
                    }
                }
            },
            NotifyConfig::default().with_poll_interval(Duration::from_secs(1)),
        )?;

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher.watch(&path, mode)?;

        self.watch_paths.lock().insert(path);
        self.watchers.push(watcher); // Keep watcher alive
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        *self.dirty.lock()
    }

    pub fn clear_dirty(&self) {
        *self.dirty.lock() = false;
    }
}
