use super::SearchResult;
use anyhow::Result;

/// A pluggable source of search results for a given query.
pub trait SearchBackend: Send + Sync {
    /// Runs `query` and returns the matching results.
    fn search(&self, query: &str) -> Result<Vec<SearchResult>>;
}
