//! An in-memory index of file and directory *names* under a root directory.
//!
//! Deliberately separate from the tantivy content index in [`super::indexer`]:
//! the launcher must produce candidates within a single keystroke
//! (docs/launcher.md §12 budgets 50ms), which means the names have to already be
//! in RAM rather than behind a query against an on-disk index. Only the path and
//! a directory flag are kept per entry, so a whole home directory costs tens of
//! megabytes rather than the content index's gigabytes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};

/// Directory names skipped wholesale: they hold machine-generated trees that are
/// large, deep, and never what someone is searching for by name. `.git` and
/// friends are also hidden, but the walk includes hidden entries under
/// [`FileIndexConfig::include_hidden`], so the list is checked independently.
const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    ".cargo",
    ".rustup",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".nuxt",
    ".tox",
    ".gradle",
    ".m2",
    "Caches",
    "DerivedData",
];

/// One indexed filesystem entry.
///
/// The file name is a slice of `path` rather than a second allocation, which
/// matters at index scale: the launcher holds hundreds of thousands of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedEntry {
    path: PathBuf,
    is_dir: bool,
}

impl IndexedEntry {
    /// Builds an entry, returning `None` when the path has no file name or the
    /// name is not valid UTF-8 — such an entry can be neither fuzzy-matched nor
    /// displayed faithfully, so it is dropped at index time instead of forcing
    /// every consumer to handle it.
    pub fn new(path: PathBuf, is_dir: bool) -> Option<Self> {
        path.file_name()?.to_str()?;
        Some(Self { path, is_dir })
    }

    /// The entry's full path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// The entry's file name. Non-empty and valid UTF-8 by construction.
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    }

    /// How many path components separate the entry from `root`, used to prefer
    /// shallower matches when ranking. Returns 0 when the entry is not under
    /// `root`.
    pub fn depth_below(&self, root: &Path) -> usize {
        self.path
            .strip_prefix(root)
            .map(|relative| relative.components().count())
            .unwrap_or(0)
    }
}

/// What to scan and how far.
#[derive(Debug, Clone)]
pub struct FileIndexConfig {
    /// Directory the scan starts from.
    pub root: PathBuf,
    /// Hard cap on indexed entries, bounding memory on pathological trees.
    pub max_entries: usize,
    /// How deep below `root` the walk descends.
    pub max_depth: usize,
    /// Whether dot-files and dot-directories are indexed.
    pub include_hidden: bool,
}

impl FileIndexConfig {
    /// Default entry cap. At roughly 150 bytes per entry this bounds the index
    /// near 30MB, which is affordable for an always-warm launcher.
    pub const DEFAULT_MAX_ENTRIES: usize = 200_000;

    /// Default walk depth. Deeper than this and paths stop being things people
    /// recall by name, while the entry count grows fastest.
    pub const DEFAULT_MAX_DEPTH: usize = 12;

    /// Configuration for the current user's home directory.
    pub fn for_home() -> Result<Self> {
        let root = dirs::home_dir().context("home directory not found")?;
        Ok(Self::for_root(root))
    }

    /// Configuration for an arbitrary root, with the default limits.
    pub fn for_root(root: PathBuf) -> Self {
        Self {
            root,
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            max_depth: Self::DEFAULT_MAX_DEPTH,
            include_hidden: false,
        }
    }
}

/// Walks `config.root` and returns the entries below it.
///
/// Synchronous and slow enough to matter (seconds on a large home directory),
/// so callers run it on a background executor — see
/// [`FileNameIndex::rebuild`].
///
/// Fails when `root` is not a readable directory. That case has to be an error
/// rather than an empty result: an unreadable home would otherwise be installed
/// as a perfectly good empty index, and every search would answer "no results"
/// for a scan that never happened.
pub fn scan(config: &FileIndexConfig) -> Result<Vec<IndexedEntry>> {
    let metadata = std::fs::metadata(&config.root)
        .with_context(|| format!("cannot read {}", config.root.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("{} is not a directory", config.root.display());
    }
    // Existing and being a directory is not the same as being listable. Without
    // this the walker reports the refusal as an entry error, which is skipped
    // like any unreadable subdirectory, and the scan succeeds with nothing in
    // it — the empty index this function exists to prevent.
    std::fs::read_dir(&config.root)
        .with_context(|| format!("cannot list {}", config.root.display()))?;

    let mut entries = Vec::new();
    let walker = ignore::WalkBuilder::new(&config.root)
        .hidden(!config.include_hidden)
        // Git-ignored files are still files a person may want to open by name —
        // a local `.env`, a scratch note, a build output they are hunting for.
        // Hiding them is a source-control policy, not a search one; the noise
        // that actually matters is pruned by `SKIPPED_DIRECTORIES` instead.
        .git_ignore(false)
        .max_depth(Some(config.max_depth))
        .filter_entry(|entry| !is_skipped_directory(entry))
        .build();

    for result in walker {
        if entries.len() >= config.max_entries {
            tracing::warn!(
                "file name index truncated at {} entries under {}",
                config.max_entries,
                config.root.display()
            );
            break;
        }
        match result {
            Ok(entry) => {
                // Depth 0 is `root` itself, which is not a search candidate.
                if entry.depth() == 0 {
                    continue;
                }
                let is_dir = is_directory(&entry);
                if let Some(indexed) = IndexedEntry::new(entry.into_path(), is_dir) {
                    entries.push(indexed);
                }
            }
            // An unreadable directory is normal (permissions, races) and must
            // not abort the scan; record it for diagnosis and move on.
            Err(error) => tracing::debug!("skipping unreadable entry: {error}"),
        }
    }

    Ok(entries)
}

/// Whether a walked entry is a directory, following symlinks.
///
/// A symlink's own file type is not its target's, so a link to a directory
/// would otherwise be indexed — and shown — as a file. Resolving it costs a
/// `stat`, which is why only symlinks pay for it.
fn is_directory(entry: &ignore::DirEntry) -> bool {
    match entry.file_type() {
        Some(kind) if kind.is_dir() => true,
        Some(kind) if kind.is_symlink() => entry.path().is_dir(),
        _ => false,
    }
}

/// Whether the walk should prune this entry.
///
/// Only directories are pruned: the names in [`SKIPPED_DIRECTORIES`] describe
/// trees, and a regular file that happens to be called `target` or `venv` is an
/// ordinary file someone may be looking for. The root is never pruned, so a home
/// directory that happens to carry one of these names still gets scanned.
fn is_skipped_directory(entry: &ignore::DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_some_and(|kind| kind.is_dir())
        && entry
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name))
}

/// How far along the index is, which is what the UI reports when a search
/// comes back with nothing.
///
/// "Empty" has three quite different meanings to a user — still working, found
/// nothing, and broken — and answering all three with "no results" is what
/// makes a launcher feel unreliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexState {
    /// The first scan has not finished. Results so far are incomplete.
    Scanning,
    /// A scan completed; the index is whatever the filesystem held.
    Ready,
    /// The scan failed, so there is nothing to search and no point waiting.
    Failed,
}

/// A shared, swappable snapshot of the name index.
///
/// Readers take an `Arc` of the current entries and match against it without
/// holding the lock, so a rebuild never blocks a keystroke.
///
/// The snapshot is replaced wholesale by a rescan and does not track the
/// filesystem live: files created after startup are not searchable until the
/// next scan. Wiring it to the watcher behind the content index is future work.
#[derive(Debug, Default)]
pub struct FileNameIndex {
    entries: RwLock<Arc<[IndexedEntry]>>,
    ready: AtomicBool,
    failed: AtomicBool,
}

impl FileNameIndex {
    /// An empty index that is not yet ready.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current entries. Cheap: it clones an `Arc`, not the entries.
    pub fn snapshot(&self) -> Arc<[IndexedEntry]> {
        match self.entries.read() {
            Ok(guard) => guard.clone(),
            // The lock only ever guards an `Arc` swap, so a panic elsewhere
            // cannot have left a half-written snapshot behind.
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// How far along the index is.
    pub fn state(&self) -> IndexState {
        if self.failed.load(Ordering::Acquire) {
            IndexState::Failed
        } else if self.ready.load(Ordering::Acquire) {
            IndexState::Ready
        } else {
            IndexState::Scanning
        }
    }

    /// Number of indexed entries.
    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    /// Whether the index holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Installs `entries` as the current snapshot and marks the index ready.
    ///
    /// Also clears a previous failure: a rescan that succeeds supersedes one
    /// that did not.
    pub fn replace(&self, entries: Vec<IndexedEntry>) {
        let entries: Arc<[IndexedEntry]> = entries.into();
        match self.entries.write() {
            Ok(mut guard) => *guard = entries,
            Err(poisoned) => *poisoned.into_inner() = entries,
        }
        self.failed.store(false, Ordering::Release);
        self.ready.store(true, Ordering::Release);
    }

    /// Records that there is nothing to scan, so callers stop waiting on one.
    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Release);
    }

    /// Scans `config.root` and installs the result. Blocking — call it from a
    /// background executor.
    ///
    /// A failed scan marks the index failed rather than leaving it to look like
    /// a scan still in progress.
    pub fn rebuild(&self, config: &FileIndexConfig) -> Result<()> {
        let started = std::time::Instant::now();
        let entries = match scan(config) {
            Ok(entries) => entries,
            Err(error) => {
                self.mark_failed();
                return Err(error);
            }
        };
        tracing::info!(
            "indexed {} names under {} in {:?}",
            entries.len(),
            config.root.display(),
            started.elapsed()
        );
        self.replace(entries);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn write_tree(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/left-pad")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("README.md"), "# readme").unwrap();
        std::fs::write(root.join("node_modules/left-pad/index.js"), "x").unwrap();
        std::fs::write(root.join(".hidden/secret.txt"), "x").unwrap();
    }

    fn names(entries: &[IndexedEntry]) -> Vec<&str> {
        let mut names: Vec<&str> = entries.iter().map(IndexedEntry::name).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn scan_collects_files_and_directories_but_skips_noise() {
        let dir = tempfile::tempdir().unwrap();
        write_tree(dir.path());

        let entries = scan(&FileIndexConfig::for_root(dir.path().to_path_buf())).unwrap();

        // `src` (a directory) and its file are indexed; node_modules is pruned
        // wholesale and hidden entries are excluded by default.
        assert_eq!(names(&entries), vec!["README.md", "main.rs", "src"]);
        assert!(
            entries
                .iter()
                .any(|entry| entry.name() == "src" && entry.is_dir())
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.name() == "main.rs" && !entry.is_dir())
        );
    }

    #[test]
    fn scan_includes_hidden_entries_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        write_tree(dir.path());

        let mut config = FileIndexConfig::for_root(dir.path().to_path_buf());
        config.include_hidden = true;
        let entries = scan(&config).unwrap();

        assert!(entries.iter().any(|entry| entry.name() == "secret.txt"));
        // Pruned directory names are skipped even when hidden entries are in.
        assert!(entries.iter().all(|entry| entry.name() != "left-pad"));
    }

    #[test]
    fn scan_respects_depth_and_entry_limits() {
        let dir = tempfile::tempdir().unwrap();
        write_tree(dir.path());

        let mut shallow = FileIndexConfig::for_root(dir.path().to_path_buf());
        shallow.max_depth = 1;
        // At depth 1 only the root's direct children are visited, so `src` is
        // indexed but `src/main.rs` is not.
        assert_eq!(names(&scan(&shallow).unwrap()), vec!["README.md", "src"]);

        let mut capped = FileIndexConfig::for_root(dir.path().to_path_buf());
        capped.max_entries = 1;
        assert_eq!(scan(&capped).unwrap().len(), 1);
    }

    #[test]
    fn a_git_ignored_file_is_still_indexed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "secret.txt\n").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "x").unwrap();

        let mut config = FileIndexConfig::for_root(dir.path().to_path_buf());
        config.include_hidden = true;
        // Git-ignored is not the same as unwanted: the launcher opens files, it
        // does not commit them.
        assert!(
            scan(&config)
                .unwrap()
                .iter()
                .any(|entry| entry.name() == "secret.txt")
        );
    }

    #[test]
    fn a_file_named_like_a_noise_directory_survives() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/artifact.bin"), "x").unwrap();
        std::fs::write(dir.path().join("venv"), "a regular file").unwrap();

        let entries = scan(&FileIndexConfig::for_root(dir.path().to_path_buf())).unwrap();
        let names = names(&entries);
        // The directory is pruned with everything under it; the file is not.
        assert!(names.contains(&"venv"), "{names:?}");
        assert!(!names.contains(&"artifact.bin"), "{names:?}");
    }

    #[test]
    fn a_root_named_like_a_noise_directory_is_still_scanned() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("inside.txt"), "x").unwrap();

        // Pruning applies below the root, never to the root itself.
        assert_eq!(
            names(&scan(&FileIndexConfig::for_root(root)).unwrap()),
            vec!["inside.txt"]
        );
    }

    #[test]
    fn an_unreadable_root_is_an_error_not_an_empty_index() {
        let index = FileNameIndex::new();
        let missing = FileIndexConfig::for_root(PathBuf::from("/definitely/not/here"));

        assert!(scan(&missing).is_err());
        assert!(index.rebuild(&missing).is_err());
        // Not "Ready and empty", which would read to the user as "no results".
        assert_eq!(index.state(), IndexState::Failed);
    }

    #[cfg(unix)]
    #[test]
    fn a_root_that_exists_but_cannot_be_listed_is_an_error_too() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("locked");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("findme.txt"), "x").unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Root ignores the mode bits, so where the suite runs as root there is
        // no refusal to test. Checked rather than assumed, because whether the
        // read is actually denied is what the test needs — not who we are.
        let denied = std::fs::read_dir(&root).is_err();
        let result = scan(&FileIndexConfig::for_root(root.clone()));
        // Restored before the assertions so a failure still leaves a removable
        // directory behind.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();

        if !denied {
            return;
        }
        // `metadata` succeeds here — the directory exists and is a directory —
        // so only the listing probe catches it. Skipping the refusal the way an
        // unreadable subdirectory is skipped would report "no results" for a
        // home the user simply cannot read.
        assert!(result.is_err(), "an unlistable root should not scan clean");
    }

    #[test]
    fn a_later_successful_scan_clears_an_earlier_failure() {
        let dir = tempfile::tempdir().unwrap();
        write_tree(dir.path());
        let index = FileNameIndex::new();

        index
            .rebuild(&FileIndexConfig::for_root(PathBuf::from(
                "/definitely/not/here",
            )))
            .unwrap_err();
        assert_eq!(index.state(), IndexState::Failed);

        index
            .rebuild(&FileIndexConfig::for_root(dir.path().to_path_buf()))
            .unwrap();
        assert_eq!(index.state(), IndexState::Ready);
        assert!(!index.is_empty());
    }

    #[test]
    fn entry_rejects_non_displayable_paths() {
        assert!(IndexedEntry::new(PathBuf::from("/"), true).is_none());
        let entry = IndexedEntry::new(PathBuf::from("/a/b/c.txt"), false).unwrap();
        assert_eq!(entry.name(), "c.txt");
        assert_eq!(entry.depth_below(Path::new("/a")), 2);
        // A path outside the root reports depth 0 rather than failing.
        assert_eq!(entry.depth_below(Path::new("/elsewhere")), 0);
    }

    #[test]
    fn index_starts_empty_and_becomes_ready_after_replace() {
        let index = FileNameIndex::new();
        assert_eq!(index.state(), IndexState::Scanning);
        assert!(index.is_empty());

        let entry = IndexedEntry::new(PathBuf::from("/a/b.txt"), false).unwrap();
        index.replace(vec![entry.clone()]);

        assert_eq!(index.state(), IndexState::Ready);
        assert_eq!(index.len(), 1);
        assert_eq!(index.snapshot().first(), Some(&entry));
    }
}
