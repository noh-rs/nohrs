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

pub struct SearchService {
    // We will add backend instances here later
}

impl SearchService {
    pub fn new() -> Self {
        Self {}
    }
}
