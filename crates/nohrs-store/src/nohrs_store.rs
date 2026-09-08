//! Persistence layer for nohrs (see `docs/persistence.md`).
//!
//! Two backends sit behind interface-segregated traits, split by a single
//! question: *does the data ever need to be queried by anything other than an
//! exact key?*
//!
//! * **SQLite** ([`SqliteStore`]) — file metadata and history, which need
//!   range/diff/ordered queries. Implements [`MetadataQuery`], [`MetadataStore`],
//!   and [`HistoryStore`]. Uses bundled SQLite in WAL mode.
//! * **redb** ([`RedbKvStore`]) — host key/value state (window position,
//!   tab/session restore, dynamic settings): pure `key -> blob`. Implements
//!   [`KvStore`].
//!
//! Both backends are synchronous and tokio-free; the UI layer is expected to
//! call them from `cx.background_spawn`. Per-operation performance logging is
//! controlled by [`StoreLogConfig`] (see `docs/persistence.md` §5).

use std::path::PathBuf;

mod redb_kv;
mod sqlite;

pub use redb_kv::RedbKvStore;
pub use sqlite::SqliteStore;

/// Errors returned by the store backends.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A SQLite operation failed.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A redb operation failed. Boxed because `redb::Error` is large and would
    /// otherwise bloat every `Result` in this crate (`clippy::result_large_err`).
    #[error("redb error: {0}")]
    Redb(Box<redb::Error>),
    /// A string was not a well-formed [`KvKey`].
    #[error("invalid kv key {key:?}: {reason}")]
    InvalidKey {
        /// The string that was rejected.
        key: String,
        /// What was wrong with it.
        reason: &'static str,
    },
}

// redb surfaces a family of error types from its different stages. Funnel each
// through `redb::Error` so `?` converts directly to [`StoreError`].
macro_rules! redb_error_from {
    ($($error:ty),+ $(,)?) => {$(
        impl From<$error> for StoreError {
            fn from(error: $error) -> Self {
                StoreError::Redb(Box::new(redb::Error::from(error)))
            }
        }
    )+};
}
redb_error_from!(
    redb::Error,
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
);

/// Result type used throughout the store crate.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Controls per-operation performance logging for the store backends. All
/// fields default to off, so a default config adds zero overhead (the SQLite
/// profile hook is not even installed). Output is emitted via `tracing` and can
/// be filtered with `RUST_LOG`; see `docs/persistence.md` §5.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreLogConfig {
    /// Log every SQL statement at `debug` (target `nohrs_store::sql`).
    pub log_all_queries: bool,
    /// Log SQL statements slower than this many milliseconds at `warn`
    /// (target `nohrs_store::sql`). Zero disables slow-query logging.
    pub slow_query_ms: u64,
    /// Log redb `get`/`put`/`delete`/`batch` operations and their durations at
    /// `debug` (target `nohrs_store::redb`).
    pub log_redb_ops: bool,
}

/// Primary key of a row in the `files` table.
pub type FileId = i64;

/// A row read back from the `files` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    /// Stable row identifier.
    pub id: FileId,
    /// Absolute path to the entry (unique).
    pub path: PathBuf,
    /// Path of the containing directory.
    pub parent_path: PathBuf,
    /// Filesystem inode number.
    pub inode: u64,
    /// Size in bytes.
    pub size: u64,
    /// Last-modified time in nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
    /// Content hash (blake3 first-N-KB), populated in P3.
    pub content_hash: Option<Vec<u8>>,
    /// Time the entry was last indexed (nanoseconds since epoch), or `None`.
    pub indexed_at: Option<i64>,
    /// Logical-deletion time (nanoseconds since epoch), or `None` if live.
    pub deleted_at: Option<i64>,
}

/// The data needed to insert or update a `files` row. `indexed_at` and
/// `deleted_at` are managed separately via [`MetadataStore::mark_indexed`] and
/// [`MetadataStore::delete_file`], so they are not part of an upsert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUpsert {
    /// Absolute path to the entry (the upsert conflict key).
    pub path: PathBuf,
    /// Path of the containing directory.
    pub parent_path: PathBuf,
    /// Filesystem inode number.
    pub inode: u64,
    /// Size in bytes.
    pub size: u64,
    /// Last-modified time in nanoseconds since the Unix epoch.
    pub mtime_ns: i64,
    /// Content hash, if already computed (otherwise `None`).
    pub content_hash: Option<Vec<u8>>,
}

/// The category of a [`HistoryEntry`], stored as the `kind` text column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// A file or directory was opened.
    Open,
    /// A search was run.
    Search,
    /// A command was invoked.
    Command,
}

impl HistoryKind {
    /// The lowercase string stored in the `kind` column.
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryKind::Open => "open",
            HistoryKind::Search => "search",
            HistoryKind::Command => "command",
        }
    }

    /// Parse the `kind` column spelling back into a [`HistoryKind`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(HistoryKind::Open),
            "search" => Some(HistoryKind::Search),
            "command" => Some(HistoryKind::Command),
            _ => None,
        }
    }
}

/// A single history record (recent files, searches, command usage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// What kind of event this is.
    pub kind: HistoryKind,
    /// Opaque, caller-defined payload (e.g. a path or query string).
    pub payload: String,
    /// When the event occurred, in nanoseconds since the Unix epoch.
    pub occurred_at: i64,
}

/// A single operation in a [`KvStore::batch`] call, applied atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvOp {
    /// Insert or overwrite `key` with `value`.
    Put {
        /// The key to write.
        key: KvKey,
        /// The value to store.
        value: Vec<u8>,
    },
    /// Remove `key` if present.
    Delete {
        /// The key to remove.
        key: KvKey,
    },
}

/// Read-only access to file metadata. Kept separate from [`MetadataStore`] so
/// callers that only read (e.g. plugins with `read_paths` permission) cannot
/// mutate the table.
pub trait MetadataQuery: Send + Sync {
    /// Fetch the record for `path`, or `None` if it is not tracked.
    fn get_file(&self, path: &std::path::Path) -> Result<Option<FileRecord>>;
    /// List the direct children of directory `parent`.
    fn list_children(&self, parent: &std::path::Path) -> Result<Vec<FileRecord>>;
    /// List records changed after `ts_ns` (modified or logically deleted since),
    /// for incremental re-indexing.
    fn list_changed_since(&self, ts_ns: i64) -> Result<Vec<FileRecord>>;
    /// Find a record by its filesystem inode, or `None` if untracked.
    fn find_by_inode(&self, inode: u64) -> Result<Option<FileRecord>>;
}

/// Read/write access to file metadata.
pub trait MetadataStore: MetadataQuery {
    /// Insert `entry`, or update the existing row with the same path. Returns
    /// the row id.
    fn upsert_file(&self, entry: &FileUpsert) -> Result<FileId>;
    /// Remove the row for `path` (hard delete in P2).
    fn delete_file(&self, path: &std::path::Path) -> Result<()>;
    /// Record that the file `id` was indexed at `indexed_at_ns`.
    fn mark_indexed(&self, id: FileId, indexed_at_ns: i64) -> Result<()>;
}

/// A host KV key: a namespace, then a name within it.
///
/// Keys are dot-separated segments and there are always at least two, so every
/// key names a namespace — `session.explorer_tabs`, `window.position`. Segments
/// are lowercase ASCII, digits and `_`.
///
/// The namespace is what lets [`KvStore::list_namespace`] hand a subsystem its
/// own state and nobody else's, and what stops two subsystems both reaching for
/// a bare `tabs`. That was a documented convention nothing checked: `put("tabs",
/// …)` compiled, round-tripped correctly, and went wrong only later and
/// elsewhere, as a listing quietly missing rows.
///
/// A literal key is checked at compile time by [`KvKey::from_static`], which is
/// where nearly all of them come from. [`KvKey::new`] and [`KvKey::parse`] cover
/// the built-at-runtime rest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KvKey(std::borrow::Cow<'static, str>);

impl KvKey {
    /// A key from a literal, rejected **at compile time** if malformed.
    ///
    /// This is a `const fn`, so `KvKey::from_static("tabs")` is a build error
    /// rather than a runtime one. Prefer it for the fixed keys a subsystem owns;
    /// it is the whole reason the convention is now enforceable rather than
    /// merely written down.
    ///
    /// # Panics
    ///
    /// If `key` is not a valid key. In a `const` context — which is where a
    /// literal belongs — that panic is a compile error and can never reach a
    /// running program.
    #[must_use]
    pub const fn from_static(key: &'static str) -> Self {
        assert!(
            is_valid_key(key),
            "a kv key must be <namespace>.<name>, lowercase ASCII, digits and _"
        );
        Self(std::borrow::Cow::Borrowed(key))
    }

    /// A key from a namespace and a name, both checked.
    pub fn new(namespace: &str, name: &str) -> Result<Self> {
        Self::parse(format!("{namespace}.{name}"))
    }

    /// A key from an existing string, checked.
    pub fn parse(key: impl Into<String>) -> Result<Self> {
        let key: String = key.into();
        if !is_valid_key(&key) {
            return Err(StoreError::InvalidKey {
                key,
                reason: "expected <namespace>.<name> in lowercase ASCII, digits and _",
            });
        }
        Ok(Self(std::borrow::Cow::Owned(key)))
    }

    /// The part before the first `.`.
    #[must_use]
    pub fn namespace(&self) -> &str {
        // A valid key always has one, so the fallback is unreachable in practice
        // and still not a panic.
        self.0.split('.').next().unwrap_or(&self.0)
    }

    /// The whole key, as stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for KvKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<&str> for KvKey {
    type Error = StoreError;

    fn try_from(key: &str) -> Result<Self> {
        Self::parse(key)
    }
}

/// Whether `key` is `<segment>.<segment>[.<segment>…]`, each segment non-empty
/// and made of lowercase ASCII, digits or `_`.
///
/// Written as a hand-rolled loop rather than `split`/`all` because it has to be
/// a `const fn` for [`KvKey::from_static`] to reject a bad literal at build
/// time, and iterators are not available there.
const fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut index = 0;
    let mut segment_len = 0;
    let mut segments = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'.' {
            if segment_len == 0 {
                return false;
            }
            segments += 1;
            segment_len = 0;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' {
            segment_len += 1;
        } else {
            return false;
        }
        index += 1;
    }
    segment_len > 0 && segments >= 2
}

/// A simple key/value blob store (host KV, backed by redb).
pub trait KvStore: Send + Sync {
    /// Fetch the value for `key`, or `None` if absent.
    fn get(&self, key: &KvKey) -> Result<Option<Vec<u8>>>;
    /// Insert or overwrite `key` with `value`.
    fn put(&self, key: &KvKey, value: &[u8]) -> Result<()>;
    /// Remove `key` if present.
    fn delete(&self, key: &KvKey) -> Result<()>;
    /// Return every `(key, value)` in `namespace`.
    ///
    /// Takes the namespace rather than a free prefix so that a listing cannot
    /// straddle one: `list_prefix("sess")` used to match `session.*` by
    /// accident, and `list_prefix("window")` would also return `window_backup.*`
    /// if such a namespace were ever added.
    fn list_namespace(&self, namespace: &str) -> Result<Vec<(KvKey, Vec<u8>)>>;
    /// Apply `ops` atomically in a single transaction.
    fn batch(&self, ops: Vec<KvOp>) -> Result<()>;
}

/// Append-only history of recent files, searches and commands.
pub trait HistoryStore: Send + Sync {
    /// Append `entry` to the history.
    fn record(&self, entry: HistoryEntry) -> Result<()>;
    /// Return up to `limit` entries of `kind`, most recent first.
    fn list(&self, kind: HistoryKind, limit: usize) -> Result<Vec<HistoryEntry>>;
}

/// Current time in nanoseconds since the Unix epoch, saturating to 0 if the
/// clock is set before the epoch.
pub(crate) fn now_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::Path;

    // A consumer that only needs to read metadata depends on the narrow
    // `MetadataQuery` trait, so it can be exercised against a hand-written mock
    // without a real database — the point of the interface segregation.
    fn newest_child(query: &dyn MetadataQuery, parent: &Path) -> Result<Option<FileRecord>> {
        Ok(query
            .list_children(parent)?
            .into_iter()
            .max_by_key(|record| record.mtime_ns))
    }

    #[derive(Default)]
    struct MockMetadataQuery {
        children: Vec<FileRecord>,
    }

    impl MetadataQuery for MockMetadataQuery {
        fn get_file(&self, path: &Path) -> Result<Option<FileRecord>> {
            Ok(self
                .children
                .iter()
                .find(|record| record.path == path)
                .cloned())
        }
        fn list_children(&self, _parent: &Path) -> Result<Vec<FileRecord>> {
            Ok(self.children.clone())
        }
        fn list_changed_since(&self, _ts_ns: i64) -> Result<Vec<FileRecord>> {
            Ok(self.children.clone())
        }
        fn find_by_inode(&self, inode: u64) -> Result<Option<FileRecord>> {
            Ok(self
                .children
                .iter()
                .find(|record| record.inode == inode)
                .cloned())
        }
    }

    fn record(path: &str, inode: u64, mtime_ns: i64) -> FileRecord {
        FileRecord {
            id: inode as FileId,
            path: PathBuf::from(path),
            parent_path: PathBuf::from("/home/user"),
            inode,
            size: 0,
            mtime_ns,
            content_hash: None,
            indexed_at: None,
            deleted_at: None,
        }
    }

    #[test]
    fn consumer_works_against_metadata_query_mock() {
        let mock = MockMetadataQuery {
            children: vec![
                record("/home/user/a.txt", 1, 100),
                record("/home/user/b.txt", 2, 300),
                record("/home/user/c.txt", 3, 200),
            ],
        };
        let newest = newest_child(&mock, Path::new("/home/user"))
            .unwrap()
            .unwrap();
        assert_eq!(newest.path, PathBuf::from("/home/user/b.txt"));
    }

    #[test]
    fn history_kind_round_trips_through_text() {
        for kind in [HistoryKind::Open, HistoryKind::Search, HistoryKind::Command] {
            assert_eq!(HistoryKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(HistoryKind::parse("unknown"), None);
    }

    #[test]
    fn a_key_needs_a_namespace_and_a_name() {
        for key in [
            "session.explorer_tabs",
            "window.position",
            "window.main.position",
            "plugin.acme_2.state",
        ] {
            assert!(KvKey::parse(key).is_ok(), "{key} should be a valid key");
        }
        for key in [
            // The one this exists for: a bare name, which used to compile and
            // then be missing from every namespace listing.
            "tabs",
            "",
            ".",
            ".leading",
            "trailing.",
            "double..dot",
            "Upper.case",
            "has space.x",
            "dash-ed.x",
        ] {
            assert!(KvKey::parse(key).is_err(), "{key} should be rejected");
        }
    }

    #[test]
    fn a_key_reports_the_namespace_it_belongs_to() {
        assert_eq!(
            KvKey::from_static("session.explorer_tabs").namespace(),
            "session"
        );
        // The namespace is the *first* segment, not everything before the last
        // dot: `list_namespace("window")` has to find this key.
        assert_eq!(
            KvKey::from_static("window.main.position").namespace(),
            "window"
        );
    }

    #[test]
    fn new_joins_and_checks_both_halves() {
        let key = KvKey::new("session", "explorer_tabs").unwrap();
        assert_eq!(key.as_str(), "session.explorer_tabs");
        assert_eq!(key.to_string(), "session.explorer_tabs");

        // A name that smuggles in its own separator is still checked as a whole,
        // so it cannot produce an empty segment.
        assert!(KvKey::new("session", "").is_err());
        assert!(KvKey::new("", "tabs").is_err());
        assert!(KvKey::new("session", ".tabs").is_err());
    }

    #[test]
    fn the_compile_time_and_runtime_checks_agree() {
        // `from_static` is a `const fn` and `parse` is not, so they are two
        // paths to one rule. A literal that `parse` rejects has to be a build
        // error, not a key that only `from_static` lets through.
        for key in ["session.tabs", "window.main.position"] {
            assert!(is_valid_key(key) && KvKey::parse(key).is_ok(), "{key}");
        }
        for key in ["tabs", "trailing.", "Upper.case"] {
            assert!(!is_valid_key(key) && KvKey::parse(key).is_err(), "{key}");
        }
    }

    #[test]
    fn an_invalid_key_says_what_it_was() {
        let error = KvKey::parse("tabs").unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("tabs"), "{rendered}");
        assert!(rendered.contains("namespace"), "{rendered}");
    }
}
