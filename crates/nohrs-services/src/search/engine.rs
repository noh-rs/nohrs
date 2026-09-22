use super::control::{InProcess, IndexControl};
use super::indexer::{IndexReader, Refresh};
use super::scoped::{self, Engine, Options};
use super::{SearchBackend, SearchResult, SearchScope};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// Deferred initial-indexing work handed to the caller so it can run on GPUI's
/// background executor (`cx.background_spawn`) instead of a `tokio::task::
/// spawn_blocking` owned by the service (async-runtime.md §2). It carries only
/// `Send` handles so the GUI can move it onto a worker thread.
pub struct InitialIndexingJob {
    control: Arc<dyn IndexControl>,
    progress_tx: postage::watch::Sender<f32>,
}

impl InitialIndexingJob {
    /// Brings the index up to date with the tree at startup.
    /// Synchronous and blocking — intended to be driven by `cx.background_spawn`.
    ///
    /// Runs on every launch rather than only when the index is empty. A watcher
    /// can only see changes made while it is running, so what happened between
    /// quitting and launching again — a checkout, a download, an editor
    /// session — reached the index nowhere else, and skipping the pass left
    /// those files answering with their old contents. The pass reads only what
    /// changed, so on a warm index it costs a walk and a `stat` per file.
    pub fn run(self) {
        let InitialIndexingJob {
            control,
            progress_tx,
        } = self;

        match control.refresh_reporting(Refresh::Changed, Some(progress_tx)) {
            Ok(report) => tracing::info!(
                "index up to date: {} written, {} unchanged, {} removed",
                report.indexed,
                report.unchanged,
                report.removed
            ),
            Err(error) => tracing::error!("Initial indexing failed: {error:#}"),
        }
    }
}

/// Reads the index, and asks whoever owns the writer to keep it current.
///
/// Reading and writing part company here. The reader is opened in this process
/// and answers from tantivy directly, so a search never waits on anything else;
/// the writing goes to an [`IndexControl`], which is `nohrs-indexd` where one
/// can be reached and this process where it cannot
/// ([ADR 0010](../../../../docs/adr/0010-indexd-owns-the-index-writer.md)).
pub struct SearchEngine {
    control: Arc<dyn IndexControl>,
    /// Held open for the life of the engine and reloaded when the writer says
    /// it has committed. `None` until something has built an index.
    reader: Mutex<Option<IndexReader>>,
    content_root: PathBuf,
    root_backend: Arc<dyn SearchBackend>,
    progress_rx: postage::watch::Receiver<f32>,
    // Taken once by `take_initial_indexing_job`; `None` afterwards.
    initial_indexing_job: Mutex<Option<InitialIndexingJob>>,
}

impl SearchEngine {
    /// Builds the engine with the writer in this process.
    pub fn new() -> Result<Self> {
        Self::with_control(Arc::new(InProcess::open_default()?))
    }

    /// Builds the engine around a writer someone else owns.
    pub fn with_control(control: Arc<dyn IndexControl>) -> Result<Self> {
        let status = control.status().context("cannot read the index's state")?;

        #[cfg(target_os = "macos")]
        let root_backend: Arc<dyn SearchBackend> =
            Arc::new(super::spotlight::SpotlightBackend::new());

        #[cfg(not(target_os = "macos"))]
        let root_backend: Arc<dyn SearchBackend> = Arc::new(super::ripgrep::RipgrepBackend::new(
            std::path::PathBuf::from("/"),
        ));

        // Progress starts at 1.0 == done, so a UI that reads it before any pass
        // has begun does not show a bar for work nobody asked for.
        let (progress_tx, progress_rx) = postage::watch::channel_with(1.0);
        let initial_indexing_job = InitialIndexingJob {
            control: Arc::clone(&control),
            progress_tx,
        };

        Ok(Self {
            control,
            reader: Mutex::new(open_reader(&status.index_path, &status.content_root)),
            content_root: status.content_root,
            root_backend,
            progress_rx,
            initial_indexing_job: Mutex::new(Some(initial_indexing_job)),
        })
    }

    /// Returns a receiver for initial-indexing progress in the range `0.0..=1.0`.
    pub fn progress_subscription(&self) -> postage::watch::Receiver<f32> {
        self.progress_rx.clone()
    }

    /// The way to ask for the index to be brought up to date.
    pub fn control(&self) -> Arc<dyn IndexControl> {
        Arc::clone(&self.control)
    }

    /// Picks up whatever has been committed to the index since the last search.
    ///
    /// Called when the writer says it has committed. Cheap enough for that to
    /// be every commit: it swaps in the new segments and leaves any search
    /// already running to finish on the old ones.
    pub fn reload(&self) {
        let mut reader = self.reader.lock().unwrap_or_else(PoisonError::into_inner);
        match reader.as_ref() {
            Some(open) => {
                if let Err(error) = open.reload() {
                    tracing::warn!("cannot reload the index: {error:#}");
                }
            }
            // There was no index when this engine opened. There may be one now:
            // this is how the first build becomes searchable without a restart.
            None => {
                if let Ok(status) = self.control.status() {
                    *reader = open_reader(&status.index_path, &status.content_root);
                }
            }
        }
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
    /// keep the UI thread responsive.
    #[tracing::instrument(target = "nohrs::op", name = "search.query", level = "debug", skip_all, fields(query = %query, scope = ?scope))]
    pub fn search(&self, query: String, scope: SearchScope) -> Result<Vec<SearchResult>> {
        match scope {
            SearchScope::Home => self.search_home(&query),
            SearchScope::Root => self.root_backend.search(&query),
        }
    }

    /// Searches the indexed tree: the index picks the files, and the lines come
    /// from the files themselves.
    ///
    /// The same path `noh search` takes, so the terminal and the window answer
    /// alike — including the fallback to reading the files when the index
    /// cannot answer, which is what keeps the window useful before the first
    /// build has finished.
    fn search_home(&self, query: &str) -> Result<Vec<SearchResult>> {
        let reader = self.reader.lock().unwrap_or_else(PoisonError::into_inner);
        let options = Options {
            engine: Engine::Auto,
            ..Options::default()
        };
        let outcome = scoped::search_using(&self.content_root, query, &options, reader.as_ref())?;
        if let scoped::Answered::Walk(Some(reason)) = &outcome.answered_by {
            tracing::debug!("read the files rather than the index: {reason}");
        }
        Ok(outcome.results)
    }
}

/// Opens the index for reading, or reports why there is nothing to read yet.
fn open_reader(
    index_path: &std::path::Path,
    content_root: &std::path::Path,
) -> Option<IndexReader> {
    match IndexReader::open(index_path.to_path_buf(), content_root.to_path_buf()) {
        Ok(reader) => reader,
        Err(error) => {
            // Not fatal: searches read the files until an index exists.
            tracing::warn!(
                "cannot open the index at {}: {error:#}",
                index_path.display()
            );
            None
        }
    }
}
