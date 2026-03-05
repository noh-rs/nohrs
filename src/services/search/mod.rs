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

/// 検索プロバイダートレイト: テスト時にモック差し替え可能
pub trait SearchProvider: Send + Sync {
    fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>>;
}

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

impl SearchProvider for SearchService {
    fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>> {
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(self.search(query.to_string(), scope)))
    }
}
