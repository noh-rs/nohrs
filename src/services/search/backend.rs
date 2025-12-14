use super::SearchResult;
use anyhow::Result;

pub trait SearchBackend: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<SearchResult>>;
}
