use crate::core::config::AppConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::schema::{Field, Schema, Term, FAST, STORED, STRING, TEXT};
use tantivy::TantivyDocument;
use tantivy::{Index, IndexWriter};

pub struct IndexManager {
    index: Index,
    _index_path: PathBuf,
    content_roots: Vec<PathBuf>,
    max_file_size: u64,
    exclude_patterns: Vec<String>,
    writer: Arc<Mutex<IndexWriter>>,
}

impl IndexManager {
    pub fn new() -> Result<Self> {
        let config = AppConfig::load().unwrap_or_default();
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let index_path = home_dir.join(".nohrs").join("index");
        let content_roots = config.resolved_index_directories();

        Self::new_internal(index_path, content_roots, &config)
    }

    /// Internal constructor for testing or custom paths
    pub fn new_with_path(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        Self::new_internal(index_path, vec![content_root], &AppConfig::default())
    }

    fn new_internal(
        index_path: PathBuf,
        content_roots: Vec<PathBuf>,
        config: &AppConfig,
    ) -> Result<Self> {
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
            _index_path: index_path,
            content_roots,
            max_file_size: config.index.max_file_size,
            exclude_patterns: config.index.exclude_patterns.clone(),
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

        // 複数ディレクトリ対応: config の directories を順に走査
        if self.content_roots.is_empty() {
            tracing::warn!("No index directories configured");
            if let Some(tx) = &progress_tx {
                let _ = tx.send(1.0);
            }
            writer_guard.commit()?;
            return Ok(());
        }

        let first_root = &self.content_roots[0];
        let mut walker_builder = ignore::WalkBuilder::new(first_root);
        for root in self.content_roots.iter().skip(1) {
            walker_builder.add(root);
        }
        walker_builder.hidden(false).git_ignore(true);
        let walker = walker_builder.build();

        if let Some(tx) = &progress_tx {
            let _ = tx.send(0.0);
        }

        let mut processed: u64 = 0;
        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    // 除外パターンに一致するパスをスキップ
                    if self.should_exclude(path) {
                        continue;
                    }
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

                    processed += 1;
                    if let Some(tx) = &progress_tx {
                        if processed % 100 == 0 {
                            // 概算: 件数ベースで進捗通知（最大 0.99 でクランプ）
                            let _ = tx.send((processed as f32 / (processed as f32 + 100.0)).min(0.99));
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
        if metadata.len() > self.max_file_size {
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
                // ファイルとディレクトリの両方を処理 (4.3.7)
                if path.is_dir() {
                    if let Err(e) = self.index_single_directory(
                        path,
                        &mut *writer_guard,
                        path_field,
                        filename_field,
                        content_field,
                        is_directory_field,
                    ) {
                        tracing::warn!("Failed to update index for directory {:?}: {}", path, e);
                    }
                } else {
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

    /// 除外パターンに一致するかチェック
    fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.exclude_patterns.iter().any(|pattern| {
            // コンポーネント名にマッチ (e.g. "node_modules", ".git")
            path.components().any(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .eq(pattern.as_str())
            })
            // glob 風マッチ (e.g. "*.log")
            || (pattern.contains('*') && glob_match(pattern, &path_str))
        })
    }

    /// インデックス対象ディレクトリ一覧を返す
    pub fn content_roots(&self) -> &[PathBuf] {
        &self.content_roots
    }
}

/// シンプルな glob マッチ (*.ext パターンのみ対応)
fn glob_match(pattern: &str, text: &str) -> bool {
    if let Some(ext) = pattern.strip_prefix("*.") {
        text.ends_with(&format!(".{}", ext))
    } else if let Some(prefix) = pattern.strip_suffix("*") {
        text.starts_with(prefix)
    } else {
        text.contains(pattern)
    }
}

// SearchBackend impl is in indexer_search.rs
