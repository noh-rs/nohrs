//! Asking for the index to be brought up to date, wherever the writer lives.
//!
//! Reading the index never comes through here — [`super::indexer::IndexReader`]
//! reads tantivy directly, from any number of processes at once. This is only
//! the *writing* side, and it is a trait because the writer moves: today it is
//! whichever process took tantivy's one writer lock, and per
//! [ADR 0009](../../../../docs/adr/0009-indexd-owns-the-index-writer.md) it
//! becomes `nohrs-indexd`, which owns the watcher beside it.
//!
//! Keeping the two apart is what makes that move safe to do in stages: a caller
//! that only reads is unaffected by it, and a caller that writes cares about
//! the request, not about who serves it.

use std::path::PathBuf;

use anyhow::{Context, Result};

use super::indexer::{IndexManager, IndexReader, IndexReport, Refresh};

/// What the index holds and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    /// Where the index is (or would be).
    pub index_path: PathBuf,
    /// The tree the index covers.
    pub content_root: PathBuf,
    /// How many documents it holds, or `None` when nothing has built one.
    pub documents: Option<u64>,
    /// Whether something is watching the tree for changes as this is answered.
    pub watching: bool,
    /// How many processes are holding the index process up, or `None` when the
    /// index has no process of its own to hold.
    pub clients: Option<usize>,
}

/// A way to ask for the index to be updated.
pub trait IndexControl: Send + Sync {
    /// Where the index is and what it holds.
    fn status(&self) -> Result<IndexStatus>;

    /// Bring the index up to date with the tree it covers, publishing how far
    /// along it is on `progress` in the range `0.0..=1.0`.
    ///
    /// The progress is what a first build needs: it is minutes of silence
    /// otherwise, and the caller has a status bar to fill.
    fn refresh_reporting(
        &self,
        refresh: Refresh,
        progress: Option<postage::watch::Sender<f32>>,
    ) -> Result<IndexReport>;

    /// Bring the index up to date, with nowhere to report progress.
    fn refresh(&self, refresh: Refresh) -> Result<IndexReport> {
        self.refresh_reporting(refresh, None)
    }
}

/// The writer in this process: takes tantivy's lock and does the work here.
///
/// What a one-shot command uses when no daemon is around to ask, and the
/// fallback everywhere else.
pub struct InProcess {
    manager: IndexManager,
}

impl InProcess {
    /// Opens the default index for writing. Does not take the writer lock yet —
    /// [`IndexManager`] leaves that until something is actually written.
    pub fn open_default() -> Result<Self> {
        Ok(Self {
            manager: IndexManager::new()?,
        })
    }

    /// Opens a specific index, for tests and for a caller with its own layout.
    pub fn open(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        Ok(Self {
            manager: IndexManager::new_with_path(index_path, content_root)?,
        })
    }

    /// The manager underneath, for a caller that also owns a watcher.
    pub fn manager(&self) -> &IndexManager {
        &self.manager
    }
}

impl IndexControl for InProcess {
    fn status(&self) -> Result<IndexStatus> {
        let (index_path, content_root) = IndexManager::default_location()?;
        let documents = IndexReader::open(index_path.clone(), content_root.clone())
            .context("cannot read the index")?
            .map(|reader| reader.document_count());
        Ok(IndexStatus {
            index_path,
            content_root,
            documents,
            // Nothing here watches: this is a pass someone asked for, not a
            // process that keeps the index level with the filesystem.
            watching: false,
            clients: None,
        })
    }

    fn refresh_reporting(
        &self,
        refresh: Refresh,
        progress: Option<postage::watch::Sender<f32>>,
    ) -> Result<IndexReport> {
        self.manager.index_home(refresh, progress)
    }
}
