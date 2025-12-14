pub mod backend;
pub mod indexer;
pub mod ripgrep;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
}

pub trait SearchBackend {
    fn search(&self, query: &str) -> Result<Vec<SearchResult>>;
}

pub struct SearchService {
    // We will add backend instances here later
}

impl SearchService {
    pub fn new() -> Self {
        Self {}
    }
}
