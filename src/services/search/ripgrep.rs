use super::backend::SearchBackend;
use super::SearchResult;
use crate::core::types::{SearchQuery, SearchType};
use anyhow::{Context, Result};
use grep::regex::RegexMatcherBuilder;
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
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
    query_str: String,
    case_insensitive: bool,
}

impl Sink for MatchStorage {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch) -> Result<bool, Self::Error> {
        let line_number = mat.line_number().unwrap_or(0) as usize;
        let line_content = std::str::from_utf8(mat.bytes())
            .unwrap_or("<binary>")
            .trim_end()
            .to_string();

        let mut results = self
            .results
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Lock poisoned"))?;

        if results.len() >= self.max_results {
            return Ok(false);
        }

        // マッチ位置を計算
        let (match_start, match_end) =
            find_match_position(&line_content, &self.query_str, self.case_insensitive);

        results.push(SearchResult {
            path: self.path.clone(),
            line_number,
            line_content,
            match_start,
            match_end,
        });

        Ok(true)
    }
}

/// 行内のマッチ位置（バイトオフセット）を返す
fn find_match_position(line: &str, query: &str, case_insensitive: bool) -> (usize, usize) {
    if query.is_empty() {
        return (0, 0);
    }
    if case_insensitive {
        let line_lower = line.to_lowercase();
        let query_lower = query.to_lowercase();
        if let Some(pos) = line_lower.find(&query_lower) {
            return (pos, pos + query_lower.len());
        }
    } else if let Some(pos) = line.find(query) {
        return (pos, pos + query.len());
    }
    (0, 0)
}

/// SearchQuery からパターン文字列を構築
fn build_pattern(query: &SearchQuery) -> String {
    let base = if query.use_regex {
        query.query.clone()
    } else {
        regex::escape(&query.query)
    };

    if query.match_whole_word {
        format!(r"\b{}\b", base)
    } else {
        base
    }
}

impl SearchBackend for RipgrepBackend {
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(Vec::new());
        }

        // SearchType::Filename の場合はファイル名のみでマッチ
        if query.search_type == SearchType::Filename {
            return self.search_filenames(query);
        }

        let pattern = build_pattern(query);
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(!query.match_case)
            .build(&pattern)
            .context("Invalid regex pattern")?;

        let results = Arc::new(Mutex::new(Vec::new()));

        let walker = WalkBuilder::new(&self.root)
            .max_depth(Some(10))
            .hidden(true)
            .git_ignore(true)
            .threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8))
            .build_parallel();

        const MAX_RESULTS: usize = 100;

        let results_ref = &results;
        let query_str = query.query.clone();
        let case_insensitive = !query.match_case;

        walker.run(|| {
            let results = results_ref.clone();
            let matcher = matcher.clone();
            let query_str = query_str.clone();
            Box::new(move |entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => return ignore::WalkState::Continue,
                };

                if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                    return ignore::WalkState::Continue;
                }

                // 上限チェック
                {
                    let r = results.lock().unwrap_or_else(|e| e.into_inner());
                    if r.len() >= MAX_RESULTS {
                        return ignore::WalkState::Quit;
                    }
                }

                let path = entry.path().to_path_buf();
                let sink = MatchStorage {
                    results: results.clone(),
                    path: path.clone(),
                    max_results: MAX_RESULTS,
                    query_str: query_str.clone(),
                    case_insensitive,
                };

                let mut searcher = SearcherBuilder::new()
                    .binary_detection(BinaryDetection::quit(b'\x00'))
                    .line_number(true)
                    .build();

                if let Err(e) = searcher.search_path(&matcher, &path, sink) {
                    tracing::debug!("Search error for {:?}: {}", path, e);
                }

                ignore::WalkState::Continue
            })
        });

        let final_results = results
            .lock()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        Ok(final_results.clone())
    }
}

impl RipgrepBackend {
    /// ファイル名のみで検索
    fn search_filenames(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pattern = build_pattern(query);
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!query.match_case)
            .build()
            .context("Invalid regex pattern")?;

        let mut results = Vec::new();
        const MAX_RESULTS: usize = 100;

        let walker = WalkBuilder::new(&self.root)
            .max_depth(Some(10))
            .hidden(true)
            .git_ignore(true)
            .build();

        for entry in walker {
            if results.len() >= MAX_RESULTS {
                break;
            }
            match entry {
                Ok(entry) => {
                    let filename = entry
                        .path()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    if re.is_match(&filename) {
                        results.push(SearchResult {
                            path: entry.path().to_path_buf(),
                            line_number: 0,
                            line_content: String::new(),
                            match_start: 0,
                            match_end: 0,
                        });
                    }
                }
                Err(e) => tracing::debug!("Walk error: {}", e),
            }
        }

        Ok(results)
    }
}
