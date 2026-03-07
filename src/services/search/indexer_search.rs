use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tantivy::schema::Value;
use tantivy::TantivyDocument;

use crate::core::types::{SearchQuery, SearchType};

use super::indexer::IndexManager;

impl super::backend::SearchBackend for IndexManager {
    fn search(&self, query: &SearchQuery) -> Result<Vec<super::SearchResult>> {
        let reader = self.index().reader()?;
        let searcher = reader.searcher();

        let schema = self.index().schema();
        let path_field = schema.get_field("path").context("Field not found")?;
        let filename_field = schema.get_field("filename").context("Field not found")?;
        let content_field = schema.get_field("content").context("Field not found")?;
        let is_directory_field = schema
            .get_field("is_directory")
            .context("Field not found")?;

        // SearchType に応じて検索対象フィールドを切り替え
        let search_fields = match query.search_type {
            SearchType::Filename => vec![filename_field],
            SearchType::Content => vec![content_field],
            SearchType::All => vec![filename_field, content_field],
        };

        let query_parser =
            tantivy::query::QueryParser::for_index(self.index(), search_fields);

        // ユーザー入力をサニタイズ: Tantivy の特殊構文をエスケープ (4.4.3)
        let sanitized = sanitize_tantivy_query(&query.query);
        let parsed_query = query_parser.parse_query(&sanitized)?;

        // 上限を 50 → 200 に増加 (4.2.3)
        let top_docs =
            searcher.search(&parsed_query, &tantivy::collector::TopDocs::with_limit(200))?;

        let match_case = query.match_case;
        let match_whole_word = query.match_whole_word;
        let query_str = &query.query;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            if let Some(path_val) = retrieved_doc.get_first(path_field) {
                if let Some(path_str) = path_val.as_str() {
                    let path_buf = PathBuf::from(path_str);

                    match retrieved_doc.get_first(is_directory_field) {
                        Some(val) if val.as_u64() == Some(1) => {
                            results.push(super::SearchResult {
                                path: path_buf,
                                line_number: 0,
                                line_content: String::new(),
                                match_start: 0,
                                match_end: 0,
                            });
                        }
                        _ => {
                            let match_lines = find_all_match_lines(
                                &path_buf,
                                query_str,
                                match_case,
                                match_whole_word,
                            );

                            if match_lines.is_empty() {
                                results.push(super::SearchResult {
                                    path: path_buf,
                                    line_number: 0,
                                    line_content: String::new(),
                                    match_start: 0,
                                    match_end: 0,
                                });
                            } else {
                                for (line_number, line_content, match_start, match_end) in
                                    match_lines
                                {
                                    results.push(super::SearchResult {
                                        path: path_buf.clone(),
                                        line_number,
                                        line_content,
                                        match_start,
                                        match_end,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
    }
}

/// Tantivy クエリ構文の特殊文字をエスケープ (4.4.3)
fn sanitize_tantivy_query(input: &str) -> String {
    let special_chars = [
        ':', '(', ')', '[', ']', '{', '}', '!', '^', '"', '~', '*', '?', '\\', '/',
    ];
    let mut result = String::with_capacity(input.len() * 2);
    for ch in input.chars() {
        if special_chars.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    // AND / OR / NOT はフレーズ化が最も安全だが、単純に小文字にする
    // Tantivy はデフォルトで大文字の AND/OR/NOT を演算子として扱う
    result
}

/// ファイルを読み込んでマッチ行を返す（行番号, 内容, match_start, match_end）
fn find_all_match_lines(
    path: &Path,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Vec<(usize, String, usize, usize)> {
    let mut matches = Vec::new();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return matches,
    };

    for (idx, line) in content.lines().enumerate() {
        if let Some((start, end)) =
            find_match_in_line(line, query, match_case, match_whole_word)
        {
            matches.push((idx + 1, line.to_string(), start, end));
        }
    }

    matches
}

/// 行内でクエリがマッチする最初の位置を返す
fn find_match_in_line(
    line: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }

    let (haystack, needle) = if match_case {
        (line.to_string(), query.to_string())
    } else {
        (line.to_lowercase(), query.to_lowercase())
    };

    let mut start = 0;
    while let Some(pos) = haystack[start..].find(&needle) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + needle.len();

        if match_whole_word {
            let at_word_start =
                abs_pos == 0 || !haystack.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
            let at_word_end = end_pos >= haystack.len()
                || !haystack.as_bytes()[end_pos].is_ascii_alphanumeric();
            if at_word_start && at_word_end {
                return Some((abs_pos, end_pos));
            }
            start = abs_pos + 1;
        } else {
            return Some((abs_pos, end_pos));
        }
    }

    None
}
