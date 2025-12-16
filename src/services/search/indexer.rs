use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::schema::{Field, Schema, Term, Value, FAST, STORED, STRING, TEXT};
use tantivy::TantivyDocument;
use tantivy::{Index, IndexWriter}; // Import trait for add_text etc? No, TantivyDocument implements it.

pub struct IndexManager {
    index: Index,
    index_path: PathBuf,
    content_root: PathBuf,
    writer: Arc<Mutex<IndexWriter>>,
}

impl IndexManager {
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let nohrs_dir = home_dir.join(".nohrs");
        let index_path = nohrs_dir.join("index");

        let documents_dir = home_dir.join("Documents");
        Self::new_internal(index_path, documents_dir)
    }

    /// Internal constructor for testing or custom paths
    pub fn new_with_path(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        Self::new_internal(index_path, content_root)
    }

    fn new_internal(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&index_path)?;

        let schema = Self::create_schema();

        let index = if index_path.join("meta.json").exists() {
            // Try to open existing index
            let existing_index = Index::open_in_dir(&index_path)?;
            let existing_schema = existing_index.schema();

            // Check if schema has required fields (e.g., filename was added later)
            if existing_schema.get_field("filename").is_err()
                || existing_schema.get_field("is_directory").is_err()
            {
                tracing::info!(
                    "Schema outdated (missing filename or is_directory field), recreating index..."
                );
                drop(existing_index);
                // Delete old index
                if let Err(e) = fs::remove_dir_all(&index_path) {
                    tracing::warn!("Failed to remove old index: {}", e);
                }
                fs::create_dir_all(&index_path)?;
                Index::create_in_dir(&index_path, schema)?
            } else {
                existing_index
            }
        } else {
            Index::create_in_dir(&index_path, schema)?
        };

        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            index_path,
            content_root,
            writer: Arc::new(Mutex::new(writer)),
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // path: stored and indexed as exact string (keyword) for ID/deletion
        schema_builder.add_text_field("path", STRING | STORED);

        // filename: tokenized for full-text search on file names
        schema_builder.add_text_field("filename", TEXT | STORED);

        // content: indexed but not stored (for full text search)
        schema_builder.add_text_field("content", TEXT);

        // last_modified: fast field for sorting or filtering
        schema_builder.add_u64_field("last_modified", FAST);

        // is_directory: fast field (0=false, 1=true)
        schema_builder.add_u64_field("is_directory", FAST | STORED);

        schema_builder.build()
    }

    // writer() helper removed as we use shared writer

    pub fn index_home(&self, progress_tx: Option<tokio::sync::watch::Sender<f32>>) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Poisoned lock: {}", e))?;
        let schema = self.index.schema();
        let path_field = schema
            .get_field("path")
            .context("Schema error: path field missing")?;
        let filename_field = schema
            .get_field("filename")
            .context("Schema error: filename field missing")?;
        let content_field = schema
            .get_field("content")
            .context("Schema error: content field missing")?;
        let is_directory_field = schema
            .get_field("is_directory")
            .context("Schema error: is_directory field missing")?;

        // 1. Count files if progress tracking is enabled
        let mut total_files = 0;
        if let Some(tx) = &progress_tx {
            let _ = tx.send(0.0);
            let walker = ignore::WalkBuilder::new(&self.content_root)
                .hidden(false)
                .git_ignore(true)
                .build();
            for result in walker {
                if let Ok(entry) = result {
                    // Count both files and directories
                    if entry.path().is_file() || entry.path().is_dir() {
                        total_files += 1;
                    }
                }
            }
        }

        // 2. Index files
        let walker = ignore::WalkBuilder::new(&self.content_root)
            .hidden(false)
            .git_ignore(true)
            .build();

        let mut processed = 0;
        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() {
                        if let Err(e) = self.index_single_file(
                            path,
                            &mut *writer_guard,
                            path_field,
                            filename_field,
                            content_field,
                            is_directory_field,
                        ) {
                            tracing::warn!("Failed to index file {:?}: {}", path, e);
                        }
                    } else if path.is_dir() {
                        if let Err(e) = self.index_single_directory(
                            path,
                            &mut *writer_guard,
                            path_field,
                            filename_field,
                            content_field,
                            is_directory_field,
                        ) {
                            tracing::warn!("Failed to index directory {:?}: {}", path, e);
                        }
                    }

                    // Update progress
                    processed += 1;
                    if let Some(tx) = &progress_tx {
                        if total_files > 0 && processed % 100 == 0 {
                            let _ = tx.send(processed as f32 / total_files as f32);
                        }
                    }
                }
                Err(err) => tracing::warn!("Walk error: {}", err),
            }
        }

        if let Some(tx) = &progress_tx {
            let _ = tx.send(1.0); // Done
        }

        writer_guard.commit()?;
        Ok(())
    }

    fn index_single_directory(
        &self,
        path: &Path,
        writer: &mut IndexWriter,
        path_field: Field,
        filename_field: Field,
        content_field: Field,
        is_directory_field: Field,
    ) -> Result<()> {
        let path_str = path.to_string_lossy();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        let mut doc = TantivyDocument::default();
        doc.add_text(path_field, &path_str);
        doc.add_text(filename_field, &filename);
        doc.add_text(content_field, &filename); // Allow searching dir by name content
        doc.add_u64(is_directory_field, 1);

        writer.delete_term(Term::from_field_text(path_field, &path_str));
        writer.add_document(doc)?;
        Ok(())
    }

    fn index_single_file(
        &self,
        path: &Path,
        writer: &mut IndexWriter,
        path_field: Field,
        filename_field: Field,
        content_field: Field,
        is_directory_field: Field,
    ) -> Result<()> {
        let metadata = fs::metadata(path)?;
        if metadata.len() > 10 * 1024 * 1024 {
            // Skip files larger than 10MB
            tracing::debug!("Skipping large file: {:?}", path);
            return Ok(());
        }

        // Try reading as string. If it fails (binary), we skip.
        match fs::read_to_string(path) {
            Ok(content) => {
                // Check if it looks like binary (contains null byte) - crude check
                if content.contains('\0') {
                    tracing::debug!("Skipping binary file (detected null byte): {:?}", path);
                    return Ok(());
                }

                let path_str = path.to_string_lossy();
                let filename = path.file_name().unwrap_or_default().to_string_lossy();

                // Add path to content so it's searchable via full text query
                let searchable_content = format!("{}\n{}", path_str, content);

                let mut doc = TantivyDocument::default();
                doc.add_text(path_field, &path_str);
                doc.add_text(filename_field, &filename);
                doc.add_text(content_field, &searchable_content);
                doc.add_u64(is_directory_field, 0);

                // Delete existing doc with same path to avoid duplicates (upsert)
                // Note: This matches exact path string.
                writer.delete_term(Term::from_field_text(path_field, &path_str));
                writer.add_document(doc)?;
            }
            Err(_) => {
                tracing::debug!("Skipping binary/unreadable file: {:?}", path);
            }
        }
        Ok(())
    }

    pub fn remove_file(&self, path: &Path) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Poisoned lock: {}", e))?;
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;

        // Remove document with matching path
        let path_str = path.to_string_lossy();
        writer_guard.delete_term(Term::from_field_text(path_field, &path_str));
        writer_guard.commit()?;

        Ok(())
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn process_changes(&self, paths: &[PathBuf]) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Poisoned lock: {}", e))?;
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;
        let filename_field = schema.get_field("filename").context("Schema error")?;
        let content_field = schema.get_field("content").context("Schema error")?;
        let is_directory_field = schema.get_field("is_directory").context("Schema error")?;

        for path in paths {
            if path.exists() {
                if let Err(e) = self.index_single_file(
                    path,
                    &mut *writer_guard,
                    path_field,
                    filename_field,
                    content_field,
                    is_directory_field,
                ) {
                    tracing::warn!("Failed to update index for {:?}: {}", path, e);
                }
            } else {
                let path_str = path.to_string_lossy();
                writer_guard.delete_term(Term::from_field_text(path_field, &path_str));
            }
        }

        if let Err(e) = writer_guard.commit() {
            tracing::error!("Failed to commit index updates: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    pub fn update_file(&self, path: &Path) -> Result<()> {
        self.process_changes(&[path.to_path_buf()])
    }
}

impl super::backend::SearchBackend for IndexManager {
    fn search(&self, query_str: &str) -> Result<Vec<super::SearchResult>> {
        let reader = self.index.reader()?;
        // Wait, self.index.reader()? self.index is Index.
        // Correct way:
        let searcher = reader.searcher();

        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Field not found")?;
        let filename_field = schema.get_field("filename").context("Field not found")?;
        let content_field = schema.get_field("content").context("Field not found")?;
        let is_directory_field = schema
            .get_field("is_directory")
            .context("Field not found")?;

        let query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![filename_field, content_field],
        );
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            // value is OwnedValue.
            // In Tantivy 0.22, retrieved_doc.get_first(field) returns Option<&OwnedValue>.
            // OwnedValue has as_str() if it's a string.
            // We need to import Value trait if we want to use generic accessors,
            // but OwnedValue might have direct methods.
            // Let's rely on explicit match or Debug to find out what works if as_str() fails.
            // Actually, for OwnedValue, it is an enum.
            // If I import Value trait, I can use .as_str().

            if let Some(path_val) = retrieved_doc.get_first(path_field) {
                if let Some(path_str) = path_val.as_str() {
                    let path_buf = PathBuf::from(path_str);

                    match retrieved_doc.get_first(is_directory_field) {
                        Some(val) if val.as_u64() == Some(1) => {
                            // Directory match
                            results.push(super::SearchResult {
                                path: path_buf,
                                line_number: 0,
                                line_content: String::new(),
                            });
                        }
                        _ => {
                            // File match
                            // Find ALL matching lines in file
                            let match_lines = find_all_match_lines(&path_buf, query_str);

                            if match_lines.is_empty() {
                                // No content matches, but file matched by filename - add with empty line
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

/// Find ALL lines in a file that match the query (case-insensitive)
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
    // Debug log if NOHR_DEBUG is set
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
