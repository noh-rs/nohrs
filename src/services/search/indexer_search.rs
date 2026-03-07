use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tantivy::schema::Value;
use tantivy::TantivyDocument;

use super::indexer::IndexManager;

impl super::backend::SearchBackend for IndexManager {
    fn search(&self, query_str: &str) -> Result<Vec<super::SearchResult>> {
        let reader = self.index().reader()?;
        let searcher = reader.searcher();

        let schema = self.index().schema();
        let path_field = schema.get_field("path").context("Field not found")?;
        let filename_field = schema.get_field("filename").context("Field not found")?;
        let content_field = schema.get_field("content").context("Field not found")?;
        let is_directory_field = schema
            .get_field("is_directory")
            .context("Field not found")?;

        let query_parser = tantivy::query::QueryParser::for_index(
            self.index(),
            vec![filename_field, content_field],
        );
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(50))?;

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
                            });
                        }
                        _ => {
                            let match_lines = find_all_match_lines(&path_buf, query_str);

                            if match_lines.is_empty() {
                                results.push(super::SearchResult {
                                    path: path_buf,
                                    line_number: 0,
                                    line_content: String::new(),
                                });
                            } else {
                                for (line_number, line_content) in match_lines {
                                    results.push(super::SearchResult {
                                        path: path_buf.clone(),
                                        line_number,
                                        line_content,
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

fn find_all_match_lines(path: &Path, query: &str) -> Vec<(usize, String)> {
    let mut matches = Vec::new();
    if let Ok(content) = fs::read_to_string(path) {
        let query_lower = query.to_lowercase();
        for (idx, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                matches.push((idx + 1, line.to_string()));
            }
        }
    }
    if std::env::var("NOHR_DEBUG").is_ok() {
        tracing::info!(
            "[DEBUG] find_all_match_lines: path={:?}, query='{}', matches={}",
            path,
            query,
            matches.len()
        );
    }
    matches
}
