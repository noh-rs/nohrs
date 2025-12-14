use super::indexer::IndexManager;
use super::ripgrep::RipgrepBackend;
use super::watcher::FileWatcher;
use super::{SearchBackend, SearchResult, SearchScope};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct SearchEngine {
    index_manager: Arc<IndexManager>,
    ripgrep_backend: Arc<RipgrepBackend>,
    _watcher: FileWatcher, // Keep alive
    _watcher_task: JoinHandle<()>,
    progress_rx: tokio::sync::watch::Receiver<f32>,
}

impl SearchEngine {
    pub async fn new() -> Result<Self> {
        let index_manager = Arc::new(IndexManager::new()?);
        let ripgrep_backend = Arc::new(RipgrepBackend::new(std::path::PathBuf::from("/")));

        // Channel for watcher events
        let (tx, mut rx) = mpsc::channel(100);

        let home_dir = dirs::home_dir().expect("Home dir not found");
        use std::time::Duration;
        let watcher = FileWatcher::new(home_dir, tx, Duration::from_secs(2))?;

        // Spawn event handler task
        let manager_clone = index_manager.clone();
        let watcher_task = tokio::spawn(async move {
            while let Some(paths) = rx.recv().await {
                if let Err(e) = manager_clone.process_changes(&paths) {
                    tracing::warn!("Failed to process batch changes: {}", e);
                }
            }
        });

        // Trigger initial indexing in background
        // specific to Home scope currently
        let (progress_tx, progress_rx) = tokio::sync::watch::channel(0.0);
        let initial_manager = index_manager.clone();
        tokio::task::spawn_blocking(move || {
            // TODO: Check if index is empty or needs refresh?
            // For now, simple walk ensures consistent state.
            if let Err(e) = initial_manager.index_home(Some(progress_tx)) {
                tracing::error!("Initial indexing failed: {}", e);
            }
        });

        Ok(Self {
            index_manager,
            ripgrep_backend,
            _watcher: watcher,
            _watcher_task: watcher_task,
            progress_rx,
        })
    }

    pub fn progress_subscription(&self) -> tokio::sync::watch::Receiver<f32> {
        self.progress_rx.clone()
    }

    pub fn index_manager(&self) -> Arc<IndexManager> {
        self.index_manager.clone()
    }

    pub async fn search(&self, query: String, scope: SearchScope) -> Result<Vec<SearchResult>> {
        // Dispatches search to appropriate backend.
        // Doing this async to not block UI (though underlying backend is sync mostly, we can spawn_blocking if needed).
        // Since both backends have synchronous search methods currently, we should wrap in spawn_blocking.

        let index_manager = self.index_manager.clone();
        let ripgrep_backend = self.ripgrep_backend.clone();

        tokio::task::spawn_blocking(move || match scope {
            SearchScope::Home => index_manager.as_ref().search(&query),
            SearchScope::Root => ripgrep_backend.as_ref().search(&query),
        })
        .await?
    }
}
