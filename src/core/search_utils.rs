use super::types::{SearchFileResult, SearchMatch};
use crate::services::fs::listing::FileEntryDto;
use crate::services::search::SearchResult;
use std::collections::HashMap;

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
                match_start: res.match_start,
                match_end: res.match_end,
            });
        }
    }

    let mut sorted_results: Vec<SearchFileResult> = file_map.into_values().collect();
    sorted_results.sort_by(|a, b| a.path.cmp(&b.path));

    sorted_results
}

/// 検索結果をファイル一覧エントリに変換する
/// name フィールドはファイル名のみを使う (4.4.2: フルパスの重複を避ける)
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
                name: res.filename.clone(),
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
