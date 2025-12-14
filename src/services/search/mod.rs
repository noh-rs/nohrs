pub mod backend;
pub mod engine;
pub mod indexer;
pub mod ripgrep;
pub mod watcher;

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    Home,
    Root,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
}

pub use backend::SearchBackend;

use anyhow::Result;
use std::sync::Arc;

pub struct SearchService {
    engine: Arc<engine::SearchEngine>,
}

impl SearchService {
    pub async fn new() -> Result<Self> {
        let engine = Arc::new(engine::SearchEngine::new().await?);
        Ok(Self { engine })
    }

    pub async fn search(&self, query: String, scope: SearchScope) -> Result<Vec<SearchResult>> {
        self.engine.search(query, scope).await
    }

    pub fn progress_subscription(&self) -> tokio::sync::watch::Receiver<f32> {
        self.engine.progress_subscription()
    }
}
