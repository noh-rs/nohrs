use super::indexer::IndexManager;
use super::watcher::FileWatcher;
use super::{SearchBackend, SearchResult, SearchScope};
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};

/// Deferred initial-indexing work handed to the caller so it can run on GPUI's
/// background executor (`cx.background_spawn`) instead of a `tokio::task::
/// spawn_blocking` owned by the service (async-runtime.md §2). It carries only
/// `Send` handles so the GUI can move it onto a worker thread.
pub struct InitialIndexingJob {
    index_manager: Arc<IndexManager>,
    progress_tx: postage::watch::Sender<f32>,
}

impl InitialIndexingJob {
    /// Runs initial indexing if the index is empty or its schema is outdated.
    /// Synchronous and blocking — intended to be driven by `cx.background_spawn`.
    pub fn run(self) {
        let InitialIndexingJob {
            index_manager,
            mut progress_tx,
        } = self;

        // Check if schema has required fields (detects schema changes)
        let schema = index_manager.index().schema();
        let has_filename_field = schema.get_field("filename").is_ok();

        if !has_filename_field {
            tracing::info!("Schema outdated (missing filename field), forcing full indexing...");
            *progress_tx.borrow_mut() = 0.0;
            if let Err(e) = index_manager.index_home(Some(progress_tx)) {
                tracing::error!("Initial indexing failed: {}", e);
            }
            return;
        }

        // Check if index already has documents
        match index_manager.index().reader() {
            Ok(reader) => {
                let doc_count = reader.searcher().num_docs();
                if doc_count == 0 {
                    tracing::info!("Index is empty, starting full indexing...");
                    *progress_tx.borrow_mut() = 0.0; // Reset to 0 for indexing
                    if let Err(e) = index_manager.index_home(Some(progress_tx)) {
                        tracing::error!("Initial indexing failed: {}", e);
                    }
                } else {
                    tracing::info!(
                        "Index already has {} documents, skipping initial indexing",
                        doc_count
                    );
                    // Progress stays at 1.0 (done)
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read index, running full indexing: {}", e);
                *progress_tx.borrow_mut() = 0.0;
                if let Err(e) = index_manager.index_home(Some(progress_tx)) {
                    tracing::error!("Initial indexing failed: {}", e);
                }
            }
        }
    }
}

/// Coordinates the home-directory index, its file watcher, and the root-scope backend.
pub struct SearchEngine {
    index_manager: Arc<IndexManager>,
    root_backend: Arc<dyn SearchBackend>,
    _watcher: FileWatcher, // Keep alive
    // The consumer thread exits on its own once `_watcher` drops and closes the
    // channel; declared after `_watcher` so that drop order makes that happen.
    _watcher_task: std::thread::JoinHandle<()>,
    progress_rx: postage::watch::Receiver<f32>,
    // Taken once by `take_initial_indexing_job`; `None` afterwards.
    initial_indexing_job: Mutex<Option<InitialIndexingJob>>,
}

impl SearchEngine {
    /// Builds the engine, opening the index and starting the file watcher.
    pub fn new() -> Result<Self> {
        let index_manager = Arc::new(IndexManager::new()?);

        #[cfg(target_os = "macos")]
        let root_backend: Arc<dyn SearchBackend> =
            Arc::new(super::spotlight::SpotlightBackend::new());

        #[cfg(not(target_os = "macos"))]
        let root_backend: Arc<dyn SearchBackend> = Arc::new(super::ripgrep::RipgrepBackend::new(
            std::path::PathBuf::from("/"),
        ));

        // Bounded channel for watcher events (async-channel; runtime-agnostic).
        let (tx, rx) = async_channel::bounded(100);

        let home_dir = dirs::home_dir().context("Home directory not found")?;
        use std::time::Duration;
        let watcher = FileWatcher::new(home_dir, tx, Duration::from_secs(2))?;

        // The watcher consumer does blocking index updates, so it runs on a
        // dedicated std::thread rather than an async task (async-runtime.md §4).
        // `recv_blocking` returns `Err` once every sender drops (i.e. when the
        // watcher above is dropped), which lets the thread exit cleanly.
        let manager_clone = index_manager.clone();
        let watcher_task = std::thread::spawn(move || {
            while let Ok(paths) = rx.recv_blocking() {
                if let Err(e) = manager_clone.process_changes(&paths) {
                    tracing::warn!("Failed to process batch changes: {}", e);
                }
            }
        });

        // Progress channel for initial indexing (starts at 1.0 == done).
        let (progress_tx, progress_rx) = postage::watch::channel_with(1.0);

        // The actual indexing is deferred to the caller (the GUI) so it runs on
        // GPUI's background executor instead of a service-owned spawn_blocking.
        let initial_indexing_job = InitialIndexingJob {
            index_manager: index_manager.clone(),
            progress_tx,
        };

        Ok(Self {
            index_manager,
            root_backend,
            _watcher: watcher,
            _watcher_task: watcher_task,
            progress_rx,
            initial_indexing_job: Mutex::new(Some(initial_indexing_job)),
        })
    }

    /// Returns a receiver for initial-indexing progress in the range `0.0..=1.0`.
    pub fn progress_subscription(&self) -> postage::watch::Receiver<f32> {
        self.progress_rx.clone()
    }

    /// Returns a shared handle to the underlying index manager.
    pub fn index_manager(&self) -> Arc<IndexManager> {
        self.index_manager.clone()
    }

    /// Hands off the one-shot initial-indexing job. Returns `None` if it has
    /// already been taken. The caller runs `job.run()` on a background executor.
    pub fn take_initial_indexing_job(&self) -> Option<InitialIndexingJob> {
        match self.initial_indexing_job.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => {
                tracing::error!("initial indexing job lock poisoned: {poisoned}");
                None
            }
        }
    }

    /// Dispatches a search to the appropriate backend. Both backends are
    /// synchronous, so callers should invoke this from `cx.background_spawn` to
    /// keep the UI thread responsive (replaces the former spawn_blocking).
    #[tracing::instrument(target = "nohrs::op", name = "search.query", level = "debug", skip_all, fields(query = %query, scope = ?scope))]
    pub fn search(&self, query: String, scope: SearchScope) -> Result<Vec<SearchResult>> {
        match scope {
            SearchScope::Home => self.index_manager.search(&query),
            SearchScope::Root => self.root_backend.search(&query),
        }
    }
}
