use super::backend::SearchBackend;
use super::SearchResult;
use anyhow::{Context, Result};
use grep::regex::RegexMatcher;
use grep::searcher::{Searcher, Sink, SinkMatch};
use ignore::WalkBuilder;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct RipgrepBackend {
    root: PathBuf,
}

impl RipgrepBackend {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

struct MatchStorage {
    results: Arc<Mutex<Vec<SearchResult>>>,
    path: PathBuf,
    max_results: usize,
}

impl Sink for MatchStorage {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch) -> Result<bool, Self::Error> {
        let line_number = mat.line_number().unwrap_or(0) as usize;
        let line_content = std::str::from_utf8(mat.bytes())
            .unwrap_or("<binary>")
            .to_string();

        let mut results = self
            .results
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"))?;

        // Check if limit reached
        if results.len() >= self.max_results {
            return Ok(false); // Stop searching this file
        }

        results.push(SearchResult {
            path: self.path.clone(),
            line_number,
            line_content,
        });

        Ok(true) // Continue searching
    }
}

impl SearchBackend for RipgrepBackend {
    fn search(&self, query_str: &str) -> Result<Vec<SearchResult>> {
        let matcher = RegexMatcher::new(query_str).context("Invalid regex")?;
        let results = Arc::new(Mutex::new(Vec::new()));

        let walker = WalkBuilder::new(&self.root)
            .max_depth(Some(10)) // Limit depth to avoid deep recursion
            .hidden(true) // Skip hidden files
            .git_ignore(true)
            .build();

        const MAX_RESULTS: usize = 100;

        for result in walker {
            // Check if we've hit the result limit
            {
                let current_results = results
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                if current_results.len() >= MAX_RESULTS {
                    tracing::info!("Search hit result limit of {}", MAX_RESULTS);
                    break;
                }
            }

            match result {
                Ok(entry) => {
                    if entry.file_type().map_or(false, |ft| ft.is_file()) {
                        let path = entry.path().to_path_buf();
                        let results_clone = results.clone();
                        let sink = MatchStorage {
                            results: results_clone,
                            path: path.clone(),
                            max_results: MAX_RESULTS,
                        };

                        let mut searcher = Searcher::new();
                        if let Err(e) = searcher.search_path(&matcher, &path, sink) {
                            // Ignore search errors (e.g. binary file) similar to ripgrep
                            tracing::debug!("Search error for {:?}: {}", path, e);
                        }
                    }
                }
                Err(e) => tracing::debug!("Walk error: {}", e),
            }
        }

        let final_results = results
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        Ok(final_results.clone())
    }
}
