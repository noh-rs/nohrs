//! On-demand search under a caller-chosen root: walk the tree and match either
//! the text inside each file or the names along the way.
//!
//! This is the V1 backend of [`docs/search.md`](../../../../docs/search.md) §2,
//! factored out of [`super::ripgrep`] so a caller that knows what it wants to
//! search — the explorer's current directory, `noh search`'s operands — can say
//! so, instead of being handed the one whole-filesystem scan the root scope
//! needs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use grep::matcher::Matcher;
use grep::regex::{RegexMatcher, RegexMatcherBuilder};
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;

use super::SearchResult;
use super::indexer::IndexReader;

/// What the query is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subject {
    /// Both: the name of every entry walked, and the text inside every file.
    /// A file can match twice, once for each, and does.
    #[default]
    Both,
    /// Only the text inside each file. Directories never match.
    Contents,
    /// Only the name of each file and directory, not its contents.
    Names,
}

impl Subject {
    /// Whether entry names are matched.
    pub fn includes_names(self) -> bool {
        matches!(self, Self::Both | Self::Names)
    }

    /// Whether file contents are matched.
    pub fn includes_contents(self) -> bool {
        matches!(self, Self::Both | Self::Contents)
    }
}

/// Which engine answers a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    /// Let the index answer when it can, and read the files when it cannot.
    #[default]
    Auto,
    /// Insist on the index, failing with the reason when it cannot answer.
    Index,
    /// Read the files, whatever the index may know.
    Walk,
}

/// Why a search did not use the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoIndex {
    /// Nothing has built an index yet.
    NotBuilt,
    /// The index exists but holds no documents.
    Empty,
    /// The search root is not inside the tree the index covers.
    OutOfScope {
        /// The tree the index does cover.
        covers: PathBuf,
    },
    /// The query is a pattern; the index answers plain-text queries.
    PatternQuery,
    /// An option was given that only a walk can honour.
    WalkOnlyOptions,
    /// The index is there but cannot be read.
    Unusable(String),
}

impl std::fmt::Display for NoIndex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBuilt => write!(formatter, "no index has been built yet"),
            Self::Empty => write!(formatter, "the index is empty"),
            Self::OutOfScope { covers } => {
                write!(formatter, "the index only covers {}", covers.display())
            }
            Self::PatternQuery => write!(
                formatter,
                "the query is a pattern, and the index answers plain-text queries"
            ),
            Self::WalkOnlyOptions => write!(
                formatter,
                "--max-depth, --hidden and --no-ignore bound a walk, and the index \
                 cannot re-apply the exclusions it was built with"
            ),
            Self::Unusable(reason) => write!(formatter, "{reason}"),
        }
    }
}

/// Which engine answered a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// The index chose the files, and their lines were matched in them.
    Index,
    /// The tree was walked. The reason is present when the index was welcome
    /// to answer and could not.
    Walk(Option<NoIndex>),
}

impl Default for Answered {
    fn default() -> Self {
        Self::Walk(None)
    }
}

/// How far a search reaches and how its query is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Which engine should answer.
    pub engine: Engine,
    /// Whether to match file contents or the names of the entries walked.
    pub subject: Subject,
    /// How many directories below the root to descend; `None` for no limit.
    pub max_depth: Option<usize>,
    /// Whether dot-files and dot-directories are searched.
    pub include_hidden: bool,
    /// Whether `.gitignore` / `.ignore` files prune the walk.
    pub respect_ignore_files: bool,
    /// Whether the query matches regardless of case.
    pub case_insensitive: bool,
    /// Whether the query is a literal string rather than a regular expression.
    pub literal: bool,
    /// Stop once this many matches have been collected; `None` for no limit.
    pub limit: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            engine: Engine::default(),
            subject: Subject::default(),
            max_depth: None,
            include_hidden: false,
            respect_ignore_files: true,
            case_insensitive: false,
            literal: false,
            limit: None,
        }
    }
}

/// What one [`search`] found.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The matches, in the order the walk reached them.
    pub results: Vec<SearchResult>,
    /// Whether [`Options::limit`] ended the search before the tree was
    /// exhausted. It says the search stopped early, not that more matches
    /// certainly exist: a limit reached by the very last match also sets it.
    pub truncated: bool,
    /// Which engine produced the results, and why it was not the index.
    pub answered_by: Answered,
}

/// Searches `root` for `query` with whichever engine [`Options::engine`] allows.
///
/// Fails when `query` is not a pattern the matcher can build, when
/// [`Engine::Index`] was insisted on and the index cannot answer, and when
/// `root` itself cannot be searched for what was asked — the caller named that
/// path, and "no matches" is the wrong answer for something never searched. A
/// missing root fails whatever the subject; one that merely cannot be opened
/// fails only a search that needed to read it (see [`must_be_searchable`]).
///
/// Inside the tree the opposite holds: a directory that cannot be read, or a
/// file that cannot be searched, is reported through `tracing` and skipped. One
/// unreadable directory in a large tree is normal and must not throw away the
/// rest of the results.
#[tracing::instrument(
    target = "nohrs::op",
    name = "search.scan",
    level = "debug",
    skip_all,
    fields(root = %root.display(), subject = ?options.subject, engine = ?options.engine)
)]
pub fn search(root: &Path, query: &str, options: &Options) -> Result<Outcome> {
    search_using(root, query, options, None)
}

/// Searches `root` with an index the caller already has open.
///
/// Opening a reader maps the index's files and builds a query parser, which a
/// one-shot command can afford once and a window answering keystrokes cannot
/// afford at all: it keeps one open and lends it here.
pub fn search_using(
    root: &Path,
    query: &str,
    options: &Options,
    open_index: Option<&IndexReader>,
) -> Result<Outcome> {
    let matcher = build_matcher(query, options)?;
    must_be_searchable(root, options)?;

    if options.limit == Some(0) {
        return Ok(Outcome::default());
    }

    match options.engine {
        Engine::Walk => walk(root, &matcher, options, Answered::Walk(None)),
        Engine::Index => match usable_index(root, query, options, open_index) {
            Ok(index) => from_index(&index, query, &matcher, options),
            Err(reason) => Err(anyhow::anyhow!(
                "the index cannot answer this search: {reason}"
            )),
        },
        Engine::Auto => match usable_index(root, query, options, open_index) {
            Ok(index) => from_index(&index, query, &matcher, options),
            // The index not being able to answer is not a failure: the files
            // themselves are still there to be read, which is slower and right.
            Err(reason) => walk(root, &matcher, options, Answered::Walk(Some(reason))),
        },
    }
}

/// Settles whether the operand can be searched at all, before an engine is
/// chosen and before the zero-limit shortcut.
///
/// Existing is not the same as being readable, and neither the walk nor the
/// index tells the caller apart from an empty tree on its own:
///
/// * `ignore` yields a directory it cannot descend into as a perfectly good
///   entry *first*, and only then the error — which is indistinguishable from
///   the ordinary unreadable directory further down that a walk must skip.
/// * A file that cannot be opened has its error swallowed by the searcher,
///   which is right for one file among thousands and wrong for the one the
///   caller named.
/// * The index answers from documents, so a root it still has but the disk does
///   not comes back as no matches.
///
/// All three would report "no matches" for something that is not searchable,
/// and `docs/cli.md` §4.1 promises `1` for an operand that could not be
/// searched. One `open` per operand settles it.
///
/// Anything that is neither a directory nor a regular file is refused rather
/// than opened. Opening a FIFO blocks until someone writes to it, and a
/// character device can be read forever, so the `open` that is meant to settle
/// this question in a syscall would instead be where the command stops. A
/// symlink is not special here: `metadata` follows it, so an operand naming a
/// link to a file is a file.
///
/// What counts as searchable depends on what is being searched for, so this
/// asks of the operand only what the search it was given actually needs:
///
/// * A name is knowable without reading the file, so a `--name` search over a
///   file nobody may open is a search that can be answered — refusing it would
///   report a failure for a question that has a perfectly good answer.
/// * A directory's entries come from listing it, so it is listed — except under
///   `--max-depth 0`, which stops above them. Nothing below the root is
///   reached, the root is not itself a candidate, and the answer is no matches
///   whether or not it could have been listed.
fn must_be_searchable(root: &Path, options: &Options) -> Result<()> {
    let about = std::fs::metadata(root)?;
    if about.is_dir() {
        if options.max_depth != Some(0) {
            std::fs::read_dir(root)?;
        }
    } else if about.is_file() {
        if options.subject.includes_contents() {
            std::fs::File::open(root)?;
        }
    } else {
        anyhow::bail!("not a regular file or a directory");
    }
    Ok(())
}

/// Walks the tree below `root`, matching every entry it reaches.
///
/// Fails when the operand itself cannot be walked, and only then. A missing or
/// unreadable *operand* is the caller having asked for something that is not
/// there, which `noh search` reports and exits `1` for; an unreadable directory
/// found part-way down is ordinary (permissions, races) and must not turn the
/// rest of the answer into a failure. Without the difference, `noh search
/// needle ./typo` would print nothing and exit `0` — "no matches", which is a
/// lie about a path that does not exist.
fn walk(
    root: &Path,
    matcher: &RegexMatcher,
    options: &Options,
    answered_by: Answered,
) -> Result<Outcome> {
    let mut outcome = Outcome {
        answered_by,
        ..Outcome::default()
    };
    let respect_ignore_files = options.respect_ignore_files;
    let walker = WalkBuilder::new(root)
        .max_depth(options.max_depth)
        .hidden(!options.include_hidden)
        .ignore(respect_ignore_files)
        .git_ignore(respect_ignore_files)
        .git_global(respect_ignore_files)
        .git_exclude(respect_ignore_files)
        .parents(respect_ignore_files)
        .build();

    let mut searcher = line_searcher();

    let mut reached_anything = false;
    for entry in walker {
        let entry = match entry {
            Ok(entry) => {
                reached_anything = true;
                entry
            }
            // The operand is the first thing the walk yields, so an error
            // before anything has been reached is the operand itself failing —
            // whatever the `metadata` call above was happy with. `depth` does
            // not tell these apart: it is `None` for the root's own error and
            // for a malformed ignore file alike. This message keeps `ignore`'s
            // wording, repeated path and all, because there is nothing to
            // unwrap it with: `ignore::Error` implements no `source`.
            Err(error) if !reached_anything => {
                return Err(anyhow::anyhow!("{error}"));
            }
            // An unreadable directory further down is normal (permissions,
            // races) and must not abort the walk; record it and move on.
            Err(error) => {
                tracing::debug!("skipping unreadable entry: {error}");
                continue;
            }
        };
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        // The root directory is not itself a candidate. A root that names a
        // single file is: searching one file is a legitimate scope.
        if entry.depth() == 0 && is_dir {
            continue;
        }
        // A symlink met along the way is not searched, because reading it would
        // read a file the scope does not contain: the operand names a tree, and
        // a link inside it can point anywhere. The walk already declines to
        // descend through one; this is the same rule for the file it names, and
        // it is what `grep -r` and ripgrep do. A symlink named as the operand
        // itself is still searched — that is the caller pointing at a file, not
        // the walk wandering out of its tree.
        if entry.depth() > 0 && entry.path_is_symlink() {
            tracing::debug!("not following {}", entry.path().display());
            continue;
        }

        collect_from(
            entry.path(),
            is_dir,
            matcher,
            &mut searcher,
            options,
            &mut outcome,
        );
        if reached_limit(options, &mut outcome) {
            break;
        }
    }

    Ok(outcome)
}

/// Matches `path` — its name, the lines inside it, or both — into `outcome`.
fn collect_from(
    path: &Path,
    is_dir: bool,
    matcher: &RegexMatcher,
    searcher: &mut Searcher,
    options: &Options,
    outcome: &mut Outcome,
) {
    if options.subject.includes_names() {
        // A name that is not UTF-8 cannot be matched against the query, and is
        // skipped rather than matched by accident.
        let name = path.file_name().and_then(|name| name.to_str());
        if name.is_some_and(|name| matcher.is_match(name.as_bytes()).unwrap_or(false)) {
            outcome.results.push(SearchResult {
                path: path.to_path_buf(),
                line_number: 0,
                line_content: String::new(),
            });
        }
    }

    let room = remaining(options.limit, outcome.results.len());
    if options.subject.includes_contents() && room != Some(0) && !is_dir {
        let sink = Collector {
            path,
            results: &mut outcome.results,
            room,
        };
        if let Err(error) = searcher.search_path(matcher, path, sink) {
            // Unreadable or undecodable files are expected in a walk of
            // someone's filesystem; ripgrep skips them too.
            tracing::debug!("cannot search {}: {error}", path.display());
        }
    }
}

/// Whether the search has collected all the caller asked for, marking the
/// outcome truncated when it has.
fn reached_limit(options: &Options, outcome: &mut Outcome) -> bool {
    match options.limit {
        Some(limit) if outcome.results.len() >= limit => {
            outcome.truncated = true;
            true
        }
        _ => false,
    }
}

fn line_searcher() -> Searcher {
    SearcherBuilder::new()
        // A match in a binary file is a line the caller cannot show — it would
        // put control bytes on a terminal — so stop at the first NUL byte, the
        // way ripgrep does by default.
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build()
}

/// How many candidate files the index is asked for.
///
/// The index ranks by relevance, so a cap keeps the pool to the documents worth
/// opening; the caller's own limit counts matches, of which one file can hold
/// many, and so cannot be used here.
const INDEX_CANDIDATE_POOL: usize = 10_000;

/// The index, and the search root resolved against the paths it stores.
struct UsableIndex<'a> {
    reader: Borrowed<'a>,
    /// The root as the filesystem canonically names it, which is how indexed
    /// paths are spelled.
    canonical_root: PathBuf,
    /// The root as the caller wrote it, which is how results are reported: a
    /// search of `.` should not start answering in absolute paths because the
    /// index happened to take the query.
    given_root: PathBuf,
}

/// Either the caller's open index or one opened here, so that the rest of the
/// search does not care which it got.
enum Borrowed<'a> {
    Lent(&'a IndexReader),
    Opened(IndexReader),
}

impl std::ops::Deref for Borrowed<'_> {
    type Target = IndexReader;

    fn deref(&self) -> &IndexReader {
        match self {
            Self::Lent(reader) => reader,
            Self::Opened(reader) => reader,
        }
    }
}

/// Whether the index can answer this search, and if not, why not.
fn usable_index<'a>(
    root: &Path,
    query: &str,
    options: &Options,
    open_index: Option<&'a IndexReader>,
) -> std::result::Result<UsableIndex<'a>, NoIndex> {
    // The index answers term queries; a regular expression means something else
    // to it entirely, and `-F` literals may still carry characters its query
    // parser reads as syntax. Both belong to the files themselves.
    if !is_plain_text(query) {
        return Err(NoIndex::PatternQuery);
    }
    if options.max_depth.is_some() || options.include_hidden || !options.respect_ignore_files {
        // These bound a walk. The index was built with its own exclusions and
        // cannot re-apply them, so honouring the flags means reading the files.
        return Err(NoIndex::WalkOnlyOptions);
    }

    let reader = match open_index {
        Some(reader) => Borrowed::Lent(reader),
        None => match IndexReader::open_default() {
            Ok(Some(reader)) => Borrowed::Opened(reader),
            Ok(None) => return Err(NoIndex::NotBuilt),
            Err(error) => return Err(NoIndex::Unusable(format!("{error:#}"))),
        },
    };
    if reader.document_count() == 0 {
        return Err(NoIndex::Empty);
    }

    // Both sides are canonicalized: either may reach the same directory through
    // a symlink, and a prefix comparison of two different spellings is a "no".
    let covers = reader
        .content_root()
        .canonicalize()
        .unwrap_or_else(|_| reader.content_root().to_path_buf());
    let canonical_root = root.canonicalize().map_err(|_| NoIndex::OutOfScope {
        covers: covers.clone(),
    })?;
    if !canonical_root.starts_with(&covers) {
        return Err(NoIndex::OutOfScope { covers });
    }

    Ok(UsableIndex {
        reader,
        canonical_root,
        given_root: root.to_path_buf(),
    })
}

/// Lets the index choose which files to open, then matches them itself.
///
/// The index knows which documents hold the query's terms, not where in the
/// file they are, so the lines still come from the files — which also means a
/// document the index has not caught up with contributes nothing rather than a
/// stale line.
fn from_index(
    index: &UsableIndex,
    query: &str,
    matcher: &RegexMatcher,
    options: &Options,
) -> Result<Outcome> {
    let candidates = index
        .reader
        .candidates(query, INDEX_CANDIDATE_POOL)
        .context("the index could not be queried")?;

    let mut outcome = Outcome {
        answered_by: Answered::Index,
        ..Outcome::default()
    };
    let mut searcher = line_searcher();
    for candidate in candidates {
        if !candidate.starts_with(&index.canonical_root) {
            continue;
        }
        // The index holds dot-files; a walk hides them unless asked. Keeping the
        // two engines' defaults apart would make `--engine` change the answer.
        if !options.include_hidden && has_hidden_component(&candidate, &index.canonical_root) {
            continue;
        }
        let Ok(metadata) = candidate.symlink_metadata() else {
            // Indexed, then deleted. Reporting it would be answering from a
            // record of a file rather than from the file.
            continue;
        };
        // Indexed as a file, since become a symlink — the index has not caught
        // up yet. Searching it would read a file the scope does not contain,
        // which is the rule the walk applies to a symlink it meets; `--engine`
        // must not be what decides whether a search stays inside its tree. The
        // operand itself is the same exception the walk makes.
        if metadata.is_symlink() && candidate != index.canonical_root {
            continue;
        }
        let path = as_given(&candidate, &index.canonical_root, &index.given_root);
        collect_from(
            &path,
            metadata.is_dir(),
            matcher,
            &mut searcher,
            options,
            &mut outcome,
        );
        if reached_limit(options, &mut outcome) {
            break;
        }
    }
    Ok(outcome)
}

/// Re-spells an indexed absolute path the way the caller named its root, so
/// that which engine answered does not change what the paths look like.
fn as_given(path: &Path, canonical_root: &Path, given_root: &Path) -> PathBuf {
    match path.strip_prefix(canonical_root) {
        Ok(relative) => given_root.join(relative),
        Err(_) => path.to_path_buf(),
    }
}

/// Whether any component below `root` is a dot-file.
fn has_hidden_component(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| name.starts_with('.'))
        })
}

/// Whether a query is plain text, rather than something only a regex engine or
/// a query parser gives meaning to.
fn is_plain_text(query: &str) -> bool {
    query
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, ' ' | '_' | '-' | '\''))
}

/// How many more matches the caller will accept.
fn remaining(limit: Option<usize>, collected: usize) -> Option<usize> {
    limit.map(|limit| limit.saturating_sub(collected))
}

fn build_matcher(query: &str, options: &Options) -> Result<RegexMatcher> {
    RegexMatcherBuilder::new()
        .case_insensitive(options.case_insensitive)
        .fixed_strings(options.literal)
        .build(query)
        .with_context(|| format!("`{query}` is not a valid pattern"))
}

/// Collects one file's matching lines, stopping once `room` runs out.
struct Collector<'a> {
    path: &'a Path,
    results: &'a mut Vec<SearchResult>,
    room: Option<usize>,
}

impl Sink for Collector<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch) -> Result<bool, Self::Error> {
        if self.room == Some(0) {
            return Ok(false);
        }
        self.results.push(SearchResult {
            path: self.path.to_path_buf(),
            line_number: mat.line_number().unwrap_or(0) as usize,
            line_content: String::from_utf8_lossy(trim_line_terminator(mat.bytes())).into_owned(),
        });
        self.room = self.room.map(|room| room.saturating_sub(1));
        Ok(self.room != Some(0))
    }
}

/// Drops the line terminator the searcher includes in a match.
///
/// Every caller renders the line next to something else — a path and a line
/// number here, a row in the explorer there — so the trailing newline is never
/// wanted, and a CRLF file would otherwise leave a stray carriage return in the
/// middle of the output.
fn trim_line_terminator(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, contents: &[u8]) -> PathBuf {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn paths(outcome: &Outcome) -> Vec<String> {
        outcome
            .results
            .iter()
            .map(|result| result.path.display().to_string())
            .collect()
    }

    fn run(root: &Path, query: &str, options: &Options) -> Outcome {
        search(root, query, options).unwrap()
    }

    #[test]
    fn a_content_search_reports_the_line_it_matched() {
        let dir = tempfile::tempdir().unwrap();
        let notes = write(dir.path(), "notes.txt", b"first line\nsecond line\nthird\n");

        let outcome = run(dir.path(), "second", &Options::default());

        assert_eq!(outcome.results.len(), 1);
        let result = &outcome.results[0];
        assert_eq!(result.path, notes);
        assert_eq!(result.line_number, 2);
        assert_eq!(result.line_content, "second line");
        assert!(!outcome.truncated);
    }

    #[test]
    fn a_crlf_file_does_not_leave_a_carriage_return_in_the_line() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"alpha\r\nbeta\r\n");

        let outcome = run(dir.path(), "alpha", &Options::default());

        assert_eq!(outcome.results[0].line_content, "alpha");
    }

    #[test]
    fn a_root_that_names_one_file_searches_just_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let notes = write(dir.path(), "notes.txt", b"needle\n");
        write(dir.path(), "other.txt", b"needle\n");

        let outcome = run(&notes, "needle", &Options::default());

        assert_eq!(paths(&outcome), vec![notes.display().to_string()]);
    }

    #[test]
    fn case_and_literal_flags_change_what_the_query_means() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"Needle\na.b\n");

        assert!(
            run(dir.path(), "needle", &Options::default())
                .results
                .is_empty()
        );
        let insensitive = Options {
            case_insensitive: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "needle", &insensitive).results.len(), 1);

        // `a.b` as a regex also matches `a-b`; as a literal it does not.
        write(dir.path(), "dashed.txt", b"a-b\n");
        assert_eq!(run(dir.path(), "a.b", &Options::default()).results.len(), 2);
        let literal = Options {
            literal: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "a.b", &literal).results.len(), 1);
    }

    #[test]
    fn an_invalid_pattern_is_an_error_rather_than_an_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let error = search(dir.path(), "a(", &Options::default()).unwrap_err();
        assert!(
            error.to_string().contains("`a(` is not a valid pattern"),
            "unexpected error: {error}"
        );

        // The same query as a literal is a perfectly good one.
        write(dir.path(), "notes.txt", b"a(\n");
        let literal = Options {
            literal: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "a(", &literal).results.len(), 1);
    }

    #[test]
    fn a_name_search_matches_directories_as_well_as_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "reports/report.txt", b"nothing to find here\n");

        let names = Options {
            subject: Subject::Names,
            ..Options::default()
        };
        let outcome = run(dir.path(), "report", &names);

        let mut found = paths(&outcome);
        found.sort();
        assert_eq!(
            found,
            vec![
                dir.path().join("reports").display().to_string(),
                dir.path().join("reports/report.txt").display().to_string(),
            ]
        );
        // A name match has no line to point at.
        assert!(outcome.results.iter().all(|result| result.line_number == 0));
    }

    #[test]
    fn the_default_subject_matches_a_file_by_name_and_by_content_alike() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "needle.txt", b"a line with needle in it\n");

        let outcome = run(dir.path(), "needle", &Options::default());

        assert_eq!(outcome.results.len(), 2);
        // The name match comes first, and carries no line to quote.
        assert!(outcome.results[0].is_name_match());
        assert_eq!(outcome.results[1].line_number, 1);
    }

    #[test]
    fn the_root_itself_is_never_a_name_match() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("haystack");
        std::fs::create_dir(&root).unwrap();
        write(&root, "haystack.txt", b"x\n");

        let names = Options {
            subject: Subject::Names,
            ..Options::default()
        };
        let outcome = run(&root, "haystack", &names);

        assert_eq!(
            paths(&outcome),
            vec![root.join("haystack.txt").display().to_string()]
        );
    }

    #[test]
    fn hidden_files_and_ignored_files_are_skipped_unless_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".hidden.txt", b"needle\n");
        write(dir.path(), ".ignore", b"ignored.txt\n");
        write(dir.path(), "ignored.txt", b"needle\n");
        write(dir.path(), "plain.txt", b"needle\n");

        assert_eq!(
            paths(&run(dir.path(), "needle", &Options::default())),
            vec![dir.path().join("plain.txt").display().to_string()]
        );

        let everything = Options {
            include_hidden: true,
            respect_ignore_files: false,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "needle", &everything).results.len(), 3);
    }

    #[test]
    fn max_depth_bounds_how_far_the_walk_descends() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "top.txt", b"needle\n");
        write(dir.path(), "one/two/deep.txt", b"needle\n");

        let shallow = Options {
            max_depth: Some(1),
            ..Options::default()
        };
        assert_eq!(
            paths(&run(dir.path(), "needle", &shallow)),
            vec![dir.path().join("top.txt").display().to_string()]
        );
        assert_eq!(
            run(dir.path(), "needle", &Options::default()).results.len(),
            2
        );
    }

    #[test]
    fn a_limit_stops_the_search_and_says_that_it_did() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\nneedle\nneedle\n");

        let limited = Options {
            limit: Some(2),
            ..Options::default()
        };
        let outcome = run(dir.path(), "needle", &limited);
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.truncated);

        let unlimited = run(dir.path(), "needle", &Options::default());
        assert_eq!(unlimited.results.len(), 3);
        assert!(!unlimited.truncated);
    }

    #[test]
    fn a_limit_of_zero_searches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\n");

        let none = Options {
            limit: Some(0),
            ..Options::default()
        };
        assert!(run(dir.path(), "needle", &none).results.is_empty());
    }

    #[test]
    fn a_binary_file_is_not_searched_for_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "binary.bin", b"\0needle\nneedle\n");

        assert!(
            run(dir.path(), "needle", &Options::default())
                .results
                .is_empty()
        );
    }

    #[test]
    fn a_search_outside_what_the_index_covers_reads_the_files_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\n");

        // A temporary directory is never inside the indexed tree, so `auto`
        // has to fall back — whether or not this machine has an index at all.
        let outcome = run(dir.path(), "needle", &Options::default());

        assert_eq!(outcome.results.len(), 1);
        assert!(
            matches!(outcome.answered_by, Answered::Walk(Some(_))),
            "unexpected engine: {:?}",
            outcome.answered_by
        );
    }

    #[test]
    fn asking_for_the_walk_does_not_blame_the_index_for_answering() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\n");

        let walked = Options {
            engine: Engine::Walk,
            ..Options::default()
        };
        assert_eq!(
            run(dir.path(), "needle", &walked).answered_by,
            Answered::Walk(None)
        );
    }

    #[test]
    fn insisting_on_the_index_fails_with_the_reason_it_cannot_answer() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\n");

        let indexed = Options {
            engine: Engine::Index,
            ..Options::default()
        };
        let error = search(dir.path(), "needle", &indexed).unwrap_err();
        assert!(
            error.to_string().contains("the index cannot answer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_pattern_and_the_walk_only_flags_keep_the_index_out_of_it() {
        assert!(is_plain_text("report 2024"));
        assert!(is_plain_text("O'Brien"));
        assert!(!is_plain_text("fn\\s+\\w+"));
        assert!(!is_plain_text("ext:rs"));

        let dir = tempfile::tempdir().unwrap();
        let bounded = Options {
            engine: Engine::Index,
            max_depth: Some(1),
            ..Options::default()
        };
        let error = search(dir.path(), "needle", &bounded).unwrap_err();
        assert!(
            error.to_string().contains("bound a walk"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn an_indexed_path_is_reported_the_way_the_caller_named_its_root() {
        let indexed = Path::new("/home/me/Documents/project/src/main.rs");
        let canonical = Path::new("/home/me/Documents/project");

        assert_eq!(
            as_given(indexed, canonical, Path::new(".")),
            PathBuf::from("./src/main.rs")
        );
        // A path the root does not cover is left as it is rather than mangled.
        assert_eq!(
            as_given(Path::new("/elsewhere/x"), canonical, Path::new(".")),
            PathBuf::from("/elsewhere/x")
        );
    }

    #[test]
    fn a_dot_directory_below_the_root_counts_as_hidden_but_the_root_does_not() {
        let root = Path::new("/home/me/.nohrs/work");
        assert!(!has_hidden_component(&root.join("src/main.rs"), root));
        assert!(has_hidden_component(&root.join(".env"), root));
        assert!(has_hidden_component(&root.join(".cache/blob"), root));
    }

    /// This asserted the opposite until it was noticed that "no matches" and
    /// "there is no such directory" are different answers, and that a search
    /// which gives the first for the second exits `0` — telling a script that
    /// a path it misspelled holds nothing, rather than that it is not there.
    #[test]
    fn a_missing_root_is_a_failure_rather_than_an_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nowhere");

        let error = search(&missing, "needle", &Options::default())
            .expect_err("searching a path that is not there came back with an answer");

        // Kept as the `io::Error` it was, so a caller can tell a path that is
        // not there from one it is not allowed to read.
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::NotFound),
            "the failure did not say what was wrong with the path: {error:#}"
        );
    }

    /// Existing is not being readable. `ignore` hands back a directory it
    /// cannot descend into as a perfectly good entry and only then an error, so
    /// a walk that merely skipped that error reported "no matches" for a
    /// directory it never managed to read.
    #[cfg(unix)]
    #[test]
    fn a_root_that_cannot_be_read_is_a_failure_rather_than_an_empty_result() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        write(&locked, "notes.txt", b"a needle in here\n");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads a directory whatever its mode, so there is nothing to
        // observe on a machine where this test cannot lock anything.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let outcome = search(&locked, "needle", &Options::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let error = outcome.expect_err("an unreadable root came back as an answer");
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::PermissionDenied),
            "the failure did not say what was wrong with the path: {error:#}"
        );
    }

    /// `File::open` on a FIFO blocks until someone writes to it, so the one
    /// syscall meant to settle whether an operand is searchable would instead
    /// be where the command stopped — with no output and no way to tell it
    /// apart from a slow search.
    #[cfg(unix)]
    #[test]
    fn an_operand_that_would_block_on_open_is_refused_rather_than_opened() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !made {
            return;
        }

        // Would never return before: nothing has this pipe open for writing.
        let error = search(&fifo, "needle", &Options::default())
            .expect_err("a FIFO was accepted as something to search");

        assert!(
            error.to_string().contains("regular file"),
            "the failure did not say why the operand was refused: {error:#}"
        );
    }

    /// A name is knowable without reading the file, so refusing an unreadable
    /// operand is right for a content search and wrong for `--name`: it reports
    /// a failure for a question that has an answer.
    #[cfg(unix)]
    #[test]
    fn a_name_search_answers_for_a_file_it_may_not_open() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = write(dir.path(), "needle-named.txt", b"secret\n");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root opens a file whatever its mode, so there is nothing to observe
        // on a machine where this test cannot lock anything.
        if std::fs::File::open(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        let by_name = search(
            &locked,
            "needle",
            &Options {
                subject: Subject::Names,
                ..Options::default()
            },
        );
        let by_content = search(
            &locked,
            "secret",
            &Options {
                subject: Subject::Contents,
                ..Options::default()
            },
        );
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(
            paths(&by_name.expect("a name search was refused a file it never needed to open"))
                .len(),
            1
        );
        // The content search still fails, because that one really cannot be
        // answered without opening the file.
        assert!(
            by_content.is_err(),
            "an unreadable file answered a content search"
        );
    }

    /// `--max-depth 0` stops above a directory's entries, so listing it is not
    /// something the search needs. Refusing an unreadable directory there
    /// reports a failure for a search whose answer — no matches — does not
    /// depend on what is inside it.
    #[cfg(unix)]
    #[test]
    fn a_zero_depth_search_answers_for_a_directory_it_may_not_list() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        write(&locked, "needle.txt", b"a needle in here\n");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads a directory whatever its mode, so there is nothing to
        // observe on a machine where this test cannot lock anything.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let at_depth_zero = search(
            &locked,
            "needle",
            &Options {
                max_depth: Some(0),
                ..Options::default()
            },
        );
        let descending = search(&locked, "needle", &Options::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let outcome =
            at_depth_zero.expect("a zero-depth search was refused a directory it never listed");
        assert!(paths(&outcome).is_empty(), "{:?}", paths(&outcome));
        // A search that would have descended still fails, because that one
        // really cannot be answered without listing the directory.
        assert!(
            descending.is_err(),
            "an unlistable directory answered a descending search"
        );
    }

    /// A search scoped to a tree must not answer with a file that is not in it.
    /// The walk already declines to descend through a symlinked directory; a
    /// symlinked *file* was still opened, so its target's contents came back
    /// under a path inside the scope.
    #[cfg(unix)]
    #[test]
    fn a_symlink_does_not_bring_the_outside_into_the_scope() {
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.txt", b"a needle out here\n");
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            dir.path().join("link.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("dirlink")).unwrap();

        let outcome = search(dir.path(), "needle", &Options::default()).unwrap();

        assert!(
            outcome.results.is_empty(),
            "a search read through a symlink and out of its scope: {:?}",
            paths(&outcome)
        );
    }

    /// `--engine` chooses how a search is answered, never what it is allowed to
    /// read. The index records a path, and a file recorded as a file can be a
    /// symlink by the time a search reads it — so the engine that answers from
    /// records has to apply the same rule the walk applies to what it meets.
    #[cfg(unix)]
    #[test]
    fn the_index_engine_stays_inside_the_scope_the_walk_does() {
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.txt", b"a needle out here\n");
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("index");
        let content = dir.path().join("content");
        std::fs::create_dir_all(&content).unwrap();

        // Indexed while it is a real file, so the index records it as one.
        let decoy = content.join("notes.txt");
        std::fs::write(&decoy, "a needle in here\n").unwrap();
        let manager = crate::search::indexer::IndexManager::new_with_path(
            index_path.clone(),
            content.clone(),
        )
        .unwrap();
        manager
            .index_home(crate::search::indexer::Refresh::Everything, None)
            .unwrap();

        // Then it becomes a link out of the tree, before the index catches up.
        std::fs::remove_file(&decoy).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), &decoy).unwrap();

        let reader = IndexReader::open(index_path, content.clone())
            .unwrap()
            .expect("an index that was just built");
        let outcome = search_using(
            &content,
            "needle",
            &Options {
                engine: Engine::Index,
                ..Options::default()
            },
            Some(&reader),
        )
        .unwrap();

        assert_eq!(outcome.answered_by, Answered::Index);
        assert!(
            outcome.results.is_empty(),
            "the index engine read through a symlink the walk would have skipped: {:?}",
            paths(&outcome)
        );
    }

    /// The other side of that rule: a symlink the caller names is the caller
    /// pointing at a file, not the walk wandering out of its tree.
    #[cfg(unix)]
    #[test]
    fn a_symlink_named_as_the_operand_is_still_searched() {
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.txt", b"a needle out here\n");
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), &link).unwrap();

        let outcome = search(&link, "needle", &Options::default()).unwrap();

        assert_eq!(paths(&outcome).len(), 1, "{:?}", paths(&outcome));
    }

    /// The other half of the same distinction: a directory the walk cannot read
    /// part-way down is ordinary, and must not turn the rest of the answer into
    /// a failure.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_below_the_root_does_not_fail_the_search() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"a needle in here\n");
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads a directory whatever its mode, so there is nothing to
        // observe on a machine where this test cannot lock anything.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let outcome = search(dir.path(), "needle", &Options::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let outcome = outcome.expect("one unreadable directory failed the whole search");
        assert_eq!(paths(&outcome).len(), 1, "{:?}", paths(&outcome));
    }
}
