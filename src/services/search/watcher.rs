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
    pub fn new(root: PathBuf, tx: mpsc::Sender<Vec<PathBuf>>) -> Result<Self> {
        // Create debouncer with 2 seconds timeout (user didn't specify, but 2s is safe for indexing)
        let mut debouncer =
            new_debouncer(Duration::from_secs(2), move |res: DebounceEventResult| {
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
