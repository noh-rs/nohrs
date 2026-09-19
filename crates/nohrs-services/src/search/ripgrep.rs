use super::SearchResult;
use super::backend::SearchBackend;
use super::scoped::{self, Options, Subject};
use anyhow::Result;
use std::path::PathBuf;

/// Search backend that walks the filesystem from `root` and matches lines with a regex.
pub struct RipgrepBackend {
    root: PathBuf,
    options: Options,
}

impl RipgrepBackend {
    /// Creates a backend that searches recursively under `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            options: Options {
                // This backend answers the whole-filesystem scope, so the depth
                // cap is what keeps a query off the deepest corners of a disk.
                max_depth: Some(10),
                // Root-scope results have always been matching lines, and the
                // explorer renders them as such; matching names here too would
                // change what the GUI shows, which is not this backend's call.
                subject: Subject::Contents,
                ..Options::default()
            },
        }
    }
}

impl SearchBackend for RipgrepBackend {
    fn search(&self, query_str: &str) -> Result<Vec<SearchResult>> {
        Ok(scoped::search(&self.root, query_str, &self.options)?.results)
    }
}
