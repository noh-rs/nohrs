use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::TantivyDocument;
use tantivy::schema::{
    FAST, Field, IndexRecordOption, STORED, STRING, Schema, TEXT, Term, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::tokenizer::{NgramTokenizer, TextAnalyzer};
use tantivy::{Index, IndexWriter}; // Import trait for add_text etc? No, TantivyDocument implements it.

/// Name the n-gram tokenizer is registered under. Tantivy resolves a field's
/// tokenizer by this name at both index and query time, so the registration in
/// [`IndexManager::new_internal`] and the `set_tokenizer` call in
/// [`IndexManager::create_schema`] have to agree on it.
const NGRAM_TOKENIZER: &str = "ngram3";

/// Gram width. Trigrams are what SQLite's FTS5 uses for the same job, and the
/// floor on a substring query: a shorter needle produces no grams at all and so
/// cannot be answered from this index (see [`IndexManager::substring_query`]).
const NGRAM_SIZE: usize = 3;

/// Every field the current schema declares. An index that predates any of them
/// is rebuilt from scratch rather than queried through a schema it does not
/// have — see [`IndexManager::new_internal`]. Adding a field to
/// [`IndexManager::create_schema`] means adding it here too, which is what
/// makes existing indexes pick it up.
const SCHEMA_FIELDS: [&str; 7] = [
    "path",
    "filename",
    "content",
    "last_modified",
    "is_directory",
    "filename_ngram",
    "path_ngram",
];

/// The schema's fields, looked up once.
///
/// Without this, every function that writes a document takes one `Field`
/// parameter per field and every caller re-resolves them by name; adding the
/// two n-gram fields would have made that eight positional arguments of the
/// same type, which is a swap waiting to happen.
#[derive(Clone, Copy)]
struct Fields {
    path: Field,
    filename: Field,
    content: Field,
    is_directory: Field,
    filename_ngram: Field,
    path_ngram: Field,
}

impl Fields {
    fn resolve(schema: &Schema) -> Result<Self> {
        let field = |name: &str| {
            schema
                .get_field(name)
                .with_context(|| format!("Schema error: {name} field missing"))
        };
        Ok(Self {
            path: field("path")?,
            filename: field("filename")?,
            content: field("content")?,
            is_directory: field("is_directory")?,
            filename_ngram: field("filename_ngram")?,
            path_ngram: field("path_ngram")?,
        })
    }
}

/// Owns the tantivy index and its writer, and performs full and incremental indexing.
pub struct IndexManager {
    index: Index,
    _index_path: PathBuf,
    content_root: PathBuf,
    writer: Arc<Mutex<IndexWriter>>,
}

impl IndexManager {
    /// Opens (or creates) the default index under `~/.nohrs/index` rooted at `~/Documents`.
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

            let missing: Vec<&str> = SCHEMA_FIELDS
                .iter()
                .copied()
                .filter(|name| existing_schema.get_field(name).is_err())
                .collect();

            if missing.is_empty() {
                existing_index
            } else {
                // An index written before a field existed has no postings for
                // it, so querying that field would silently return nothing.
                // Rebuilding is the only way to populate it.
                tracing::info!(
                    "Schema outdated (missing {}), recreating index...",
                    missing.join(", ")
                );
                drop(existing_index);
                // Delete old index
                if let Err(e) = fs::remove_dir_all(&index_path) {
                    tracing::warn!("Failed to remove old index: {}", e);
                }
                fs::create_dir_all(&index_path)?;
                Index::create_in_dir(&index_path, schema)?
            }
        } else {
            Index::create_in_dir(&index_path, schema)?
        };

        // The n-gram fields name this tokenizer in their schema options, so it
        // has to be registered before the index is written to or queried —
        // including on the branch that opened an existing index, whose manager
        // is registered per process rather than stored on disk.
        //
        // No `LowerCaser` here: case folding happens in [`ngram_source`],
        // before the grams are cut, and doing it twice in two places is how the
        // boundary bug it fixes got missed in the first place.
        index.tokenizers().register(
            NGRAM_TOKENIZER,
            TextAnalyzer::builder(NgramTokenizer::all_ngrams(NGRAM_SIZE, NGRAM_SIZE)?).build(),
        );

        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            _index_path: index_path,
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

        // filename_ngram / path_ngram: the same text again, cut into trigrams,
        // so that a needle landing inside a token can still be found. The
        // default tokenizer above splits on word boundaries, which is why
        // `filename` alone cannot answer `*er_fil*` against
        // "explorer_file_ops.rs" (ADR 0009).
        //
        // Frequencies without positions, because the grams are a candidate
        // filter rather than the answer. `NgramTokenizer::all_ngrams` emits
        // every gram of a term at the same position, so a phrase query over
        // them cannot tell `abcd` from `abc---bcd` — both contain the grams
        // `abc` and `bcd`. Adjacency is therefore checked against the stored
        // text in [`IndexManager::search`], and storing positions here would
        // cost index size for a guarantee they do not provide.
        let ngram_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(NGRAM_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqs),
        );
        schema_builder.add_text_field("filename_ngram", ngram_options.clone());
        // `path` itself stays `STRING`: it is the deletion key, matched as an
        // exact term by `delete_term`, and tokenizing it would break that.
        schema_builder.add_text_field("path_ngram", ngram_options);

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
    pub fn index_home(&self, mut progress_tx: Option<postage::watch::Sender<f32>>) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Poisoned lock: {}", e))?;
        let fields = Fields::resolve(&self.index.schema())?;

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
                        if let Err(e) = self.index_single_file(path, &mut writer_guard, fields) {
                            tracing::warn!("Failed to index file {:?}: {}", path, e);
                        }
                    } else if path.is_dir()
                        && let Err(e) = self.index_single_directory(path, &mut writer_guard, fields)
                    {
                        tracing::warn!("Failed to index directory {:?}: {}", path, e);
                    }

                    // Update progress
                    processed += 1;
                    if let Some(tx) = &mut progress_tx
                        && total_files > 0
                        && processed % 100 == 0
                    {
                        *tx.borrow_mut() = processed as f32 / total_files as f32;
                    }
                }
                Err(err) => tracing::warn!("Walk error: {}", err),
            }
        }

        if let Some(tx) = &mut progress_tx {
            *tx.borrow_mut() = 1.0; // Done
        }

        writer_guard.commit()?;
        Ok(())
    }

    fn index_single_directory(
        &self,
        path: &Path,
        writer: &mut IndexWriter,
        fields: Fields,
    ) -> Result<()> {
        let path_str = path.to_string_lossy();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        let mut doc = TantivyDocument::default();
        doc.add_text(fields.path, &path_str);
        doc.add_text(fields.filename, &filename);
        doc.add_text(fields.content, &filename); // Allow searching dir by name content
        doc.add_u64(fields.is_directory, 1);
        doc.add_text(fields.filename_ngram, ngram_source(&filename));
        doc.add_text(fields.path_ngram, ngram_source(&path_str));

        writer.delete_term(Term::from_field_text(fields.path, &path_str));
        writer.add_document(doc)?;
        Ok(())
    }

    fn index_single_file(
        &self,
        path: &Path,
        writer: &mut IndexWriter,
        fields: Fields,
    ) -> Result<()> {
        let metadata = fs::metadata(path)?;
        // The watcher hands this function every changed path, directories
        // included, and a directory has no body to read. Writing one through
        // here would replace its document with `is_directory = 0` and turn it
        // into a file until the next full re-index.
        if metadata.is_dir() {
            return self.index_single_directory(path, writer, fields);
        }

        let path_str = path.to_string_lossy();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        // Only the body is conditional. A photo, a 2 GB disk image and a file
        // the user cannot read are all still things they search for by name, so
        // the document is written either way and skipping costs only `content`.
        let body = indexable_body(path, &metadata);

        let mut doc = TantivyDocument::default();
        doc.add_text(fields.path, &path_str);
        doc.add_text(fields.filename, &filename);
        doc.add_u64(fields.is_directory, 0);
        // Only the name and the path get n-grams. Applying them to `content`
        // would multiply the postings for every file body in the home
        // directory, which ADR 0009 rules out.
        doc.add_text(fields.filename_ngram, ngram_source(&filename));
        doc.add_text(fields.path_ngram, ngram_source(&path_str));
        if let Some(content) = body {
            // The path goes into `content` as well, so a full-text query can
            // reach the file by where it lives and not only by what is in it.
            doc.add_text(fields.content, format!("{path_str}\n{content}"));
        }

        // Delete existing doc with same path to avoid duplicates (upsert)
        // Note: This matches exact path string.
        writer.delete_term(Term::from_field_text(fields.path, &path_str));
        writer.add_document(doc)?;
        Ok(())
    }

    /// Removes the document for `path` from the index and commits.
    #[tracing::instrument(target = "nohrs::op", name = "index.remove", level = "debug", skip_all, fields(path = %path.display()))]
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

    /// Returns a reference to the underlying tantivy index.
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Re-indexes or removes each of `paths` (depending on existence) and commits once.
    #[tracing::instrument(target = "nohrs::op", name = "index.process_changes", level = "debug", skip_all, fields(paths = paths.len()))]
    pub fn process_changes(&self, paths: &[PathBuf]) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow::anyhow!("Poisoned lock: {}", e))?;
        let fields = Fields::resolve(&self.index.schema())?;

        for path in paths {
            if path.exists() {
                if let Err(e) = self.index_single_file(path, &mut writer_guard, fields) {
                    tracing::warn!("Failed to update index for {:?}: {}", path, e);
                }
            } else {
                let path_str = path.to_string_lossy();
                writer_guard.delete_term(Term::from_field_text(fields.path, &path_str));
            }
        }

        if let Err(e) = writer_guard.commit() {
            tracing::error!("Failed to commit index updates: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    /// Re-indexes (or removes if missing) the single file at `path`.
    pub fn update_file(&self, path: &Path) -> Result<()> {
        self.process_changes(&[path.to_path_buf()])
    }

    /// Narrows `*needle*` to the documents that could contain it: every one of
    /// the needle's trigrams must appear in the name or the path.
    ///
    /// This is a filter, not the answer. Grams carry no position (see
    /// [`IndexManager::create_schema`]), so `abc---bcd` survives a query for
    /// `abcd`; [`IndexManager::search`] confirms each survivor against the
    /// stored text. SQLite's FTS5 trigram index works the same way, for the
    /// same reason.
    fn substring_query(
        &self,
        needle: &str,
        fields: Fields,
    ) -> Result<Box<dyn tantivy::query::Query>> {
        // Fewer characters than one gram produces an empty token stream, and a
        // query with no clauses matches nothing. Saying so beats returning zero
        // hits and letting the user conclude the substring is absent.
        if needle.chars().count() < NGRAM_SIZE {
            anyhow::bail!(
                "substring search needs at least {NGRAM_SIZE} characters; `{needle}` has {}",
                needle.chars().count()
            );
        }

        // Cut by the same analyzer the fields were indexed with, rather than by
        // a second implementation of the same rule here — that way the grams
        // agree with the index by construction, lowercasing included.
        let mut analyzer = self
            .index
            .tokenizers()
            .get(NGRAM_TOKENIZER)
            .context("n-gram tokenizer not registered")?;
        let mut grams = Vec::new();
        let folded = ngram_source(needle);
        let mut stream = analyzer.token_stream(&folded);
        while let Some(token) = stream.next() {
            grams.push(token.text.clone());
        }

        let mut clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        for gram in grams {
            // A gram may sit in either field, so each is its own OR pair, and
            // the pairs are required together: every gram present, in one or
            // the other.
            let either: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> =
                [fields.filename_ngram, fields.path_ngram]
                    .into_iter()
                    .map(|field| {
                        let term = Term::from_field_text(field, &gram);
                        let query: Box<dyn tantivy::query::Query> = Box::new(
                            tantivy::query::TermQuery::new(term, IndexRecordOption::WithFreqs),
                        );
                        (tantivy::query::Occur::Should, query)
                    })
                    .collect();
            clauses.push((
                tantivy::query::Occur::Must,
                Box::new(tantivy::query::BooleanQuery::new(either)),
            ));
        }

        Ok(Box::new(tantivy::query::BooleanQuery::new(clauses)))
    }
}

/// Largest body this index will hold. Above it a file is searchable by name and
/// path alone.
const MAX_INDEXED_BODY_BYTES: u64 = 10 * 1024 * 1024;

/// The text of `path` if this index holds its body, `None` if the file is
/// searchable by name and path alone.
///
/// One decision in one place, because both callers have to agree on it:
/// [`IndexManager::index_single_file`] writes the `content` field from it, and
/// [`find_all_match_lines`] must not read a body the index never held — which
/// for a multi-gigabyte disk image matched by its name would mean reading the
/// whole thing into memory to return a hit that carries no line at all.
fn indexable_body(path: &Path, metadata: &fs::Metadata) -> Option<String> {
    if metadata.len() > MAX_INDEXED_BODY_BYTES {
        tracing::debug!("Name only, file is large: {path:?}");
        return None;
    }

    // Indexing and searching both run on the search backend's own threads,
    // never the GPUI foreground loop, so the blocking read is fine here.
    #[allow(clippy::disallowed_methods)]
    match fs::read_to_string(path) {
        // A null byte means binary that happened to decode as UTF-8.
        Ok(content) if content.contains('\0') => {
            tracing::debug!("Name only, file looks binary: {path:?}");
            None
        }
        Ok(content) => Some(content),
        Err(error) => {
            tracing::debug!("Name only, file is unreadable: {path:?}: {error}");
            None
        }
    }
}

/// The form the n-gram fields are cut from, at index and query time alike.
///
/// Folding case has to happen *before* the grams are cut, not as a token filter
/// after them. `İ` folds to two characters, so filtering afterwards leaves the
/// index cutting on the original boundaries while the query cuts on the folded
/// ones, and the two never meet — measured, and it is what a decomposed macOS
/// name runs into.
///
/// This folds case and nothing else, so `İ` reaches `i` plus a combining dot
/// rather than a bare `i`. The plain query path answers the same way; being
/// blind to combining marks would have to be decided for both.
fn ngram_source(text: &str) -> String {
    text.to_lowercase()
}

/// Whether `haystack` actually contains `needle`, ignoring case.
///
/// The n-gram index cannot answer this on its own, so it is what turns a
/// candidate into a hit.
fn contains_ignoring_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Recognises the `*abc*` substring form and returns `abc`.
///
/// Anything else — including a bare `*`, `**`, and a leading-or-trailing-only
/// star — is left to the ordinary parser, so that a query which merely contains
/// a star is not silently reinterpreted.
fn substring_needle(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    let inner = trimmed.strip_prefix('*')?.strip_suffix('*')?;
    if inner.is_empty() || inner.contains('*') {
        return None;
    }
    Some(inner)
}

impl super::backend::SearchBackend for IndexManager {
    fn search(&self, query_str: &str) -> Result<Vec<super::SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        let fields = Fields::resolve(&self.index.schema())?;
        let path_field = fields.path;
        let is_directory_field = fields.is_directory;

        let needle = substring_needle(query_str);
        let query = match needle {
            Some(needle) => self.substring_query(needle, fields)?,
            None => tantivy::query::QueryParser::for_index(
                &self.index,
                vec![fields.filename, fields.content],
            )
            .parse_query(query_str)?,
        };
        // A `*abc*` hit is a name or path match, so the per-line scan below
        // looks for `abc` rather than for the wrapped form the user typed,
        // which appears in no file.
        let line_needle = needle.unwrap_or(query_str);

        // A substring query collects every candidate rather than the top N.
        // Its hits are decided below, by whether the stored path holds the
        // needle — a question scoring knows nothing about — so cutting to the
        // best-scoring candidates first would discard true matches that happen
        // to score low. Measured: 10,500 decoys sharing the needle's grams plus
        // one long-named true match, and `*aaabbb*` returned nothing at all.
        // Ordinary queries keep the top-N cut, where score is the answer.
        let addresses: Vec<tantivy::DocAddress> = if needle.is_some() {
            searcher
                .search(&query, &tantivy::collector::DocSetCollector)?
                .into_iter()
                .collect()
        } else {
            searcher
                .search(
                    &query,
                    &tantivy::collector::TopDocs::with_limit(10000).order_by_score(),
                )?
                .into_iter()
                .map(|(_score, address)| address)
                .collect()
        };

        let mut results = Vec::new();
        for doc_address in addresses {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            // value is OwnedValue.
            // In Tantivy 0.22, retrieved_doc.get_first(field) returns Option<&OwnedValue>.
            // OwnedValue has as_str() if it's a string.
            // We need to import Value trait if we want to use generic accessors,
            // but OwnedValue might have direct methods.
            // Let's rely on explicit match or Debug to find out what works if as_str() fails.
            // Actually, for OwnedValue, it is an enum.
            // If I import Value trait, I can use .as_str().

            if let Some(path_val) = retrieved_doc.get_first(path_field)
                && let Some(path_str) = path_val.as_str()
            {
                // The gram filter admits documents holding every gram of the
                // needle in any arrangement, so `*abcd*` reaches `abc---bcd`.
                // The stored path settles it. Nothing is read from disk: `path`
                // is `STORED`, and the name is its last component.
                if let Some(needle) = needle
                    && !contains_ignoring_case(path_str, needle)
                {
                    continue;
                }

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
                        let match_lines = find_all_match_lines(&path_buf, line_needle);

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
        Ok(results)
    }
}

/// Find ALL lines in a file that match the query (case-insensitive)
// Runs as part of the search backend, off the GPUI foreground loop, so the
// blocking read does not stall rendering.
#[allow(clippy::disallowed_methods)]
fn find_all_match_lines(path: &Path, query: &str) -> Vec<(usize, String)> {
    let mut matches = Vec::new();
    let body = match fs::metadata(path) {
        Ok(metadata) => indexable_body(path, &metadata),
        Err(error) => {
            // A file the index knows about but cannot be stat'd now — deleted
            // between the search and this read, or permissions changed. The
            // name match still stands, so this is a missing line rather than a
            // missing hit, but it should not pass in silence.
            tracing::debug!("Name only, metadata unavailable: {path:?}: {error}");
            None
        }
    };
    if let Some(content) = body {
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
// Fixture trees are built with `std::fs::write`, which is banned in app code to
// keep blocking I/O off the GPUI foreground thread. Same exemption as
// `file_index.rs`.
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::search::backend::SearchBackend;

    fn indexed(root: &Path, files: &[(&str, &str)]) -> IndexManager {
        for (name, contents) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let manager = IndexManager::new_with_path(root.join(".index"), root.to_path_buf()).unwrap();
        manager.index_home(None).unwrap();
        manager
    }

    fn hit_names(results: &[crate::search::SearchResult]) -> Vec<String> {
        let mut names: Vec<String> = results
            .iter()
            .filter_map(|result| result.path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    #[test]
    fn a_substring_inside_a_token_is_found_where_a_plain_query_misses_it() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("explorer_file_ops.rs", "fn apply() {}")]);

        // `er_fil` straddles the boundary the default tokenizer splits on, so
        // the ordinary query cannot see it. This is the whole reason the n-gram
        // fields exist (ADR 0009).
        let plain = manager.search("er_fil").unwrap();
        assert!(
            !hit_names(&plain).contains(&"explorer_file_ops.rs".to_string()),
            "plain query unexpectedly matched: {:?}",
            hit_names(&plain)
        );

        let substring = manager.search("*er_fil*").unwrap();
        assert!(
            hit_names(&substring).contains(&"explorer_file_ops.rs".to_string()),
            "substring query missed the file: {:?}",
            hit_names(&substring)
        );
    }

    #[test]
    fn a_substring_query_matches_the_path_as_well_as_the_name() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("deeply/nested/leaf.txt", "x")]);

        // `epl` appears only in the `deeply` directory component, never in the
        // file's own name.
        let results = manager.search("*epl*").unwrap();
        assert!(
            hit_names(&results).contains(&"leaf.txt".to_string()),
            "path substring missed the file: {:?}",
            hit_names(&results)
        );
    }

    #[test]
    fn a_substring_query_answers_only_with_files_that_contain_it() {
        let dir = tempfile::tempdir().unwrap();
        // `abc---bcd.txt` holds every trigram of `abcd` — `abc` and `bcd` —
        // without holding `abcd`. The gram index alone cannot separate the two,
        // so this is what the stored-text check in `search` exists for.
        let manager = indexed(
            dir.path(),
            &[
                ("abc---bcd.txt", "x"),
                ("zzabcdzz.txt", "x"),
                ("alpha.txt", "x"),
            ],
        );

        assert_eq!(
            hit_names(&manager.search("*abcd*").unwrap()),
            vec!["zzabcdzz.txt".to_string()],
            "a file holding the grams but not the substring must not be a hit"
        );

        // Order is part of the same guarantee: `alpha` holds `lph`, never `hpl`.
        assert!(
            hit_names(&manager.search("*lph*").unwrap()).contains(&"alpha.txt".to_string()),
            "the substring itself should match"
        );
        assert!(
            !hit_names(&manager.search("*hpl*").unwrap()).contains(&"alpha.txt".to_string()),
            "a reversed substring must not match"
        );
    }

    fn is_directory_flag(manager: &IndexManager, path: &Path) -> Option<u64> {
        let reader = manager.index().reader().ok()?;
        let searcher = reader.searcher();
        let fields = Fields::resolve(&manager.index().schema()).ok()?;
        let term = Term::from_field_text(fields.path, &path.to_string_lossy());
        let query = tantivy::query::TermQuery::new(term, IndexRecordOption::Basic);
        let address = searcher
            .search(&query, &tantivy::collector::DocSetCollector)
            .ok()?
            .into_iter()
            .next()?;
        let document: TantivyDocument = searcher.doc(address).ok()?;
        document.get_first(fields.is_directory)?.as_u64()
    }

    #[test]
    fn a_directory_stays_a_directory_when_its_own_change_is_processed() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("papers/notes.txt", "x")]);
        let papers = dir.path().join("papers");

        assert_eq!(
            is_directory_flag(&manager, &papers),
            Some(1),
            "the initial index should have it as a directory"
        );

        // The watcher forwards directory paths here too, and a directory cannot
        // be read as a body.
        manager
            .process_changes(std::slice::from_ref(&papers))
            .unwrap();

        assert_eq!(
            is_directory_flag(&manager, &papers),
            Some(1),
            "re-processing a directory must not turn it into a file"
        );
    }

    #[test]
    fn a_body_the_index_does_not_hold_is_not_read_to_answer_a_name_match() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // The needle is in the name *and* in the body, so a hit carrying a line
        // proves the body was read. Both files are ones the index stores by
        // name alone: one binary, one over the size limit.
        std::fs::write(root.join("holiday_snapshot.png"), "\u{0}day_snap").unwrap();
        let oversized = format!("day_snap\n{}", "p".repeat(MAX_INDEXED_BODY_BYTES as usize));
        std::fs::write(root.join("day_snap_archive.bin"), oversized).unwrap();

        let manager = IndexManager::new_with_path(root.join(".index"), root.to_path_buf()).unwrap();
        manager.index_home(None).unwrap();

        let results = manager.search("*day_snap*").unwrap();
        assert_eq!(
            hit_names(&results),
            vec![
                "day_snap_archive.bin".to_string(),
                "holiday_snapshot.png".to_string()
            ],
            "both should match by name"
        );
        assert!(
            results
                .iter()
                .all(|result| result.line_number == 0 && result.line_content.is_empty()),
            "a name-only hit must carry no line: {results:?}"
        );
    }

    #[test]
    fn a_file_whose_body_is_not_indexed_is_still_findable_by_name() {
        let dir = tempfile::tempdir().unwrap();
        // A null byte is what the body check rejects as binary. Before, that
        // rejection dropped the whole document, so a photo or a disk image was
        // unreachable by name as well as by content.
        let manager = indexed(dir.path(), &[("holiday_snapshot.png", "\u{0}PNG")]);

        assert!(
            hit_names(&manager.search("*day_snap*").unwrap())
                .contains(&"holiday_snapshot.png".to_string()),
            "a file with an unindexable body must still answer a name search"
        );
    }

    #[test]
    fn a_needle_that_folds_to_more_characters_than_it_has_still_matches() {
        let dir = tempfile::tempdir().unwrap();
        // macOS stores names decomposed, so this file is `I` plus a combining
        // dot — exactly what the precomposed `\u{130}` in the query below folds
        // to. Folding as a token filter, after the grams were already cut, left
        // the index holding a 4-character `i\u{307}st` while the query asked for
        // the 3-character grams of the folded form, and the two never met.
        let manager = indexed(dir.path(), &[("I\u{307}stanbul.txt", "x")]);

        assert_eq!(
            hit_names(&manager.search("*\u{130}stanbul*").unwrap()),
            vec!["I\u{307}stanbul.txt".to_string()],
            "a needle whose folded form is longer than itself must still match"
        );

        // The other half of the same rule: `\u{130}` folds to `i` plus a
        // combining dot, never to a bare `i`, so an ASCII needle does not reach
        // it. That is Unicode default caseless matching, and the plain query
        // path answers the same way — measured. Making either blind to
        // combining marks is a change to both paths, not to this one.
        assert!(
            hit_names(&manager.search("*istanbul*").unwrap()).is_empty(),
            "an ASCII needle is not expected to reach a dotted capital I"
        );
    }

    // 10,500 files is far too slow for the default suite, so this one is opt-in:
    // `cargo test -p nohrs-services --release -- --ignored`. It is kept because
    // nothing cheaper distinguishes collecting every candidate from taking the
    // best-scoring 10,000 of them, and that distinction was a silent bug.
    #[test]
    #[ignore = "indexes 10,500 files"]
    fn a_true_match_is_not_lost_behind_better_scoring_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for index in 0..10_500 {
            std::fs::write(root.join(format!("aaa-abb-aab-bbb-{index}.txt")), "x").unwrap();
        }
        // Long, so BM25's length normalisation scores it below every decoy —
        // and it is the only file that actually contains `aaabbb`.
        let long = format!("{}_aaabbb_{}.txt", "q".repeat(120), "w".repeat(120));
        std::fs::write(root.join(&long), "x").unwrap();

        let manager = IndexManager::new_with_path(root.join(".index"), root.to_path_buf()).unwrap();
        manager.index_home(None).unwrap();

        assert_eq!(
            hit_names(&manager.search("*aaabbb*").unwrap()),
            vec![long],
            "the only true match must survive the decoys"
        );
    }

    #[test]
    fn a_needle_shorter_than_one_gram_is_an_error_rather_than_an_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("alpha.txt", "x")]);

        let error = manager.search("*al*").unwrap_err().to_string();
        assert!(
            error.contains("at least 3 characters"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_needle_carrying_query_syntax_is_matched_as_text() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("a:b[c].txt", "x")]);

        // A needle never reaches a query parser, so parser syntax in it is
        // just more characters to cut into grams.
        let results = manager.search("*a:b[c]*").unwrap();
        assert!(
            hit_names(&results).contains(&"a:b[c].txt".to_string()),
            "syntax-bearing needle missed the file: {:?}",
            hit_names(&results)
        );
    }

    #[test]
    fn a_substring_query_folds_case_like_every_other_search_path() {
        let dir = tempfile::tempdir().unwrap();
        let manager = indexed(dir.path(), &[("ExplorerFileOps.rs", "x")]);

        for needle in ["*rerfile*", "*rerFile*", "*RERFILE*"] {
            assert!(
                hit_names(&manager.search(needle).unwrap())
                    .contains(&"ExplorerFileOps.rs".to_string()),
                "{needle} missed the file"
            );
        }
    }

    #[test]
    fn only_the_wrapped_form_is_treated_as_a_substring_query() {
        assert_eq!(substring_needle("*abc*"), Some("abc"));
        assert_eq!(substring_needle("  *abc*  "), Some("abc"));
        // A bare or doubled star names no needle, and a one-sided star is a
        // plain query that happens to contain one.
        assert_eq!(substring_needle("*"), None);
        assert_eq!(substring_needle("**"), None);
        assert_eq!(substring_needle("*abc"), None);
        assert_eq!(substring_needle("abc*"), None);
        assert_eq!(substring_needle("abc"), None);
        // Two needles in one query is not a form this answers.
        assert_eq!(substring_needle("*a*b*"), None);
    }

    #[test]
    fn an_index_missing_a_field_is_rebuilt_rather_than_queried_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join(".index");

        // An index written before the n-gram fields existed.
        std::fs::create_dir_all(&index_path).unwrap();
        let mut old = Schema::builder();
        old.add_text_field("path", STRING | STORED);
        old.add_text_field("filename", TEXT | STORED);
        old.add_text_field("content", TEXT);
        old.add_u64_field("last_modified", FAST);
        old.add_u64_field("is_directory", FAST | STORED);
        drop(Index::create_in_dir(&index_path, old.build()).unwrap());

        std::fs::write(dir.path().join("explorer_file_ops.rs"), "fn apply() {}").unwrap();
        let manager = IndexManager::new_with_path(index_path, dir.path().to_path_buf()).unwrap();
        manager.index_home(None).unwrap();

        // Opening it would otherwise succeed and then answer every substring
        // query with nothing, because the field has no postings.
        for name in SCHEMA_FIELDS {
            assert!(
                manager.index().schema().get_field(name).is_ok(),
                "{name} missing after reopen"
            );
        }
        assert!(
            hit_names(&manager.search("*er_fil*").unwrap())
                .contains(&"explorer_file_ops.rs".to_string()),
            "rebuilt index did not answer a substring query"
        );
    }
}
