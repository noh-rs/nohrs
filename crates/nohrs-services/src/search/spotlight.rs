use super::SearchResult;
use super::backend::SearchBackend;
use anyhow::{Context, Result};
use nohrs_core::config::{SEARCH_MAX_LINE_LEN, SEARCH_MAX_MATCHES_PER_FILE};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;

/// Search backend that uses macOS Spotlight (`mdfind`) to find candidate files,
/// then greps their contents.
pub struct SpotlightBackend;

impl Default for SpotlightBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SpotlightBackend {
    /// Creates a new Spotlight-backed search backend.
    pub fn new() -> Self {
        Self
    }

    fn grep_in_file(&self, path: &PathBuf, query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return results,
        };
        let reader = BufReader::new(file);
        let query_lower = query.to_lowercase();

        // Simple case-insensitive search
        for (index, line) in reader.lines().enumerate() {
            if let Ok(content) = line {
                if content.len() > SEARCH_MAX_LINE_LEN {
                    continue; // Skip very long lines
                }

                if content.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        path: path.clone(),
                        line_number: index + 1,
                        line_content: content.trim().to_string(),
                    });

                    if results.len() >= SEARCH_MAX_MATCHES_PER_FILE {
                        break;
                    }
                }
            }
        }

        results
    }
}

impl SearchBackend for SpotlightBackend {
    fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        // `Command` invokes `mdfind` directly (no shell), so there is no
        // shell-injection vector. To stop Spotlight from interpreting query
        // operators in user input we pass `-literal`, and we scope the search to
        // the user's home directory with `-onlyin`. mdfind has no `--`
        // end-of-options separator, so `-literal` is the closest equivalent for
        // forcing the argument to be treated as a literal query.
        let home = dirs::home_dir().unwrap_or_else(|| {
            tracing::warn!("Home directory not found; scoping mdfind search to filesystem root");
            PathBuf::from("/")
        });
        let output = Command::new("mdfind")
            .arg("-onlyin")
            .arg(&home)
            .arg("-literal")
            .arg(query)
            .output()
            .context("Failed to execute mdfind")?;

        match output.status.code() {
            Some(0) => {}
            Some(code) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!(
                    "mdfind exited with status {code}: {}",
                    stderr.trim()
                ));
            }
            None => return Err(anyhow::anyhow!("mdfind was terminated by a signal")),
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        // Removed MAX_TOTAL_RESULTS to return all results as requested
        for line in stdout.lines() {
            let path = PathBuf::from(line);
            if path.exists() && path.is_file() {
                let file_matches = self.grep_in_file(&path, query);
                results.extend(file_matches);
            }
        }

        Ok(results)
    }
}
