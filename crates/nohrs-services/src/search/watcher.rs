use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::path::PathBuf;
use std::time::Duration;
// `notify` / `notify-debouncer-mini` are runtime-agnostic (they drive their own
// std::thread). The debounce callback runs on that thread, so it forwards
// batches over an `async-channel` sender with the blocking `send_blocking`
// (async-runtime.md §2/§4).
use async_channel::Sender;

/// Watches a directory tree and forwards debounced batches of changed paths.
pub struct FileWatcher {
    // Keep debouncer alive
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    /// Starts watching `root` recursively, sending debounced change batches on `tx`
    /// with the given debounce `timeout`.
    pub fn new(root: PathBuf, tx: Sender<Vec<PathBuf>>, timeout: Duration) -> Result<Self> {
        // Create debouncer with specified timeout
        let mut debouncer = new_debouncer(timeout, move |res: DebounceEventResult| {
            match res {
                Ok(events) => {
                    let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                    // We run on notify's own thread, so a blocking send is correct here.
                    if let Err(e) = tx.send_blocking(paths) {
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
