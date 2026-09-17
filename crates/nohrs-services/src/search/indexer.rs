use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tantivy::TantivyDocument;
use tantivy::schema::{FAST, Field, STORED, STRING, Schema, TEXT, Term, Value};
use tantivy::{Index, IndexWriter}; // Import trait for add_text etc? No, TantivyDocument implements it.

/// How much heap tantivy's writer may use while indexing.
const WRITER_HEAP_BYTES: usize = 50_000_000;

/// Files larger than this carry no content into the index (`docs/search.md` §3.5).
const MAX_INDEXED_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// How much of the content root an indexing pass re-reads.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Read only the files whose modification time differs from the index's.
    #[default]
    Changed,
    /// Read every file, whatever the index already holds. For when the index
    /// is suspected of being wrong rather than merely out of date.
    Everything,
}

/// What one indexing pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexReport {
    /// Documents written, whether new or replacing an older one.
    pub indexed: usize,
    /// Files left alone because the index already had them at that time.
    pub unchanged: usize,
    /// Documents dropped because the file is no longer there.
    pub removed: usize,
}

/// The schema fields an indexing pass writes, looked up once.
struct Fields {
    path: Field,
    filename: Field,
    content: Field,
    last_modified: Field,
    is_directory: Field,
}

impl Fields {
    fn of(schema: &Schema) -> Result<Self> {
        let field = |name: &str| {
            schema
                .get_field(name)
                .with_context(|| format!("Schema error: {name} field missing"))
        };
        Ok(Self {
            path: field("path")?,
            filename: field("filename")?,
            content: field("content")?,
            last_modified: field("last_modified")?,
            is_directory: field("is_directory")?,
        })
    }
}

/// When the file was last modified, in nanoseconds since the epoch.
///
/// `None` where the platform or filesystem does not say, which costs that file
/// a re-read on every pass — the only safe reading of "no idea when this
/// changed".
fn modified_nanos(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since_epoch| u64::try_from(since_epoch.as_nanos()).unwrap_or(u64::MAX))
}

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

            // The whole schema is compared, not just which fields exist: a field
            // whose options changed is as unusable as a missing one. That is not
            // hypothetical — `last_modified` became stored so that an
            // incremental pass has a time to compare against, and an index left
            // by the older schema would answer "no time recorded" for every
            // document and re-read the whole tree on every pass, forever.
            if existing_schema != schema {
                tracing::info!("Index schema is out of date, recreating index...");
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

        // path: stored and indexed as exact string (keyword) for ID/deletion,
        // and columnar so that an incremental pass can read every indexed path
        // without decompressing the document store (see `indexed_modifications`).
        schema_builder.add_text_field("path", STRING | STORED | FAST);

        // filename: tokenized for full-text search on file names
        schema_builder.add_text_field("filename", TEXT | STORED);

        // content: indexed but not stored (for full text search)
        schema_builder.add_text_field("content", TEXT);

        // last_modified: stored as well as fast, because an incremental pass
        // compares it against the file on disk to decide whether to read the
        // file at all (see `index_home`). Without it in the document there is
        // nothing to compare, and every pass is a full re-read.
        schema_builder.add_u64_field("last_modified", FAST | STORED);

        // is_directory: fast field (0=false, 1=true)
        schema_builder.add_u64_field("is_directory", FAST | STORED);

        schema_builder.build()
    }

    /// Indexes the content root, reporting progress through `progress_tx` if given.
    ///
    /// [`Refresh::Changed`] reads only the files whose modification time differs
    /// from the one in the index, which is what makes this affordable to run on
    /// every launch and from `noh index build`. Either way, documents whose file
    /// has since disappeared are removed: a pass that only ever adds leaves a
    /// deleted file answering searches forever.
    #[tracing::instrument(
        target = "nohrs::op",
        name = "index.build_home",
        level = "debug",
        skip_all,
        fields(refresh = ?refresh)
    )]
    pub fn index_home(
        &self,
        refresh: Refresh,
        progress_tx: Option<postage::watch::Sender<f32>>,
    ) -> Result<IndexReport> {
        self.with_writer(|writer| self.index_home_with(writer, refresh, progress_tx))
    }

    fn index_home_with(
        &self,
        writer: &mut IndexWriter,
        refresh: Refresh,
        mut progress_tx: Option<postage::watch::Sender<f32>>,
    ) -> Result<IndexReport> {
        let fields = Fields::of(&self.index.schema())?;
        // What the index holds now, by path. The walk reports back which of
        // these it reached, so whatever is left at the end is a document whose
        // file is gone.
        let mut known = self.indexed_modifications(&fields)?;

        // The denominator for progress. A warm index knows how many documents
        // it holds, which is the answer without walking anything; only a cold
        // one pays for a counting pass, and that pass reads the walker's own
        // file type rather than calling `stat` on every entry.
        let tally = Tally::default();
        let progress = progress_tx.take().map(|mut sender| {
            *sender.borrow_mut() = 0.0;
            let total = match known.len() {
                0 => count_entries(&self.content_root),
                documents => documents,
            };
            ProgressTicker::start(sender, tally.walked.clone(), total)
        });
        // Only paths the index already holds need reporting back: orphans are
        // what the index knows and the walk did not reach, so a path that was
        // never indexed says nothing about them.
        let (seen_tx, seen_rx) = std::sync::mpsc::channel::<Vec<String>>();
        walker(&self.content_root)
            .threads(indexing_threads())
            .build_parallel()
            .run(|| {
                // Per worker: the batch flushes itself when the walk drops this
                // visitor, so nothing is lost and nothing is sent per entry.
                let mut batch = SeenBatch::new(seen_tx.clone());
                let tally = &tally;
                let known = &known;
                let fields = &fields;
                let writer = &*writer;
                Box::new(move |result| {
                    let entry = match result {
                        Ok(entry) => entry,
                        Err(error) => {
                            tracing::warn!("Walk error: {}", error);
                            return ignore::WalkState::Continue;
                        }
                    };
                    // Counted before anything can skip the entry, so progress
                    // reaches 1.0 whatever the walk runs into.
                    tally.walked.fetch_add(1, Ordering::Relaxed);

                    let path = entry.path();
                    let indexed_path = path.to_string_lossy();
                    // Reported as reached even when the entry turns out to be
                    // unreadable below: a file that is here but cannot be read
                    // is not a file that is gone.
                    let previously = match known.get(indexed_path.as_ref()) {
                        Some(modified) => {
                            batch.reached(&indexed_path);
                            *modified
                        }
                        None => None,
                    };

                    let metadata = match fs::metadata(path) {
                        Ok(metadata) => metadata,
                        Err(error) => {
                            tracing::debug!("cannot stat {}: {error}", path.display());
                            return ignore::WalkState::Continue;
                        }
                    };
                    let modified = modified_nanos(&metadata);
                    // A file with no readable modification time is re-read every
                    // pass: there is nothing to tell us it has not changed.
                    if refresh == Refresh::Changed && modified.is_some() && previously == modified {
                        tally.unchanged.fetch_add(1, Ordering::Relaxed);
                        return ignore::WalkState::Continue;
                    }

                    let indexed = if metadata.is_file() {
                        index_one_file(path, &metadata, modified, writer, fields)
                    } else if metadata.is_dir() {
                        index_one_directory(path, modified, writer, fields)
                    } else {
                        return ignore::WalkState::Continue;
                    };
                    match indexed {
                        Ok(()) => {
                            tally.indexed.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(error) => tracing::warn!("Failed to index {:?}: {}", path, error),
                    }
                    ignore::WalkState::Continue
                })
            });
        drop(seen_tx);

        for batch in seen_rx {
            for reached in batch {
                known.remove(&reached);
            }
        }

        let mut report = tally.into_report();
        // Whatever the walk never reached is a document with no file behind it.
        for gone in known.keys() {
            writer.delete_term(Term::from_field_text(fields.path, gone));
            report.removed += 1;
        }

        writer.commit()?;
        // Reported done only once the commit lands, so nothing reads "finished"
        // from an index that has not been written yet.
        if let Some(progress) = progress {
            progress.finish();
        }
        Ok(report)
    }

    /// The modification time the index holds for each path it knows.
    ///
    /// Read from the index's own columns rather than its document store. The
    /// store keeps documents compressed in blocks, so pulling one field out of
    /// every document means decompressing the whole index — on every launch,
    /// before any file has been looked at. The columns are memory-mapped and
    /// hold exactly these two values.
    ///
    /// The `Option` in the value is "the document carries no time", which is
    /// what an index written before `last_modified` was stored says for every
    /// document; those are re-read once and carry one afterwards.
    fn indexed_modifications(&self, fields: &Fields) -> Result<HashMap<String, Option<u64>>> {
        let searcher = self.index.reader()?.searcher();
        let mut known = HashMap::with_capacity(searcher.num_docs() as usize);

        for segment_reader in searcher.segment_readers() {
            let columns = segment_reader.fast_fields();
            let (Ok(Some(paths)), Ok(times)) = (columns.str("path"), columns.u64("last_modified"))
            else {
                // No columns to read: an index from a schema that predates
                // them. Fall back to the store so that the pass still knows
                // what the index holds — being slow is recoverable, and
                // treating every document as unknown would drop them all as
                // orphans.
                self.indexed_modifications_from_store(fields, segment_reader, &mut known)?;
                continue;
            };

            // The dictionary is read once, in its own order, rather than once
            // per document: resolving each document's term as the document is
            // reached seeks around a sorted table 20,000 times over, which
            // measured at 33µs a document — more than the whole walk. Streamed
            // in order it is a single sequential read.
            let dictionary = paths.dictionary();
            let mut by_ordinal: Vec<Option<String>> = Vec::with_capacity(dictionary.num_terms());
            let mut terms = dictionary.stream()?;
            while terms.advance() {
                by_ordinal.push(Some(String::from_utf8_lossy(terms.key()).into_owned()));
            }

            for doc_id in segment_reader.doc_ids_alive() {
                let Some(ordinal) = paths.term_ords(doc_id).next() else {
                    continue;
                };
                // Taken rather than cloned: every document holds a distinct
                // path, so nothing else will ask for this one.
                let Some(path) = by_ordinal.get_mut(ordinal as usize).and_then(Option::take) else {
                    continue;
                };
                known.insert(path, times.first(doc_id));
            }
        }
        Ok(known)
    }

    fn indexed_modifications_from_store(
        &self,
        fields: &Fields,
        segment_reader: &tantivy::SegmentReader,
        known: &mut HashMap<String, Option<u64>>,
    ) -> Result<()> {
        let store = segment_reader.get_store_reader(1)?;
        for doc_id in segment_reader.doc_ids_alive() {
            let document: TantivyDocument = store.get(doc_id)?;
            let Some(path) = document
                .get_first(fields.path)
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let modified = document
                .get_first(fields.last_modified)
                .and_then(|value| value.as_u64());
            known.insert(path.to_string(), modified);
        }
        Ok(())
    }
}

/// The walk every indexing pass makes, configured in one place so the counting
/// pass and the indexing pass cannot disagree about what is in the tree.
fn walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).git_ignore(true);
    builder
}

/// How many entries the tree holds, for the progress denominator.
///
/// Reads the walker's own file type instead of calling `stat`, which is the
/// difference between a pass over the tree and two of them.
fn count_entries(root: &Path) -> usize {
    let total = AtomicUsize::new(0);
    walker(root)
        .threads(indexing_threads())
        .build_parallel()
        .run(|| {
            let total = &total;
            Box::new(move |result| {
                if let Ok(entry) = result {
                    if entry.file_type().is_some_and(|kind| !kind.is_symlink()) {
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                }
                ignore::WalkState::Continue
            })
        });
    total.load(Ordering::Relaxed)
}

/// How many threads an indexing pass walks with.
///
/// Half the machine, capped, per the resource policy in `docs/search.md` §7.1:
/// indexing is background work and must leave the machine usable. The cap also
/// bounds how many files are held in memory at once.
fn indexing_threads() -> usize {
    std::thread::available_parallelism()
        .map(|count| (count.get() / 2).clamp(1, 4))
        .unwrap_or(1)
}

/// The counters an indexing pass keeps, shared across its walk threads.
#[derive(Debug, Default, Clone)]
struct Tally {
    walked: Arc<AtomicUsize>,
    indexed: Arc<AtomicUsize>,
    unchanged: Arc<AtomicUsize>,
}

impl Tally {
    fn into_report(self) -> IndexReport {
        IndexReport {
            indexed: self.indexed.load(Ordering::Relaxed),
            unchanged: self.unchanged.load(Ordering::Relaxed),
            removed: 0,
        }
    }
}

/// One walk thread's list of indexed paths it reached.
///
/// Batched rather than sent per entry, and flushed when the walk drops the
/// visitor that owns it, so a worker never contends on the collector and never
/// loses what it gathered.
struct SeenBatch {
    seen: Vec<String>,
    sink: std::sync::mpsc::Sender<Vec<String>>,
}

impl SeenBatch {
    fn new(sink: std::sync::mpsc::Sender<Vec<String>>) -> Self {
        Self {
            seen: Vec::new(),
            sink,
        }
    }

    fn reached(&mut self, path: &str) {
        self.seen.push(path.to_string());
    }
}

impl Drop for SeenBatch {
    fn drop(&mut self) {
        let seen = std::mem::take(&mut self.seen);
        if seen.is_empty() {
            return;
        }
        if let Err(error) = self.sink.send(seen) {
            // The collector is gone, which means the pass is being torn down.
            // Say so rather than swallow it: the orphan sweep that follows will
            // be reading an incomplete picture.
            tracing::warn!("indexing progress could not be reported back: {error}");
        }
    }
}

/// Publishes indexing progress on its own thread.
///
/// The walk threads only ever bump an atomic; this reads it on a timer. That
/// keeps the UI's update rate off the number of files (a tree of 200,000 would
/// otherwise push 200,000 updates) and keeps the channel out of the hot loop.
struct ProgressTicker {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<postage::watch::Sender<f32>>>,
}

impl ProgressTicker {
    const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

    fn start(
        mut sender: postage::watch::Sender<f32>,
        walked: Arc<AtomicUsize>,
        total: usize,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker = std::thread::spawn({
            let stop = stop.clone();
            move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Self::INTERVAL);
                    if total > 0 {
                        let done = walked.load(Ordering::Relaxed);
                        let ratio = (done as f32 / total as f32).clamp(0.0, 1.0);
                        *sender.borrow_mut() = ratio;
                    }
                }
                sender
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }

    /// Stops the ticker and reports the pass as finished.
    fn finish(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(mut sender) => *sender.borrow_mut() = 1.0,
                Err(_) => tracing::warn!("the indexing progress thread panicked"),
            }
        }
    }
}

impl Drop for ProgressTicker {
    fn drop(&mut self) {
        // `finish` takes the handle, so this only runs on an early return, and
        // then only to stop the thread rather than to claim the pass finished.
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn index_one_directory(
    path: &Path,
    modified: Option<u64>,
    writer: &IndexWriter,
    fields: &Fields,
) -> Result<()> {
    let path_str = path.to_string_lossy();
    let filename = path.file_name().unwrap_or_default().to_string_lossy();

    let mut doc = TantivyDocument::default();
    doc.add_text(fields.path, &path_str);
    doc.add_text(fields.filename, &filename);
    doc.add_text(fields.content, &filename); // Allow searching dir by name content
    doc.add_u64(fields.is_directory, 1);
    if let Some(modified) = modified {
        doc.add_u64(fields.last_modified, modified);
    }

    writer.delete_term(Term::from_field_text(fields.path, &path_str));
    writer.add_document(doc)?;
    Ok(())
}

fn index_one_file(
    path: &Path,
    metadata: &fs::Metadata,
    modified: Option<u64>,
    writer: &IndexWriter,
    fields: &Fields,
) -> Result<()> {
    if metadata.len() > MAX_INDEXED_FILE_BYTES {
        tracing::debug!("Skipping large file: {:?}", path);
        return Ok(());
    }

    // Indexing runs on the pass's own walk threads, never the GPUI foreground
    // loop, so the blocking read is fine here.
    #[allow(clippy::disallowed_methods)]
    let Ok(content) = fs::read_to_string(path) else {
        tracing::debug!("Skipping binary/unreadable file: {:?}", path);
        return Ok(());
    };
    // Crude, and the same check the rest of the search stack makes: a NUL byte
    // means this is not text anyone wants lines quoted from.
    if content.contains('\0') {
        tracing::debug!("Skipping binary file (detected null byte): {:?}", path);
        return Ok(());
    }

    let path_str = path.to_string_lossy();
    let filename = path.file_name().unwrap_or_default().to_string_lossy();

    let mut doc = TantivyDocument::default();
    doc.add_text(fields.path, &path_str);
    doc.add_text(fields.filename, &filename);
    // The path is searchable as content, as a second value of the field rather
    // than concatenated onto the front of it: joining them would copy the whole
    // file to prepend one line, which on a large tree is the read done twice.
    doc.add_text(fields.content, &path_str);
    doc.add_text(fields.content, &content);
    doc.add_u64(fields.is_directory, 0);
    if let Some(modified) = modified {
        doc.add_u64(fields.last_modified, modified);
    }

    // Delete the existing document for this path first, so a re-index replaces
    // rather than duplicates.
    writer.delete_term(Term::from_field_text(fields.path, &path_str));
    writer.add_document(doc)?;
    Ok(())
}

impl IndexManager {
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

    /// The tree this index is built from.
    pub fn content_root(&self) -> &Path {
        &self.content_root
    }

    /// Where this index is stored.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// How many documents the index holds as of the last commit.
    pub fn document_count(&self) -> Result<u64> {
        Ok(self.index.reader()?.searcher().num_docs())
    }

    /// Re-indexes or removes each of `paths` (depending on existence) and commits once.
    #[tracing::instrument(target = "nohrs::op", name = "index.process_changes", level = "debug", skip_all, fields(paths = paths.len()))]
    pub fn process_changes(&self, paths: &[PathBuf]) -> Result<()> {
        self.with_writer(|writer| self.process_changes_with(writer, paths))
    }

    fn process_changes_with(&self, writer: &mut IndexWriter, paths: &[PathBuf]) -> Result<()> {
        let fields = Fields::of(&self.index.schema())?;

        for path in paths {
            match fs::metadata(path) {
                Ok(metadata) if metadata.is_file() => {
                    let modified = modified_nanos(&metadata);
                    if let Err(e) = index_one_file(path, &metadata, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                // A directory the watcher reported is its own entry in the
                // index, and its children arrive as their own events.
                Ok(metadata) if metadata.is_dir() => {
                    let modified = modified_nanos(&metadata);
                    if let Err(e) = index_one_directory(path, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                Ok(_) => {}
                // Gone, or no longer reachable: either way the document for it
                // must not keep answering searches.
                Err(_) => {
                    let path_str = path.to_string_lossy();
                    writer.delete_term(Term::from_field_text(fields.path, &path_str));
                }
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
    // Held open for the life of the reader. Building one per query re-opens the
    // index's files and rebuilds its searcher pool, which a one-shot command
    // can afford and a launcher answering every keystroke inside 50ms
    // (`docs/launcher.md` §12) cannot.
    reader: tantivy::IndexReader,
    // Built once as well: parsing a query against a schema is cheap, but
    // constructing the parser walks every field's options.
    query_parser: tantivy::query::QueryParser,
    path_field: Field,
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
        let field = |name: &str| {
            schema.get_field(name).with_context(|| {
                format!(
                    "the index at {} was built by an older version of nohrs (no `{name}` field); rebuild it",
                    index_path.display()
                )
            })
        };
        let path_field = field("path")?;
        let filename_field = field("filename")?;
        let content_field = field("content")?;

        // `Manual` rather than tantivy's default: the default installs a
        // directory watcher per reader to notice commits, which is a thread and
        // an inotify/FSEvents registration in every process that reads. Readers
        // are told when to [`IndexReader::reload`] instead, by whoever wrote.
        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::Manual)
            .try_into()
            .with_context(|| format!("cannot read the index at {}", index_path.display()))?;
        let query_parser =
            tantivy::query::QueryParser::for_index(&index, vec![filename_field, content_field]);

        Ok(Some(Self {
            reader,
            query_parser,
            path_field,
            index_path,
            content_root,
        }))
    }

    /// Picks up what has been committed to the index since this reader opened.
    ///
    /// Cheap enough to call on every notice: it swaps in the new segments and
    /// leaves any in-flight search reading the old ones.
    pub fn reload(&self) -> Result<()> {
        self.reader.reload()?;
        Ok(())
    }

    /// Where the index itself is stored.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// The tree the index was built from.
    pub fn content_root(&self) -> &Path {
        &self.content_root
    }

    /// How many documents the index holds as of the last [`IndexReader::reload`].
    /// Zero means it exists but has not been filled in, which answers no query
    /// correctly.
    pub fn document_count(&self) -> u64 {
        self.reader.searcher().num_docs()
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
        let searcher = self.reader.searcher();
        let path_field = self.path_field;
        let parsed = self
            .query_parser
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
    fn a_second_pass_reads_only_what_changed() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");

        let first = manager.index_home(Refresh::Changed, None).unwrap();
        assert!(first.indexed > 0);
        assert_eq!(first.unchanged, 0);

        // Nothing touched: the pass should find nothing to write.
        let second = manager.index_home(Refresh::Changed, None).unwrap();
        assert_eq!(second.indexed, 0, "a warm index was written to anyway");
        assert_eq!(second.unchanged, first.indexed);

        // One file edited: only that file is re-read. The directory holding it
        // has its own modification time bumped, so it is written again too.
        std::fs::write(content.join("notes.txt"), "a different needle\n").unwrap();
        let third = manager.index_home(Refresh::Changed, None).unwrap();
        assert!(
            (1..=2).contains(&third.indexed),
            "an edit to one file rewrote {} documents",
            third.indexed
        );

        let reader = IndexReader::open(dir.path().join("index"), content.clone())
            .unwrap()
            .expect("an index that was just built");
        assert_eq!(
            reader.candidates("different", 10).unwrap(),
            vec![content.join("notes.txt")],
            "the new contents did not reach the index"
        );
    }

    #[test]
    fn full_rereads_what_an_incremental_pass_would_leave_alone() {
        let (_dir, manager) = staged();
        let first = manager.index_home(Refresh::Changed, None).unwrap();

        let full = manager.index_home(Refresh::Everything, None).unwrap();

        assert_eq!(full.indexed, first.indexed);
        assert_eq!(full.unchanged, 0);
    }

    #[test]
    fn a_file_that_is_gone_stops_answering_searches() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        manager.index_home(Refresh::Changed, None).unwrap();

        std::fs::remove_file(content.join("notes.txt")).unwrap();
        let report = manager.index_home(Refresh::Changed, None).unwrap();

        assert_eq!(report.removed, 1);
        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("needle", 10).unwrap().is_empty(),
            "a deleted file is still in the index"
        );
    }

    #[test]
    fn what_a_manager_builds_a_reader_can_read() {
        let (dir, manager) = staged();
        manager.index_home(Refresh::Changed, None).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), dir.path().join("content"))
            .unwrap()
            .expect("an index that was just built");

        assert!(reader.document_count() > 0);
        let candidates = reader.candidates("needle", 10).unwrap();
        assert_eq!(candidates, vec![dir.path().join("content/notes.txt")]);
    }

    #[test]
    fn a_reader_can_be_opened_while_a_writer_is_held() {
        let (dir, manager) = staged();
        manager.index_home(Refresh::Changed, None).unwrap();
        // `manager` still owns the writer here; opening for reading must not
        // wait on it, which is what lets `noh search` run beside the app.
        let reader = IndexReader::open(dir.path().join("index"), dir.path().join("content"))
            .unwrap()
            .expect("an index that was just built");
        assert!(reader.document_count() > 0);
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
