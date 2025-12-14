use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tantivy::schema::{Field, Schema, Term, Value, FAST, STORED, TEXT};
use tantivy::TantivyDocument;
use tantivy::{Index, IndexWriter}; // Import trait for add_text etc? No, TantivyDocument implements it.

pub struct IndexManager {
    index: Index,
    index_path: PathBuf,
    content_root: PathBuf,
}

impl IndexManager {
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let nohrs_dir = home_dir.join(".nohrs");
        let index_path = nohrs_dir.join("index");

        Self::new_internal(index_path, home_dir)
    }

    /// Internal constructor for testing or custom paths
    pub fn new_with_path(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        Self::new_internal(index_path, content_root)
    }

    fn new_internal(index_path: PathBuf, content_root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&index_path)?;

        let schema = Self::create_schema();

        let index = if index_path.join("meta.json").exists() {
            Index::open_in_dir(&index_path)?
        } else {
            Index::create_in_dir(&index_path, schema)?
        };

        Ok(Self {
            index,
            index_path,
            content_root,
        })
    }

    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // path: stored and indexed as text
        schema_builder.add_text_field("path", TEXT | STORED);

        // content: indexed but not stored (for full text search)
        schema_builder.add_text_field("content", TEXT);

        // last_modified: fast field for sorting or filtering
        schema_builder.add_u64_field("last_modified", FAST);

        schema_builder.build()
    }

    pub fn writer(&self) -> Result<IndexWriter> {
        // 50MB heap for indexing buffer
        Ok(self.index.writer(50_000_000)?)
    }

    pub fn index_home(&self) -> Result<()> {
        let writer = self.writer()?;
        let schema = self.index.schema();
        let path_field = schema
            .get_field("path")
            .context("Schema error: path field missing")?;
        let content_field = schema
            .get_field("content")
            .context("Schema error: content field missing")?;

        // Use ignore crate to walk home directory respecting .gitignore
        let walker = ignore::WalkBuilder::new(&self.content_root)
            .hidden(false) // Allow hidden files initially, but .gitignore usually handles them.
            // But usually we don't want to index .git/ etc.
            // ignore crate handles .git automatically.
            .git_ignore(true)
            .build();

        let mut index_writer = writer;

        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() {
                        if let Err(e) = self.index_single_file(
                            path,
                            &mut index_writer,
                            path_field,
                            content_field,
                        ) {
                            tracing::warn!("Failed to index file {:?}: {}", path, e);
                        }
                    }
                }
                Err(err) => tracing::warn!("Walk error: {}", err),
            }
        }

        index_writer.commit()?;
        Ok(())
    }

    fn index_single_file(
        &self,
        path: &Path,
        writer: &mut IndexWriter,
        path_field: Field,
        content_field: Field,
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

                let mut doc = TantivyDocument::default();
                doc.add_text(path_field, &path_str);
                doc.add_text(content_field, &content);

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
        let writer = self.writer()?;
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;
        let path_str = path.to_string_lossy();

        // Remove document with matching path
        let mut index_writer = writer;
        index_writer.delete_term(Term::from_field_text(path_field, &path_str));
        index_writer.commit()?;

        Ok(())
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn update_file(&self, path: &Path) -> Result<()> {
        let writer = self.writer()?;
        let schema = self.index.schema();
        let path_field = schema.get_field("path").context("Schema error")?;
        let content_field = schema.get_field("content").context("Schema error")?;

        let mut index_writer = writer;
        self.index_single_file(path, &mut index_writer, path_field, content_field)?;
        index_writer.commit()?;
        Ok(())
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
        let content_field = schema.get_field("content").context("Field not found")?;

        let query_parser =
            tantivy::query::QueryParser::for_index(&self.index, vec![path_field, content_field]);
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
                    results.push(super::SearchResult {
                        path: path_buf,
                        line_number: 1,
                        line_content: "Refinement needed: Line finding".to_string(),
                    });
                }
            }
        }
        Ok(results)
    }
}
