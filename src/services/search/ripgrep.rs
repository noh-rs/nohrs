use super::backend::SearchBackend;
use super::SearchResult;
use anyhow::Result;

pub struct RipgrepBackend;

impl SearchBackend for RipgrepBackend {
    fn search(&self, _query: &str) -> Result<Vec<SearchResult>> {
        Ok(vec![])
    }
}
