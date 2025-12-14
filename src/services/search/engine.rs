use super::indexer::IndexManager;
use super::watcher::FileWatcher;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct SearchEngine {
    index_manager: Arc<IndexManager>,
    _watcher: FileWatcher, // Keep alive
    _watcher_task: JoinHandle<()>,
}

impl SearchEngine {
    pub async fn new() -> Result<Self> {
        let index_manager = Arc::new(IndexManager::new()?);

        // Channel for watcher events
        let (tx, mut rx) = mpsc::channel(100);

        let home_dir = dirs::home_dir().expect("Home dir not found");
        let watcher = FileWatcher::new(home_dir, tx)?;

        // Spawn event handler task
        let manager_clone = index_manager.clone();
        let watcher_task = tokio::spawn(async move {
            while let Some(paths) = rx.recv().await {
                for path in paths {
                    tracing::debug!("File changed: {:?}", path);
                    // Decide if add or remove. Notify doesn't easily distinguish move/rename in debounced events sometimes without details.
                    // But debounced events usually give just paths.
                    // If file exists, index it. If not, remove it.
                    if path.exists() {
                        // We need to expose index_single_file or add a public method for updating
                        // single file in IndexManager.
                        // Let's add `update_file(path)` to IndexManager.
                        if let Err(e) = manager_clone.update_file(&path) {
                            tracing::warn!("Failed to update index for {:?}: {}", path, e);
                        }
                    } else {
                        if let Err(e) = manager_clone.remove_file(&path) {
                            tracing::warn!("Failed to remove index for {:?}: {}", path, e);
                        }
                    }
                }
            }
        });

        Ok(Self {
            index_manager,
            _watcher: watcher,
            _watcher_task: watcher_task,
        })
    }

    pub fn index_manager(&self) -> Arc<IndexManager> {
        self.index_manager.clone()
    }
}
