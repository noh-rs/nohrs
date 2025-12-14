use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct FileWatcher {
    // Keep debouncer alive
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    pub fn new(root: PathBuf, tx: mpsc::Sender<Vec<PathBuf>>, timeout: Duration) -> Result<Self> {
        // Create debouncer with specified timeout
        let mut debouncer = new_debouncer(timeout, move |res: DebounceEventResult| {
            match res {
                Ok(events) => {
                    let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                    // Blocking send is fine here as we are in a separate thread managed by notify
                    // But wait, tx is tokio sender. We need blocking_send inside this sync closure.
                    if let Err(e) = tx.blocking_send(paths) {
                        tracing::warn!("Failed to send watcher events: {}", e);
                        // Receiver dropped, we can't do much.
                    }
                }
                Err(e) => {
                    tracing::warn!("Watcher error: {:?}", e);
                }
            }
        })?;

        debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
