use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
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
        // The tree this pass is about has to be readable before any of it means
        // anything. A walk that cannot read its own root reports one error and
        // ends, which `Unseen` then reads — correctly, for a subdirectory — as
        // "do not call anything under here gone". Applied to the root that
        // protects the whole index, so a content root that has been renamed or
        // unmounted would leave every document answering searches forever while
        // the pass reported success. Failing says which it is.
        std::fs::read_dir(&self.content_root)
            .with_context(|| format!("cannot read {} to index it", self.content_root.display()))?;

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
        // Where the walk could not see. An unreadable directory is reported
        // once and not descended into, so without this every document beneath
        // it would look like a document whose file is gone.
        let unseen = Mutex::new(Unseen::default());
        walker(&self.content_root)
            .threads(indexing_threads())
            .build_parallel()
            .run(|| {
                // Per worker: the batch flushes itself when the walk drops this
                // visitor, so nothing is lost and nothing is sent per entry.
                let mut batch = SeenBatch::new(seen_tx.clone());
                let unseen = &unseen;
                let tally = &tally;
                let known = &known;
                let fields = &fields;
                let writer = &*writer;
                Box::new(move |result| {
                    let entry = match result {
                        Ok(entry) => entry,
                        Err(error) => {
                            tracing::warn!("Walk error: {}", error);
                            unseen
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .note(&error);
                            return ignore::WalkState::Continue;
                        }
                    };
                    // Counted before anything can skip the entry, so progress
                    // reaches 1.0 whatever the walk runs into.
                    tally.walked.fetch_add(1, Ordering::Relaxed);

                    let path = entry.path();
                    // A symlink is not indexed, because its target is a file
                    // the covered tree does not contain and a search scoped to
                    // that tree would then answer with it. Deliberately not
                    // reported as reached either: one that used to be a regular
                    // file is a document to drop, not one to keep.
                    if entry.depth() > 0 && entry.path_is_symlink() {
                        return ignore::WalkState::Continue;
                    }

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
        let unseen = unseen.into_inner().unwrap_or_else(PoisonError::into_inner);
        // Whatever the walk never reached is a document with no file behind it
        // — unless the walk could not look there, in which case nothing is
        // known about it either way. Keeping such a document costs one stale
        // answer until a pass does reach it; deleting it loses the document on
        // a directory that was merely busy or briefly unreadable, which no
        // later pass puts back short of a full rebuild.
        for gone in known.keys() {
            if unseen.covers(gone) {
                continue;
            }
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

    /// The paths the index holds strictly beneath `path`.
    ///
    /// Seeks rather than scans. The term dictionary is sorted, so the documents
    /// under one directory are a contiguous range in it, and a watcher batch
    /// must not cost a pass over everything the index holds: a deleted file is
    /// ordinary, and answering "nothing is beneath it" has to be cheap.
    fn indexed_beneath(&self, fields: &Fields, path: &str) -> Result<Vec<String>> {
        // Everything below `path` and nothing else: `foo/` excludes `foo` and
        // stops short of `foo!`, `foo.txt` and any other sibling sharing the
        // prefix without the separator.
        let prefix = format!("{path}/");
        let mut beneath = Vec::new();
        let searcher = self.index.reader()?.searcher();

        for segment_reader in searcher.segment_readers() {
            let Ok(Some(paths)) = segment_reader.fast_fields().str("path") else {
                // No column to seek in: an index from a schema that predates
                // it. Read the store instead — slow, and the alternative is
                // leaving documents behind that answer for a tree that is gone.
                let mut known = HashMap::new();
                self.indexed_modifications_from_store(fields, segment_reader, &mut known)?;
                beneath.extend(known.into_keys().filter(|held| held.starts_with(&prefix)));
                continue;
            };

            let mut terms = paths.dictionary().range().ge(&prefix).into_stream()?;
            while terms.advance() {
                let held = String::from_utf8_lossy(terms.key()).into_owned();
                // Sorted, so the first key past the prefix ends the range.
                if !held.starts_with(&prefix) {
                    break;
                }
                beneath.push(held);
            }
        }
        Ok(beneath)
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

/// Whether the directories above a changed path are ones a walk would descend
/// through, remembered for the length of one batch.
///
/// The batch is usually several changes in one directory — a build writing into
/// `target/`, an editor saving — so the same ancestors are asked about
/// repeatedly, and the answer cannot change under a debounce that has already
/// elapsed.
#[derive(Default)]
struct Ancestors {
    walkable: std::collections::HashSet<PathBuf>,
}

impl Ancestors {
    /// Whether `path` can be reached from `root` without passing through a
    /// symlink, which is the rule the walk applies to everything it meets.
    ///
    /// A path outside `root` is not walkable at all: the index covers one tree,
    /// and a document keyed inside it must have come from inside it.
    fn are_walkable(&mut self, root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let mut walked = root.to_path_buf();
        let mut components: Vec<_> = relative.components().collect();
        // The last component is the path itself, which the caller asks about
        // separately — it is allowed to be a link there, and is then handled as
        // a document that must go.
        components.pop();

        for component in components {
            walked.push(component);
            if self.walkable.contains(&walked) {
                continue;
            }
            // Unreadable counts as not walkable: nothing below a directory the
            // walk cannot enter is reachable, whatever the reason.
            let followable = fs::symlink_metadata(&walked)
                .map(|about| !about.is_symlink())
                .unwrap_or(false);
            if !followable {
                return false;
            }
            self.walkable.insert(walked.clone());
        }
        true
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
                if let Ok(entry) = result
                    && entry.file_type().is_some_and(|kind| !kind.is_symlink())
                {
                    total.fetch_add(1, Ordering::Relaxed);
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

/// The parts of the tree a pass could not look at.
///
/// A pass decides a document is stale by not reaching its file, so it has to
/// tell "the file is gone" apart from "the walk could not get there". `ignore`
/// reports an unreadable directory once and does not descend, so one `EACCES`
/// or one busy mount stands for everything beneath it.
#[derive(Default)]
struct Unseen {
    /// Paths the walk reported an error for, whose contents it never saw.
    ///
    /// Held the way the index spells a path — `to_string_lossy` — rather than
    /// as the `PathBuf` the walk reported, because that is what the documents
    /// being compared against are keyed by. A path with a non-UTF-8 component
    /// spells the same way on both sides only if both sides are lossy, and
    /// getting that wrong would silently un-protect exactly the documents this
    /// type exists to protect.
    roots: Vec<String>,
    /// Whether an error arrived naming no path at all. Nothing then says what
    /// went unwalked, so nothing can be called gone.
    anywhere: bool,
}

impl Unseen {
    fn note(&mut self, error: &ignore::Error) {
        match unwalked(error) {
            Some(path) => self.roots.push(path.to_string_lossy().into_owned()),
            None => self.anywhere = true,
        }
    }

    /// Whether `indexed_path` is somewhere this pass could not see.
    ///
    /// Takes the indexed spelling of the path, not a `Path`, so that the
    /// comparison is between two strings the same lossy conversion produced.
    fn covers(&self, indexed_path: &str) -> bool {
        self.anywhere
            || self
                .roots
                .iter()
                .any(|root| Path::new(indexed_path).starts_with(root))
    }
}

/// The path a walk error says was not walked, if it names one.
///
/// `ignore` wraps its errors — a depth around a path around the `io::Error` —
/// so the path is found by unwrapping rather than by matching one variant.
/// `None` is not "no path exists" but "this pass cannot say what it missed",
/// which is why it is treated as the whole tree rather than as nothing.
fn unwalked(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        // The ancestor, not the child: the loop is entered at the ancestor and
        // everything below it goes unwalked.
        ignore::Error::Loop { ancestor, .. } => Some(ancestor),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            unwalked(err)
        }
        // The first that names one: they are all errors from the same failure.
        ignore::Error::Partial(errors) => errors.iter().find_map(unwalked),
        _ => None,
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
        let mut ancestors = Ancestors::default();

        for path in paths {
            // Asked without following the link, because a symlink is not
            // indexed: its target is a file the covered tree does not contain.
            // A path that has become one is handled below as a path that is
            // gone, which is what it is as far as the index is concerned —
            // otherwise a live change would put back what a full pass refuses
            // to index. `symlink_metadata` spares only the final component and
            // resolves the rest, so the same has to be asked of the directories
            // above it: a file under one that has become a link is reported as
            // an ordinary file, at a path inside the tree.
            let about = fs::symlink_metadata(path)
                .ok()
                .filter(|about| !about.is_symlink())
                .filter(|_| ancestors.are_walkable(&self.content_root, path));
            match about {
                Some(metadata) if metadata.is_file() => {
                    let modified = modified_nanos(&metadata);
                    if let Err(e) = index_one_file(path, &metadata, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                // A directory the watcher reported is its own entry in the
                // index, and its children arrive as their own events.
                Some(metadata) if metadata.is_dir() => {
                    let modified = modified_nanos(&metadata);
                    if let Err(e) = index_one_directory(path, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                Some(_) => {}
                // Gone, unreachable, or now a symlink: for the index all three
                // are the same, a document that must stop answering searches.
                //
                // Its descendants go with it. A directory takes its children
                // when it goes and nothing touched them, so no event ever
                // arrives on their behalf; left alone they answer for files
                // that are not there, and where the directory was replaced by a
                // link, at paths that now resolve outside the tree.
                None => {
                    let path_str = path.to_string_lossy();
                    writer.delete_term(Term::from_field_text(fields.path, &path_str));
                    for beneath in self.indexed_beneath(&fields, &path_str)? {
                        writer.delete_term(Term::from_field_text(fields.path, &beneath));
                    }
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
    fn a_place_the_walk_could_not_look_is_not_a_place_where_files_are_gone() {
        let unseen = {
            let mut unseen = Unseen::default();
            unseen.note(&ignore::Error::WithPath {
                path: PathBuf::from("/home/someone/locked"),
                err: Box::new(ignore::Error::Io(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                ))),
            });
            unseen
        };

        assert!(unseen.covers("/home/someone/locked/notes.txt"));
        assert!(unseen.covers("/home/someone/locked"));
        // The rest of the tree was walked and is still judged on what it holds.
        assert!(!unseen.covers("/home/someone/elsewhere/notes.txt"));
        // Not a prefix match on the string: a sibling that merely starts with
        // the same letters was walked like any other.
        assert!(!unseen.covers("/home/someone/locked-out/notes.txt"));
    }

    /// The documents are keyed by the index's lossy spelling of their path, so
    /// the unwalked roots must be spelled the same way. Held as raw `PathBuf`s,
    /// a directory with a non-UTF-8 component would never match the documents
    /// beneath it — un-protecting exactly the ones this is for.
    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_is_still_recognised_as_unseen() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        // A lone 0xFF is not valid UTF-8, so `to_string_lossy` replaces it.
        let raw = PathBuf::from(OsStr::from_bytes(b"/home/someone/lock\xffed"));
        let mut unseen = Unseen::default();
        unseen.note(&ignore::Error::WithPath {
            path: raw.clone(),
            err: Box::new(ignore::Error::Io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied,
            ))),
        });

        let beneath = raw.join("notes.txt");
        assert!(
            unseen.covers(&beneath.to_string_lossy()),
            "a document under an unreadable non-UTF-8 directory was left unprotected"
        );
    }

    #[test]
    fn an_error_that_names_nowhere_stands_for_the_whole_tree() {
        let mut unseen = Unseen::default();
        unseen.note(&ignore::Error::Io(std::io::Error::other("a broken mount")));

        assert!(
            unseen.covers("/anywhere/at/all"),
            "an error saying nothing about where it happened let documents be dropped"
        );
    }

    #[test]
    fn the_path_is_found_however_the_walk_wrapped_it() {
        let wrapped = ignore::Error::WithDepth {
            depth: 3,
            err: Box::new(ignore::Error::WithPath {
                path: PathBuf::from("/home/someone/locked"),
                err: Box::new(ignore::Error::Io(std::io::Error::other("nope"))),
            }),
        };

        assert_eq!(unwalked(&wrapped), Some(Path::new("/home/someone/locked")));
    }

    /// A directory that cannot be read is reported once and not descended into,
    /// so everything the index holds beneath it goes unvisited. Treating that
    /// as "the files are gone" empties the index for a directory that was only
    /// briefly unreadable, and no later pass puts those documents back.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_does_not_empty_the_index_beneath_it() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let locked = content.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("deep.txt"), "a needle in the deep\n").unwrap();
        manager.index_home(Refresh::Changed, None).unwrap();

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads a directory whatever its mode, so there is nothing to
        // observe on a machine where this test cannot lock anything.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let report = manager.index_home(Refresh::Changed, None).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            report.removed, 0,
            "a directory that could not be read was read as a directory that is gone"
        );
        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("deep", 10).unwrap().is_empty(),
            "the documents under an unreadable directory were dropped"
        );
    }

    /// The index covers a tree, and a search scoped to that tree answers from
    /// it — so a symlink's target, which the tree does not contain, must not
    /// get in. A document that becomes a symlink is dropped rather than kept.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_not_indexed_and_stops_being_indexed() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "a needle out here\n").unwrap();
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let link = content.join("link.txt");
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), &link).unwrap();

        manager.index_home(Refresh::Everything, None).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content.clone())
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("needle", 10).unwrap().contains(&link),
            "a symlink's target was indexed as though it were in the tree"
        );
    }

    /// A live change must not put back what a full pass refuses to index: the
    /// watcher reports a path, and `metadata` follows a link, so a file
    /// replaced by a symlink came back as its target's contents.
    #[cfg(unix)]
    #[test]
    fn the_watcher_does_not_index_a_file_that_has_become_a_symlink() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "a needle out here\n").unwrap();
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let notes = content.join("notes.txt");
        manager.index_home(Refresh::Everything, None).unwrap();

        std::fs::remove_file(&notes).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), &notes).unwrap();
        manager.update_file(&notes).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("needle", 10).unwrap().is_empty(),
            "the watcher indexed a symlink's target as though it were in the tree"
        );
    }

    /// A directory takes its descendants with it when it goes, and the watcher
    /// reports only the directory: the children were not touched, so no event
    /// ever arrives for them. Deleting the one term leaves every document
    /// beneath it answering — and where the directory was replaced by a link,
    /// each of those paths now resolves through it to a file the covered tree
    /// does not contain.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_gone_takes_its_descendants_out_of_the_index() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(outside.path().join("project")).unwrap();
        std::fs::write(
            outside.path().join("project/notes.txt"),
            "a beacon out here\n",
        )
        .unwrap();

        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let project = content.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("notes.txt"), "a beacon in here\n").unwrap();
        manager.index_home(Refresh::Everything, None).unwrap();

        // Established first, so that its later absence is the fix working
        // rather than the document never having been there.
        let reader = IndexReader::open(dir.path().join("index"), content.clone())
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader
                .candidates("beacon", 10)
                .unwrap()
                .contains(&project.join("notes.txt")),
            "the descendant was never indexed, so its removal would prove nothing"
        );

        // The directory is replaced by a link to one outside the tree. Only the
        // directory is reported: nothing touched the children.
        std::fs::remove_dir_all(&project).unwrap();
        std::os::unix::fs::symlink(outside.path().join("project"), &project).unwrap();
        manager
            .process_changes(std::slice::from_ref(&project))
            .unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("beacon", 10).unwrap().is_empty(),
            "a document under a directory that is gone still answers, at a path \
             that now resolves outside the tree"
        );
    }

    /// The same rule for a descendant reported on its own. `symlink_metadata`
    /// does not follow the final component, but it does resolve the rest of the
    /// path — so a file under a directory that has become a link is reported as
    /// an ordinary file, and indexing it puts the link's target into the index
    /// under a path inside the tree.
    #[cfg(unix)]
    #[test]
    fn a_path_reached_through_a_symlinked_directory_is_not_indexed() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(outside.path().join("project")).unwrap();
        std::fs::write(
            outside.path().join("project/notes.txt"),
            "a beacon out here\n",
        )
        .unwrap();

        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let project = content.join("project");
        manager.index_home(Refresh::Everything, None).unwrap();
        std::os::unix::fs::symlink(outside.path().join("project"), &project).unwrap();

        manager
            .process_changes(&[project.join("notes.txt")])
            .unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("beacon", 10).unwrap().is_empty(),
            "a file reached through a symlinked directory was indexed as though \
             the tree contained it"
        );
    }

    /// A pass that cannot read its own root has not established that anything
    /// is gone — but `Unseen` would read the root's error the way it reads a
    /// subdirectory's, and hold every document as protected while reporting a
    /// clean pass. A renamed or unmounted content root is a failure, not a
    /// refresh that found nothing to do.
    #[test]
    fn a_pass_whose_content_root_is_gone_fails_rather_than_reporting_success() {
        let (dir, manager) = staged();
        manager.index_home(Refresh::Everything, None).unwrap();

        std::fs::remove_dir_all(dir.path().join("content")).unwrap();

        let error = manager
            .index_home(Refresh::Changed, None)
            .expect_err("a pass over a content root that is gone reported success");
        assert!(
            format!("{error:#}").contains("cannot read"),
            "the failure did not say the root could not be read: {error:#}"
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
