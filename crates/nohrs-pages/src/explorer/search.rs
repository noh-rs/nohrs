use super::ExplorerPane;
use super::types::{SearchFileResult, SearchMatch, StatusLevel};
use gpui::{AppContext, AsyncApp, Context, Window};
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_services::search::SearchResult;
use std::collections::HashMap;

impl ExplorerPane {
    pub(crate) fn trigger_search(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Invalidate any in-flight search; only the latest request may apply its
        // results (including the empty-query and degraded-mode early returns,
        // which clear results and must not be overwritten by a slower search).
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;

        if self.search_query.is_empty() {
            self.search_results = None;
            self.apply_filter();
            self.clear_status();
            cx.notify();
            return;
        }

        let Some(service) = self.search_service.clone() else {
            // Degraded mode: full-text search is unavailable, so fall back to
            // filtering the already-loaded directory by filename.
            self.search_results = None;
            self.apply_filter();
            self.set_status(
                StatusLevel::Error,
                "Search index unavailable; filtering current folder by name only",
            );
            cx.notify();
            return;
        };

        self.is_performing_search = true;
        self.clear_status();
        cx.notify();

        let query = self.search_query.clone();
        let scope = self.search_scope;

        cx.spawn(
            move |this: gpui::WeakEntity<ExplorerPane>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    // `SearchService::search` is synchronous, and grouping plus
                    // `results_to_entries` resolve per-file metadata (stat calls).
                    // Run the whole chain on GPUI's background executor so the UI
                    // thread stays responsive (replaces tokio::task::spawn_blocking).
                    let processed = cx
                        .background_spawn(async move {
                            match service.search(query, scope) {
                                Ok(res) => {
                                    let grouped = group_results(res);
                                    let entries = results_to_entries(&grouped);
                                    Ok((grouped, entries))
                                }
                                Err(error) => Err(error),
                            }
                        })
                        .await;

                    this.update(&mut cx, |this, cx| {
                        // Discard results if a newer search has since been issued.
                        if this.search_generation != generation {
                            return;
                        }
                        match processed {
                            Ok((grouped, entries)) => {
                                this.filtered_entries = entries;
                                this.search_results = Some(grouped);
                                this.clear_status();
                            }
                            Err(error) => {
                                tracing::error!("Search failed: {}", error);
                                this.search_results = Some(Vec::new());
                                this.filtered_entries = Vec::new();
                                this.set_status(
                                    StatusLevel::Error,
                                    format!("Search failed: {}", error),
                                );
                            }
                        }
                        this.is_performing_search = false;
                        this.update_item_sizes();
                        cx.notify();
                    })
                }
            },
        )
        .detach();
    }

    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_visible {
            return;
        }
        self.search_visible = true;
        self.search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_visible {
            return;
        }
        self.search_visible = false;
        self.search_results = None;
        self.search_query.clear();
        self.apply_filter();
        self.update_editor_search(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_visible {
            self.close_search(window, cx);
        } else {
            self.open_search(window, cx);
        }
    }
}

pub fn group_results(results: Vec<SearchResult>) -> Vec<SearchFileResult> {
    let mut file_map: HashMap<String, SearchFileResult> = HashMap::new();

    for res in results {
        let entry = file_map
            .entry(res.path.to_string_lossy().to_string())
            .or_insert_with(|| {
                let path = std::path::Path::new(&res.path);
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let folder = if let Some(parent) = path.parent() {
                    parent.to_string_lossy().to_string()
                } else {
                    String::new()
                };

                SearchFileResult {
                    path: res.path.to_string_lossy().to_string(),
                    folder,
                    filename,
                    matches: Vec::new(),
                }
            });

        if res.line_number > 0 {
            entry.matches.push(SearchMatch {
                line_number: res.line_number,
                line_content: res.line_content,
                match_start: 0,
                match_end: 0,
            });
        }
    }

    let mut sorted_results: Vec<SearchFileResult> = file_map.into_values().collect();
    sorted_results.sort_by(|a, b| a.path.cmp(&b.path));

    if std::env::var("NOHR_DEBUG").is_ok() {
        for r in &sorted_results {
            tracing::info!(
                "[DEBUG] group_results: file='{}', matches={}",
                r.filename,
                r.matches.len()
            );
        }
    }

    sorted_results
}

pub fn results_to_entries(results: &[SearchFileResult]) -> Vec<FileEntryDto> {
    results
        .iter()
        .map(|res| {
            let meta = std::fs::metadata(&res.path).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            FileEntryDto {
                name: if res.folder.is_empty() {
                    res.filename.clone()
                } else {
                    format!("{}/{}", res.folder, res.filename)
                },
                path: res.path.clone(),
                kind: if is_dir {
                    "dir".to_string()
                } else {
                    "file".to_string()
                },
                size,
                modified,
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn result(path: &str, line: usize, content: &str) -> SearchResult {
        SearchResult {
            path: PathBuf::from(path),
            line_number: line,
            line_content: content.to_string(),
        }
    }

    #[test]
    fn group_results_aggregates_matches_per_file_and_sorts() {
        let grouped = group_results(vec![
            result("/b/two.txt", 5, "x"),
            result("/a/one.txt", 1, "a"),
            result("/a/one.txt", 9, "b"),
        ]);
        // Sorted by path: /a/one.txt before /b/two.txt.
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].path, "/a/one.txt");
        assert_eq!(grouped[0].filename, "one.txt");
        assert_eq!(grouped[0].folder, "/a");
        assert_eq!(grouped[0].matches.len(), 2);
        assert_eq!(grouped[1].filename, "two.txt");
    }

    #[test]
    fn group_results_skips_zero_line_markers_and_empty_input() {
        assert!(group_results(Vec::new()).is_empty());
        // A line_number of 0 is a file-only hit; no SearchMatch is recorded.
        let grouped = group_results(vec![result("/a/file.txt", 0, "")]);
        assert_eq!(grouped.len(), 1);
        assert!(grouped[0].matches.is_empty());
    }

    #[test]
    fn results_to_entries_reads_metadata_and_builds_names() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("real.txt");
        std::fs::write(&file_path, "12345").unwrap();

        let entries = results_to_entries(&[
            SearchFileResult {
                path: file_path.to_string_lossy().to_string(),
                folder: dir.path().to_string_lossy().to_string(),
                filename: "real.txt".to_string(),
                matches: Vec::new(),
            },
            SearchFileResult {
                path: "/missing/ghost.txt".to_string(),
                folder: String::new(),
                filename: "ghost.txt".to_string(),
                matches: Vec::new(),
            },
        ]);

        assert_eq!(entries[0].kind, "file");
        assert_eq!(entries[0].size, 5);
        assert!(entries[0].name.ends_with("real.txt"));

        // Missing file: defaults, and empty folder means the name is just the filename.
        assert_eq!(entries[1].size, 0);
        assert_eq!(entries[1].name, "ghost.txt");
    }
}
