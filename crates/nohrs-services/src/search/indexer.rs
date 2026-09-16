use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::TantivyDocument;
use tantivy::schema::{FAST, Field, STORED, STRING, Schema, TEXT, Term, Value};
use tantivy::{Index, IndexWriter}; // Import trait for add_text etc? No, TantivyDocument implements it.

/// How much heap tantivy's writer may use while indexing.
const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Owns the tantivy index and its writer, and performs full and incremental indexing.
pub struct IndexManager {
    index: Index,
    index_path: PathBuf,
    content_root: PathBuf,
    // Opened by the first write and kept afterwards, never by construction:
    // tantivy allows a single writer across all processes, so taking one up
    // front would deny it to `noh index build` and to a second window for as
    // long as this process lives — including when it never writes at all.
    // Readers ([`IndexReader`]) need no lock and are unaffected either way.
    writer: Mutex<Option<IndexWriter>>,
}

impl IndexManager {
    /// Opens (or creates) the default index under `~/.nohrs/index` rooted at `~/Documents`.
    pub fn new() -> Result<Self> {
        let (index_path, content_root) = Self::default_location()?;
        Self::new_internal(index_path, content_root)
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

        Ok(Self {
            index,
            index_path,
            content_root,
            writer: Mutex::new(None),
        })
    }

    /// Runs `work` against the index writer, opening one if this process does
    /// not hold it yet.
    ///
    /// Failing to open it means another process is writing — the app while
    /// `noh index build` runs, or the other way round — which is a thing to
    /// say plainly rather than a bug to report.
    fn with_writer<R>(&self, work: impl FnOnce(&mut IndexWriter) -> Result<R>) -> Result<R> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|error| anyhow::anyhow!("Poisoned lock: {}", error))?;
        if guard.is_none() {
            let writer = self.index.writer(WRITER_HEAP_BYTES).with_context(|| {
                format!(
                    "the index at {} is being written by another nohrs process; \
                     tantivy allows one writer at a time",
                    self.index_path.display()
                )
            })?;
            *guard = Some(writer);
        }
        let writer = guard
            .as_mut()
            .context("the index writer went missing after it was opened")?;
        work(writer)
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

    /// Indexes the content root from scratch, reporting progress through `progress_tx` if given.
    #[tracing::instrument(
        target = "nohrs::op",
        name = "index.build_home",
        level = "debug",
        skip_all
    )]
    pub fn index_home(&self, progress_tx: Option<postage::watch::Sender<f32>>) -> Result<()> {
        self.with_writer(|writer| self.index_home_with(writer, progress_tx))
    }

    fn index_home_with(
        &self,
        writer: &mut IndexWriter,
        mut progress_tx: Option<postage::watch::Sender<f32>>,
    ) -> Result<()> {
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
        if let Some(tx) = &mut progress_tx {
            *tx.borrow_mut() = 0.0;
            let walker = ignore::WalkBuilder::new(&self.content_root)
                .hidden(false)
                .git_ignore(true)
                .build();
            for entry in walker.flatten() {
                // Count both files and directories
                if entry.path().is_file() || entry.path().is_dir() {
                    total_files += 1;
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
                            writer,
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
                            writer,
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
                    if let Some(tx) = &mut progress_tx {
                        if total_files > 0 && processed % 100 == 0 {
                            *tx.borrow_mut() = processed as f32 / total_files as f32;
                        }
                    }
                }
                Err(err) => tracing::warn!("Walk error: {}", err),
            }
        }

        if let Some(tx) = &mut progress_tx {
            *tx.borrow_mut() = 1.0; // Done
        }

        writer.commit()?;
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
        // Indexing runs on the search backend's own threads, never the GPUI
        // foreground loop, so the blocking read is fine here.
        #[allow(clippy::disallowed_methods)]
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

    /// Removes the document for `path` from the index and commits.
    #[tracing::instrument(target = "nohrs::op", name = "index.remove", level = "debug", skip_all, fields(path = %path.display()))]
    pub fn remove_file(&self, path: &Path) -> Result<()> {
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;

        self.with_writer(|writer| {
            // Remove document with matching path
            let path_str = path.to_string_lossy();
            writer.delete_term(Term::from_field_text(path_field, &path_str));
            writer.commit()?;
            Ok(())
        })
    }

    /// Returns a reference to the underlying tantivy index.
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Re-indexes or removes each of `paths` (depending on existence) and commits once.
    #[tracing::instrument(target = "nohrs::op", name = "index.process_changes", level = "debug", skip_all, fields(paths = paths.len()))]
    pub fn process_changes(&self, paths: &[PathBuf]) -> Result<()> {
        self.with_writer(|writer| self.process_changes_with(writer, paths))
    }

    fn process_changes_with(&self, writer: &mut IndexWriter, paths: &[PathBuf]) -> Result<()> {
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;
        let filename_field = schema.get_field("filename").context("Schema error")?;
        let content_field = schema.get_field("content").context("Schema error")?;
        let is_directory_field = schema.get_field("is_directory").context("Schema error")?;

        for path in paths {
            if path.exists() {
                if let Err(e) = self.index_single_file(
                    path,
                    writer,
                    path_field,
                    filename_field,
                    content_field,
                    is_directory_field,
                ) {
                    tracing::warn!("Failed to update index for {:?}: {}", path, e);
                }
            } else {
                let path_str = path.to_string_lossy();
                writer.delete_term(Term::from_field_text(path_field, &path_str));
            }
        }

        if let Err(e) = writer.commit() {
            tracing::error!("Failed to commit index updates: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    /// Re-indexes (or removes if missing) the single file at `path`.
    pub fn update_file(&self, path: &Path) -> Result<()> {
        self.process_changes(&[path.to_path_buf()])
    }

    /// Where the default index lives and what it covers.
    pub fn default_location() -> Result<(PathBuf, PathBuf)> {
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        Ok((
            home_dir.join(".nohrs").join("index"),
            home_dir.join("Documents"),
        ))
    }
}

/// Read-only access to an index someone else built.
///
/// [`IndexManager`] opens a writer, and tantivy's writer takes an exclusive lock
/// on the index directory: a second process — `noh search` while the app is
/// running — would be refused one. Answering a query needs no writer, so every
/// read-only caller goes through this instead and can share the index with a
/// running GUI.
pub struct IndexReader {
    index: Index,
    index_path: PathBuf,
    content_root: PathBuf,
}

impl IndexReader {
    /// Opens the default index, or `None` when nothing has built one yet.
    ///
    /// Deliberately does not create one: an empty index left behind by a reader
    /// is indistinguishable from a real but useless one, and every later search
    /// would answer "no results" from it.
    pub fn open_default() -> Result<Option<Self>> {
        let (index_path, content_root) = IndexManager::default_location()?;
        Self::open(index_path, content_root)
    }

    /// Opens the index at `index_path`, said to cover `content_root`.
    ///
    /// Returns `None` when there is no index there, and an error when there is
    /// one that cannot answer queries (an index left by an older schema).
    pub fn open(index_path: PathBuf, content_root: PathBuf) -> Result<Option<Self>> {
        if !index_path.join("meta.json").exists() {
            return Ok(None);
        }
        let index = Index::open_in_dir(&index_path)
            .with_context(|| format!("cannot open the index at {}", index_path.display()))?;
        let schema = index.schema();
        for field in ["path", "filename", "content"] {
            schema.get_field(field).with_context(|| {
                format!(
                    "the index at {} was built by an older version of nohrs (no `{field}` field); rebuild it",
                    index_path.display()
                )
            })?;
        }
        Ok(Some(Self {
            index,
            index_path,
            content_root,
        }))
    }

    /// Where the index itself is stored.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// The tree the index was built from.
    pub fn content_root(&self) -> &Path {
        &self.content_root
    }

    /// How many documents the index holds. Zero means it exists but has not
    /// been filled in, which answers no query correctly.
    pub fn document_count(&self) -> Result<u64> {
        Ok(self.index.reader()?.searcher().num_docs())
    }

    /// Whether `path` is inside the tree the index covers.
    pub fn covers(&self, path: &Path) -> bool {
        path.starts_with(&self.content_root)
    }

    /// The paths the index considers the best matches for `query`, best first,
    /// at most `pool` of them.
    ///
    /// These are candidates, not results: the index knows which documents hold
    /// the query's terms, not where in the file they are, so a caller that wants
    /// lines matches them itself.
    pub fn candidates(&self, query: &str, pool: usize) -> Result<Vec<PathBuf>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Field not found")?;
        let filename_field = schema.get_field("filename").context("Field not found")?;
        let content_field = schema.get_field("content").context("Field not found")?;

        let query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![filename_field, content_field],
        );
        let parsed = query_parser
            .parse_query(query)
            .with_context(|| format!("the index cannot read `{query}` as a query"))?;
        // Scored order: this is the BM25 ranking the index exists to provide,
        // and the caller keeps it, so the best candidates survive a limit.
        let top_docs = searcher.search(
            &parsed,
            &tantivy::collector::TopDocs::with_limit(pool).order_by_score(),
        )?;

        let mut paths = Vec::with_capacity(top_docs.len());
        for (_score, address) in top_docs {
            let document: TantivyDocument = searcher.doc(address)?;
            if let Some(path) = document
                .get_first(path_field)
                .and_then(|value| value.as_str())
            {
                paths.push(PathBuf::from(path));
            }
        }
        Ok(paths)
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

        // Removed limit from TopDocs to return all results
        let top_docs = searcher.search(
            &query,
            &tantivy::collector::TopDocs::with_limit(10000).order_by_score(),
        )?;

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
// Runs as part of the search backend, off the GPUI foreground loop, so the
// blocking read does not stall rendering.
#[allow(clippy::disallowed_methods)]
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    /// A manager over a temporary index, plus the tree it covers.
    fn staged() -> (tempfile::TempDir, IndexManager) {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("index");
        let content_root = dir.path().join("content");
        std::fs::create_dir_all(&content_root).unwrap();
        std::fs::write(content_root.join("notes.txt"), "a needle in here\n").unwrap();
        let manager = IndexManager::new_with_path(index_path, content_root).unwrap();
        (dir, manager)
    }

    #[test]
    fn what_a_manager_builds_a_reader_can_read() {
        let (dir, manager) = staged();
        manager.index_home(None).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), dir.path().join("content"))
            .unwrap()
            .expect("an index that was just built");

        assert!(reader.document_count().unwrap() > 0);
        let candidates = reader.candidates("needle", 10).unwrap();
        assert_eq!(candidates, vec![dir.path().join("content/notes.txt")]);
    }

    #[test]
    fn a_reader_can_be_opened_while_a_writer_is_held() {
        let (dir, manager) = staged();
        manager.index_home(None).unwrap();
        // `manager` still owns the writer here; opening for reading must not
        // wait on it, which is what lets `noh search` run beside the app.
        let reader = IndexReader::open(dir.path().join("index"), dir.path().join("content"))
            .unwrap()
            .expect("an index that was just built");
        assert!(reader.document_count().unwrap() > 0);
    }

    #[test]
    fn opening_a_reader_never_creates_an_index() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("index");

        assert!(
            IndexReader::open(missing.clone(), dir.path().to_path_buf())
                .unwrap()
                .is_none()
        );
        assert!(
            !missing.exists(),
            "reading brought an empty index into being"
        );
    }
}
