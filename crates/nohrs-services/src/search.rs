//! File search: a tantivy-backed full-text index over the home directory plus
//! a platform regex backend for whole-system scans.

/// Trait abstracting a search backend.
pub mod backend;
/// The search engine wiring together the index, watcher, and root backend.
pub mod engine;
/// Tantivy index management and incremental updates.
pub mod indexer;
/// Ripgrep/`grep`-crate based regex search backend (non-macOS root scans).
pub mod ripgrep;
/// macOS Spotlight (`mdfind`) based search backend.
pub mod spotlight;
/// Filesystem change watcher feeding incremental index updates.
pub mod watcher;

use std::path::PathBuf;

/// Which part of the filesystem a search covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// The indexed home directory.
    Home,
    /// The whole filesystem from root, via the platform regex backend.
    Root,
}

/// A single match: the file, the matching line number, and its content.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Path of the file containing the match.
    pub path: PathBuf,
    /// 1-based line number of the match within the file.
    pub line_number: usize,
    /// The full text of the matching line.
    pub line_content: String,
}

pub use backend::SearchBackend;
pub use engine::InitialIndexingJob;

use anyhow::Result;
use std::sync::Arc;

/// Entry point for performing searches and managing the index lifecycle.
pub struct SearchService {
    engine: Arc<engine::SearchEngine>,
}

impl SearchService {
    /// Builds the service, initializing the index, file watcher, and root backend.
    pub fn new() -> Result<Self> {
        let engine = Arc::new(engine::SearchEngine::new()?);
        Ok(Self { engine })
    }

    /// Search is synchronous; run it on GPUI's background executor
    /// (`cx.background_spawn`) so the UI thread is not blocked.
    pub fn search(&self, query: String, scope: SearchScope) -> Result<Vec<SearchResult>> {
        self.engine.search(query, scope)
    }

    /// Hands off the one-shot initial-indexing job for the caller to run on a
    /// background executor. Returns `None` once it has been taken.
    pub fn take_initial_indexing_job(&self) -> Option<InitialIndexingJob> {
        self.engine.take_initial_indexing_job()
    }

    /// Returns a receiver for initial-indexing progress in the range `0.0..=1.0`.
    pub fn progress_subscription(&self) -> postage::watch::Receiver<f32> {
        self.engine.progress_subscription()
    }
}
