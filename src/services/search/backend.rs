use super::SearchResult;
use crate::core::types::SearchQuery;
use anyhow::Result;

pub trait SearchBackend: Send + Sync {
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
}
