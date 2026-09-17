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

/// What the index already holds for one path, as far as deciding whether to
/// write it again goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Indexed {
    modified: Option<u64>,
    is_directory: bool,
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
    ///
    /// The searcher comes from the caller for the same reason: one batch can
    /// hold many gone paths — an `rm -rf` reports them together — and opening a
    /// reader apiece would map the segments once per path, which is the cost
    /// the seek above is here to avoid.
    fn indexed_beneath(
        &self,
        fields: &Fields,
        searcher: &tantivy::Searcher,
        path: &str,
    ) -> Result<Vec<String>> {
        // Everything below `path` and nothing else: `foo/` excludes `foo` and
        // stops short of `foo!`, `foo.txt` and any other sibling sharing the
        // prefix without the separator.
        //
        // The platform's separator rather than `/`, because these are compared
        // against paths the walk produced: on Windows those hold `\`, and a
        // prefix ending in `/` matches none of them — a directory that went
        // would take its own document and leave every document under it
        // answering.
        let prefix = format!("{path}{}", std::path::MAIN_SEPARATOR);
        let mut beneath = Vec::new();

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

    /// What the index holds for `path`: the modification time it was written
    /// with, and whether it was written as a directory.
    ///
    /// The kind belongs here beside the time because the two change
    /// independently. A path replaced by one of the other kind keeps its
    /// modification time whenever whatever replaced it preserved the time —
    /// `tar -x` and `rsync -a` both do — and a document that then went
    /// unwritten would answer for a directory that is now a file.
    ///
    /// `None` when the index holds no document for that path at all, which is
    /// not the same as a document whose modification time could not be read.
    fn indexed_as(
        &self,
        fields: &Fields,
        searcher: &tantivy::Searcher,
        path: &str,
    ) -> Result<Option<Indexed>> {
        let query = tantivy::query::TermQuery::new(
            Term::from_field_text(fields.path, path),
            tantivy::schema::IndexRecordOption::Basic,
        );
        let found = searcher.search(
            &query,
            &tantivy::collector::TopDocs::with_limit(1).order_by_score(),
        )?;
        let Some((_score, address)) = found.first() else {
            return Ok(None);
        };
        let document: TantivyDocument = searcher.doc(*address)?;
        Ok(Some(Indexed {
            modified: document
                .get_first(fields.last_modified)
                .and_then(|value| value.as_u64()),
            is_directory: document
                .get_first(fields.is_directory)
                .and_then(|value| value.as_u64())
                .is_some_and(|written| written != 0),
        }))
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
    fn are_walkable(&mut self, root: &Path, path: &Path) -> Reach {
        let Ok(relative) = path.strip_prefix(root) else {
            return Reach::Outside;
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
            // A directory that cannot be read is answered apart from one that
            // is a link. Neither is walked, but only the link says anything
            // about what is underneath: a document below it is keyed at a path
            // that now resolves outside the tree, so it must go, while one
            // below a directory this process merely cannot open is a document
            // about a file that is very likely still there.
            match fs::symlink_metadata(&walked) {
                Ok(about) if about.is_symlink() => return Reach::Outside,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Reach::Outside;
                }
                Err(_) => return Reach::Unknown,
            }
            self.walkable.insert(walked.clone());
        }
        Reach::Walkable
    }
}

/// What a look at a path the watcher reported established about it.
enum Reported {
    /// It is there, and this is what it is.
    Present(fs::Metadata),
    /// It is not there, or is no longer part of the tree the index covers.
    Gone,
    /// It could not be looked at, which settles nothing about whether it is
    /// there. The index keeps what it holds.
    Unreadable,
}

/// How a path the watcher reported stands in relation to the tree the index
/// covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Reachable from the content root without passing through a symlink.
    Walkable,
    /// Not part of the covered tree: outside the root, gone, or below a
    /// symlink, which a walk does not descend through.
    Outside,
    /// One of the directories above it could not be read, so whether it is part
    /// of the covered tree is not established either way.
    Unknown,
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

/// Reads `path` as text, refusing to follow it if it is a link.
///
/// The check that a path is not a link and the read of that path are separate
/// syscalls, and what sits at the name can change in between — so the read
/// carries the rule rather than trusting the check to still hold. Losing that
/// race then costs a skipped file instead of a document whose content came from
/// outside the covered tree.
///
/// This narrows the window rather than closing it. A *directory* above the file
/// can be replaced between the two as well, and `O_NOFOLLOW` says nothing about
/// the path's earlier components; shutting that case out needs the whole walk
/// to descend by `openat` from a held root, which is a larger change than this.
///
/// The size is asked of the open handle and the read is bounded, because the
/// caller's `metadata` describes whatever was at that name a syscall ago. A
/// file can grow past the limit, or be replaced by one that is already past it,
/// in between — a log being appended to is enough, and nothing adversarial is
/// needed. An unbounded read would then pull the whole of it into memory, on a
/// pass whose entire point is to stay out of the way.
///
/// Indexing runs on the pass's own walk threads, never the GPUI foreground
/// loop, so the blocking read is fine here.
/// What a file's contents amount to for the index.
enum Readable {
    /// The text to index beside the name.
    Text(String),
    /// A regular file whose contents are not going in because of what they are:
    /// too large to index. Its name still does, and the document keeps the
    /// file's modification time, because that decision holds for as long as the
    /// file does not change.
    NameOnly,
    /// A regular file whose contents could not be read at all. Its name goes in
    /// without a modification time, so that the next pass reads it again rather
    /// than taking the name-only document for one that is up to date — a
    /// permission or a lock that lasted a minute would otherwise leave the
    /// contents out of the index until the file was next written to.
    Unreadable,
    /// Not a regular file any more: a link took its place between the walk's
    /// check and this read. Nothing about it belongs in the index, the name
    /// included, because the index covers one tree and a link points anywhere.
    NotAFile,
}

#[allow(clippy::disallowed_methods)]
fn read_without_following(path: &Path) -> Readable {
    use std::io::Read;

    #[cfg(unix)]
    let opened = {
        use std::os::unix::fs::OpenOptionsExt;

        // Via rustix rather than a hand-written constant: `O_NOFOLLOW` is a
        // different value on Linux and macOS, and the wrong one silently opens
        // the link instead of refusing it.
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
    };
    // `File::open` follows a link, so where `O_NOFOLLOW` is not available the
    // refusal has to be made before the open. It is a check rather than a
    // refusal the kernel performs: a link put in place between the two is
    // followed, where `O_NOFOLLOW` cannot be raced at all.
    #[cfg(not(unix))]
    let opened = match fs::symlink_metadata(path) {
        Ok(about) if about.is_symlink() => return Readable::NotAFile,
        _ => fs::File::open(path),
    };

    let mut file = match opened {
        Ok(file) => file,
        // `O_NOFOLLOW` refusing a link is the case this has to tell apart from
        // an ordinary unreadable file: one gets no document at all, the other
        // keeps its name in the index.
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) => {
            return Readable::NotAFile;
        }
        Err(_) => return Readable::Unreadable,
    };

    let Ok(about) = file.metadata() else {
        return Readable::Unreadable;
    };
    if !about.is_file() {
        return Readable::NotAFile;
    }
    if about.len() > MAX_INDEXED_FILE_BYTES {
        return Readable::NameOnly;
    }

    // One byte past the limit, so that a file which grew after that check is
    // caught by the length of what came back rather than read to its end.
    let mut content = String::new();
    let read = match file
        .by_ref()
        .take(MAX_INDEXED_FILE_BYTES + 1)
        .read_to_string(&mut content)
    {
        Ok(read) => read,
        // Read to its end and found not to be text. That is a decision about
        // the file, and it holds until the file changes — as it does for one
        // too large — so the document keeps its modification time and no later
        // pass reads it again. Most trees are full of these.
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            return Readable::NameOnly;
        }
        // Anything else is a read that did not happen.
        Err(_) => return Readable::Unreadable,
    };
    if read as u64 > MAX_INDEXED_FILE_BYTES {
        return Readable::NameOnly;
    }
    Readable::Text(content)
}

fn index_one_file(
    path: &Path,
    metadata: &fs::Metadata,
    modified: Option<u64>,
    writer: &IndexWriter,
    fields: &Fields,
) -> Result<()> {
    // A file too large, unreadable, or not text carries its name into the index
    // and not its contents, which is what `docs/search.md` §3.5 promises for
    // one. Writing the document matters as much as leaving the contents out of
    // it: a file that was small and textual when it was last indexed already
    // has one, and returning here would leave that older document answering
    // searches with what the file used to hold.
    let path_str = path.to_string_lossy();

    // Whether what went in reflects the file as it is now, or stands in for a
    // read that did not happen. The document keeps its modification time only in
    // the first case: recording the time beside a name-only document written
    // because the file could not be read would make every later incremental
    // pass consider it up to date, and the contents would stay out of the index
    // until something wrote to the file.
    let mut stands_for_a_read_that_failed = false;
    let content = if metadata.len() > MAX_INDEXED_FILE_BYTES {
        tracing::debug!("Indexing the name only, the file is large: {:?}", path);
        None
    } else {
        match read_without_following(path) {
            // Crude, and the same check the rest of the search stack makes: a
            // NUL byte means this is not text anyone wants lines quoted from.
            Readable::Text(text) if text.contains('\0') => {
                tracing::debug!("Indexing the name only, the file is binary: {:?}", path);
                None
            }
            Readable::Text(text) => Some(text),
            Readable::NameOnly => {
                tracing::debug!("Indexing the name only, the file is large: {:?}", path);
                None
            }
            Readable::Unreadable => {
                tracing::debug!("Indexing the name only, the file was not read: {:?}", path);
                stands_for_a_read_that_failed = true;
                None
            }
            // A link took the file's place between the walk's check and this
            // read. The index holds no document for one, so the name does not
            // go in either — and whatever this path held before has to go,
            // which is what the watcher does for the same change.
            Readable::NotAFile => {
                tracing::debug!("Dropping the document, the path is now a link: {:?}", path);
                writer.delete_term(Term::from_field_text(fields.path, &path_str));
                return Ok(());
            }
        }
    };

    let filename = path.file_name().unwrap_or_default().to_string_lossy();

    let mut doc = TantivyDocument::default();
    doc.add_text(fields.path, &path_str);
    doc.add_text(fields.filename, &filename);
    // The path is searchable as content, as a second value of the field rather
    // than concatenated onto the front of it: joining them would copy the whole
    // file to prepend one line, which on a large tree is the read done twice.
    doc.add_text(fields.content, &path_str);
    if let Some(content) = &content {
        doc.add_text(fields.content, content);
    }
    doc.add_u64(fields.is_directory, 0);
    if let Some(modified) = modified
        && !stands_for_a_read_that_failed
    {
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
        // Opened on the first path that needs the index looked at, and shared by
        // the rest. Nothing this loop writes reaches a reader before the commit
        // at the end, so one view answers for the whole batch.
        let mut viewing: Option<tantivy::Searcher> = None;

        for path in paths {
            let about = if path == &self.content_root {
                // The root is the one link that is followed. The walk follows
                // it, so a tree reached through one is a tree this index
                // legitimately covers — and reading a change reported for the
                // root as "a link, therefore gone" would take the root's
                // document and, with it, every document beneath: all of them.
                match fs::metadata(path) {
                    // A root that is no longer a directory is not a document to
                    // write: indexing it as the file it has become would leave
                    // every document under the directory it used to be
                    // answering for a tree that is not there. Nor is it a
                    // deletion to carry out here, on one watcher event — what
                    // the index covers has changed, which is a question for a
                    // pass over the whole tree, and `index_home` refuses to
                    // sweep a root it cannot walk. So the index is left as it
                    // is until one runs.
                    Ok(about) if !about.is_dir() => {
                        tracing::warn!(
                            "the content root {} is not a directory, leaving the index alone",
                            path.display()
                        );
                        Reported::Unreadable
                    }
                    Ok(about) => Reported::Present(about),
                    // The root being unreachable is this process losing sight
                    // of the tree, not the tree ceasing to exist, which is the
                    // distinction `index_home` makes when it fails rather than
                    // reporting a clean sweep of an index it never validated.
                    // An unmount that lasts a second would otherwise cost every
                    // document under the root — all of them — and no later pass
                    // puts those back short of a full rebuild.
                    Err(error) => {
                        tracing::warn!(
                            "the content root {} cannot be read, leaving the index alone: {error}",
                            path.display()
                        );
                        continue;
                    }
                }
            } else {
                // Asked without following the link, because a symlink is not
                // indexed: its target is a file the covered tree does not
                // contain. A path that has become one is handled below as a
                // path that is gone, which is what it is as far as the index is
                // concerned — otherwise a live change would put back what a
                // full pass refuses to index. `symlink_metadata` spares only
                // the final component and resolves the rest, so the same has to
                // be asked of the directories above it: a file under one that
                // has become a link is reported as an ordinary file, at a path
                // inside the tree.
                //
                // Not being able to look is answered apart from finding nothing
                // there. A permission that changed on a directory above, or a
                // mount that went away for a moment, makes every path under it
                // unstattable — and taking that for "gone" would delete those
                // documents and every document beneath them, which no later
                // pass puts back short of a full rebuild. It is the same
                // distinction `index_home` makes when it refuses to report a
                // clean sweep of a tree it could not read.
                match fs::symlink_metadata(path) {
                    Ok(about) if about.is_symlink() => Reported::Gone,
                    Ok(about) => match ancestors.are_walkable(&self.content_root, path) {
                        Reach::Walkable => Reported::Present(about),
                        Reach::Outside => Reported::Gone,
                        Reach::Unknown => Reported::Unreadable,
                    },
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Reported::Gone,
                    Err(_) => Reported::Unreadable,
                }
            };
            match about {
                Reported::Unreadable => {
                    tracing::debug!(
                        "{} cannot be read, leaving what the index holds for it alone",
                        path.display()
                    );
                }
                Reported::Present(metadata) if metadata.is_file() => {
                    let modified = modified_nanos(&metadata);
                    // Asked before the file is opened, because opening it is
                    // itself a change as far as the watcher is concerned:
                    // `notify` reports a read as `Access(Open)` and the
                    // debouncer passes it on like any other event. Indexing
                    // whatever is reported without first asking whether it
                    // changed therefore feeds the watcher its own work, and the
                    // daemon re-indexes the tree every debounce interval for as
                    // long as it runs. Everything this arm does short of
                    // `index_one_file` leaves the file unopened, so asking ends
                    // the loop rather than slowing it down.
                    //
                    // A file with no readable modification time is re-read, as
                    // it is by a pass: nothing says it has not changed.
                    if modified.is_some() {
                        if viewing.is_none() {
                            viewing = Some(self.index.reader()?.searcher());
                        }
                        let held = match viewing.as_ref() {
                            Some(searcher) => {
                                let path = path.to_string_lossy();
                                self.indexed_as(&fields, searcher, &path)?
                            }
                            None => None,
                        };
                        let already = Indexed {
                            modified,
                            is_directory: false,
                        };
                        if held == Some(already) {
                            continue;
                        }
                    }
                    if let Err(e) = index_one_file(path, &metadata, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                // A directory the watcher reported is its own entry in the
                // index, and its children arrive as their own events.
                Reported::Present(metadata) if metadata.is_dir() => {
                    let modified = modified_nanos(&metadata);
                    if let Err(e) = index_one_directory(path, modified, writer, &fields) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    }
                }
                Reported::Present(_) => {}
                // Gone, outside the tree, or now a symlink: for the index all
                // three are the same, a document that must stop answering
                // searches.
                //
                // Its descendants go with it. A directory takes its children
                // when it goes and nothing touched them, so no event ever
                // arrives on their behalf; left alone they answer for files
                // that are not there, and where the directory was replaced by a
                // link, at paths that now resolve outside the tree.
                Reported::Gone => {
                    let path_str = path.to_string_lossy();
                    writer.delete_term(Term::from_field_text(fields.path, &path_str));
                    if viewing.is_none() {
                        viewing = Some(self.index.reader()?.searcher());
                    }
                    if let Some(searcher) = viewing.as_ref() {
                        for beneath in self.indexed_beneath(&fields, searcher, &path_str)? {
                            writer.delete_term(Term::from_field_text(fields.path, &beneath));
                        }
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

    /// A change reported for a path the process cannot stat says nothing about
    /// whether the file is there: a permission that changed on a directory
    /// above it, or a mount that went away for a moment, makes every path under
    /// it unstattable. Read as "gone" it costs that document and every document
    /// beneath it, which no later incremental pass puts back — the walk does
    /// not reach them either, so nothing short of a full rebuild restores them.
    #[cfg(unix)]
    #[test]
    fn a_change_under_a_directory_that_cannot_be_read_keeps_what_the_index_holds() {
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
        if std::fs::metadata(locked.join("deep.txt")).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        // Exactly what the watcher reports for a file whose directory has just
        // had its permissions changed.
        let outcome = manager.process_changes(&[locked.join("deep.txt")]);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        outcome.unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("deep", 10).unwrap().is_empty(),
            "a file that could not be stat'd was taken for a file that is gone"
        );
    }

    /// A content root that has become a regular file is not a document to
    /// write. Indexing it as the file it now is would leave every document
    /// under the directory it used to be answering for a tree that is not
    /// there, at paths that no longer resolve.
    #[test]
    fn a_content_root_that_became_a_file_is_not_indexed_as_one() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        manager.index_home(Refresh::Everything, None).unwrap();

        std::fs::remove_dir_all(&content).unwrap();
        std::fs::write(&content, "a beacon in here\n").unwrap();
        manager
            .process_changes(std::slice::from_ref(&content))
            .unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("beacon", 10).unwrap().is_empty(),
            "a content root that became a file was indexed as a file"
        );
    }

    /// A binary file is read to its end and found not to be text, which is a
    /// decision about the file rather than a read that failed: it holds until
    /// the file changes. Treating it as unreadable would leave the document
    /// without a modification time, and every later incremental pass would read
    /// the whole file again to reach the same conclusion — on most trees, most
    /// of the files.
    #[test]
    fn a_binary_file_is_not_read_again_by_every_pass() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        // Invalid UTF-8, so the read fails rather than the NUL check catching
        // it after a successful decode.
        std::fs::write(content.join("image.bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();

        manager.index_home(Refresh::Changed, None).unwrap();
        let second = manager.index_home(Refresh::Changed, None).unwrap();

        assert_eq!(
            second.indexed, 0,
            "a file that is not text was read again by the next pass: {second:?}"
        );
        assert!(
            second.unchanged > 0,
            "nothing was compared at all: {second:?}"
        );
    }

    /// A file that could not be read still gets a document, so that searches by
    /// name find it — but that document stands in for a read that did not
    /// happen, and recording the file's modification time beside it would make
    /// every later incremental pass consider it up to date. A permission that
    /// lasted a minute would leave the contents out of the index until
    /// something wrote to the file.
    #[cfg(unix)]
    #[test]
    fn a_file_that_could_not_be_read_is_read_again_by_the_next_pass() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let locked = content.join("locked.txt");
        std::fs::write(&locked, "a beacon in here\n").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads a file whatever its mode, so there is nothing to observe
        // on a machine where this test cannot lock anything.
        if std::fs::read_to_string(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        manager.index_home(Refresh::Changed, None).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Nothing has been written to the file, so only a pass that does not
        // take the name-only document for an up-to-date one reads it now.
        let report = manager.index_home(Refresh::Changed, None).unwrap();
        assert!(
            report.indexed > 0,
            "a file that could not be read last time was not read again: {report:?}"
        );

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("beacon", 10).unwrap().is_empty(),
            "the contents of a file that became readable never reached the index"
        );
    }

    /// A pass skips a reported path whose modification time the index already
    /// holds, so that indexing does not re-read what its own last read woke the
    /// watcher over. A path that changed kind keeps its modification time
    /// whenever whatever replaced it preserved the time, so the time alone
    /// cannot decide: the document would go on saying "directory" for something
    /// that is now a file with contents to search.
    #[test]
    fn a_directory_replaced_by_a_file_of_the_same_age_is_still_read() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let notes = content.join("notes.txt");

        let was = std::fs::metadata(&notes).unwrap().modified().unwrap();
        std::fs::remove_file(&notes).unwrap();
        std::fs::create_dir(&notes).unwrap();
        std::fs::File::open(&notes)
            .unwrap()
            .set_modified(was)
            .unwrap();
        manager.index_home(Refresh::Everything, None).unwrap();

        // Back to a file, at the age the directory was indexed with.
        std::fs::remove_dir(&notes).unwrap();
        std::fs::write(&notes, "a needle in here\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&notes)
            .unwrap()
            .set_modified(was)
            .unwrap();
        manager.update_file(&notes).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("needle", 10).unwrap().is_empty(),
            "a directory that became a file of the same age was left in the index as a directory"
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

    /// A file that stops being indexable — grown past the limit, replaced by
    /// something binary — still has the document it got when it was neither.
    /// Returning without writing would leave that document answering with the
    /// contents the file no longer has.
    #[test]
    fn a_file_that_stops_being_indexable_stops_answering_with_what_it_held() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let notes = content.join("notes.txt");
        manager.index_home(Refresh::Everything, None).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content.clone())
            .unwrap()
            .expect("an index that was just built");
        assert!(
            reader.candidates("needle", 10).unwrap().contains(&notes),
            "the file was never indexed by content, so losing it proves nothing"
        );

        // The same name, now holding something that carries no text into the
        // index. The name is still indexed; the old contents must not be.
        std::fs::write(&notes, b"\0\0\0 binary now\n").unwrap();
        manager.index_home(Refresh::Everything, None).unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("needle", 10).unwrap().contains(&notes),
            "a file that is no longer text still answers with the text it used to hold"
        );
        assert!(
            reader.candidates("notes", 10).unwrap().contains(&notes),
            "the name was dropped along with the contents"
        );
    }

    /// The size the caller checked belongs to whatever was at that name a
    /// syscall ago. The read is bounded on its own account, so a file that grew
    /// past the limit in between is not pulled into memory whole.
    #[test]
    fn a_file_that_grew_past_the_limit_is_not_read_to_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.txt");
        let oversized = usize::try_from(MAX_INDEXED_FILE_BYTES).unwrap() + 1024;
        std::fs::write(&big, vec![b'a'; oversized]).unwrap();

        assert!(
            matches!(read_without_following(&big), Readable::NameOnly),
            "a file past the limit was read anyway"
        );

        let small = dir.path().join("small.txt");
        std::fs::write(&small, b"a beacon in here\n").unwrap();
        assert!(
            matches!(read_without_following(&small), Readable::Text(_)),
            "an ordinary file was refused, so refusing a large one proves nothing"
        );
    }

    /// The check that a path is not a link and the read of it are two syscalls,
    /// and what sits at the name can change in between. The read carries the
    /// rule itself, so losing that race costs a skipped file rather than a
    /// document holding content from outside the covered tree.
    #[cfg(unix)]
    #[test]
    fn a_file_that_became_a_link_is_not_read_through() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "a beacon out here\n").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let ordinary = dir.path().join("notes.txt");
        std::fs::write(&ordinary, "a beacon in here\n").unwrap();
        assert!(
            matches!(read_without_following(&ordinary), Readable::Text(_)),
            "an ordinary file was refused, so refusing a link proves nothing"
        );

        let swapped = dir.path().join("swapped.txt");
        std::os::unix::fs::symlink(&secret, &swapped).unwrap();
        assert!(
            matches!(read_without_following(&swapped), Readable::NotAFile),
            "a link was read through as though it were the file that was checked"
        );
    }

    /// Losing the check/read race to a link is not the same as a file whose
    /// contents cannot go in: the index holds no document for a link, so it
    /// must not gain one by name either. The distinction matters because the
    /// name-only document exists for the other cases.
    #[cfg(unix)]
    #[test]
    fn a_file_that_became_a_link_is_dropped_rather_than_indexed_by_name() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "a beacon out here\n").unwrap();

        let (dir, manager) = staged();
        let content = dir.path().join("content");
        let notes = content.join("notes.txt");
        manager.index_home(Refresh::Everything, None).unwrap();

        // The metadata the walk would have handed on, taken while it is still
        // an ordinary file — then the file is replaced before the read.
        let about = std::fs::symlink_metadata(&notes).unwrap();
        std::fs::remove_file(&notes).unwrap();
        std::os::unix::fs::symlink(&secret, &notes).unwrap();

        manager
            .with_writer(|writer| {
                let fields = Fields::of(&manager.index.schema())?;
                index_one_file(&notes, &about, None, writer, &fields)?;
                writer.commit()?;
                Ok(())
            })
            .unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("notes", 10).unwrap().contains(&notes),
            "a path that became a link kept a document, by name"
        );
    }

    /// `index_home` fails rather than sweeping when it cannot read its own
    /// content root, because "the pass could not look" is not "the files are
    /// gone". A change reported for a root that has been unmounted or removed
    /// has to mean the same thing — otherwise the live path deletes the root's
    /// document and every document beneath it, which is all of them, for an
    /// outage that may last a second.
    #[test]
    fn a_content_root_that_cannot_be_read_does_not_empty_the_index() {
        let (dir, manager) = staged();
        let content = dir.path().join("content");
        manager.index_home(Refresh::Everything, None).unwrap();

        // The volume goes away, and the watcher reports the root.
        std::fs::remove_dir_all(&content).unwrap();
        manager
            .process_changes(std::slice::from_ref(&content))
            .unwrap();

        // And comes back.
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("notes.txt"), "a needle in here\n").unwrap();

        let reader = IndexReader::open(dir.path().join("index"), content)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("needle", 10).unwrap().is_empty(),
            "an outage of the content root emptied the index"
        );
    }

    /// The content root is the one link that is followed: the walk follows it,
    /// so an index over a root that is a symlink is an ordinary index. Reading
    /// a change reported for the root as "this is a link, so it is gone" would
    /// delete the root's own document and — since a directory that goes takes
    /// its descendants — every document under it, which is all of them.
    #[cfg(unix)]
    #[test]
    fn a_content_root_that_is_a_link_is_not_read_as_a_tree_that_is_gone() {
        let real = tempfile::tempdir().unwrap();
        std::fs::write(real.path().join("notes.txt"), "a beacon in here\n").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("content");
        std::os::unix::fs::symlink(real.path(), &root).unwrap();
        let manager = IndexManager::new_with_path(dir.path().join("index"), root.clone()).unwrap();
        manager.index_home(Refresh::Everything, None).unwrap();

        // Established first: the walk does follow the root link, so there is
        // something here for the next step to be able to destroy.
        let reader = IndexReader::open(dir.path().join("index"), root.clone())
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("beacon", 10).unwrap().is_empty(),
            "the root link was never followed, so this test proves nothing"
        );

        // The watcher reports the root itself — a permission change, a touch.
        manager
            .process_changes(std::slice::from_ref(&root))
            .unwrap();

        let reader = IndexReader::open(dir.path().join("index"), root)
            .unwrap()
            .expect("an index that was just built");
        assert!(
            !reader.candidates("beacon", 10).unwrap().is_empty(),
            "a change reported for a symlinked content root emptied the index"
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
