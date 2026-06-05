//! SQLite backend: file metadata and history (`docs/persistence.md` §2).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    FileId, FileRecord, FileUpsert, HistoryEntry, HistoryKind, HistoryStore, MetadataQuery,
    MetadataStore, Result, StoreLogConfig, now_ns,
};

/// Forward-only migrations, applied in order and recorded in `_migrations`.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/001_init.sql"))];

// rusqlite's `profile` hook takes a bare `fn` pointer (not a closure), so the
// query-logging thresholds are kept in process-global atomics that the callback
// reads. A nohrs process opens a single metadata store, so this global state is
// effectively per-store; the last `open` wins if several are created.
static LOG_ALL_QUERIES: AtomicBool = AtomicBool::new(false);
static SLOW_QUERY_MS: AtomicU64 = AtomicU64::new(0);

/// SQLite-backed metadata and history store.
///
/// The connection is wrapped in a `Mutex` (SQLite WAL allows one writer plus
/// many readers, but a single `Connection` is not `Sync`), so callers should
/// keep operations short and run them off the UI thread.
pub struct SqliteStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open (creating if needed) the database at `path`, run pending migrations,
    /// and install query logging per `log`.
    pub fn open(path: &Path, log: &StoreLogConfig) -> Result<Self> {
        Self::from_connection(Connection::open(path)?, log)
    }

    /// Open an in-memory database (for tests). Each call is an isolated database.
    pub fn open_in_memory(log: &StoreLogConfig) -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?, log)
    }

    fn from_connection(mut connection: Connection, log: &StoreLogConfig) -> Result<Self> {
        // WAL gives single-writer / many-reader concurrency. `query_row` (not
        // `pragma_update`) because the statement returns the resulting mode.
        connection.query_row("PRAGMA journal_mode=WAL;", [], |row| {
            row.get::<_, String>(0)
        })?;
        // `synchronous=NORMAL` is the recommended pairing with WAL: it fsyncs at
        // checkpoints rather than every commit, so a power loss may drop the last
        // few transactions but never corrupts the database. Our data (metadata /
        // history) is a rebuildable cache, so trading that durability for far
        // fewer fsyncs is the right call.
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        configure_query_logging(&mut connection, log);
        migrate(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Lock the connection, recovering from a poisoned mutex (a panic in another
    /// thread while holding the lock) rather than propagating the panic — the
    /// connection itself remains usable.
    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Install (or not) the rusqlite profile hook based on `log`. When both
/// thresholds are off the hook is left uninstalled so logging adds no overhead.
fn configure_query_logging(connection: &mut Connection, log: &StoreLogConfig) {
    LOG_ALL_QUERIES.store(log.log_all_queries, Ordering::Relaxed);
    SLOW_QUERY_MS.store(log.slow_query_ms, Ordering::Relaxed);
    if log.log_all_queries || log.slow_query_ms > 0 {
        connection.profile(Some(profile_callback));
    } else {
        connection.profile(None);
    }
}

fn profile_callback(sql: &str, duration: Duration) {
    let elapsed_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    let slow_threshold = SLOW_QUERY_MS.load(Ordering::Relaxed);
    if slow_threshold > 0 && elapsed_ms >= slow_threshold {
        tracing::warn!(target: "nohrs_store::sql", elapsed_ms, sql, "slow query");
    } else if LOG_ALL_QUERIES.load(Ordering::Relaxed) {
        tracing::debug!(target: "nohrs_store::sql", elapsed_ms, sql, "query");
    }
}

fn migrate(connection: &mut Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (\
             version INTEGER PRIMARY KEY, \
             applied_at INTEGER NOT NULL\
         );",
    )?;
    for (version, sql) in MIGRATIONS {
        let already_applied: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if !already_applied {
            // Apply the migration and record its version in one transaction:
            // SQLite DDL is transactional, so a crash mid-migration rolls back
            // cleanly. Otherwise a half-applied migration with no bookkeeping
            // would re-run the (non-idempotent) DDL on next startup and fail.
            let transaction = connection.transaction()?;
            transaction.execute_batch(sql)?;
            transaction.execute(
                "INSERT INTO _migrations (version, applied_at) VALUES (?1, ?2)",
                params![version, now_ns()],
            )?;
            transaction.commit()?;
        }
    }
    Ok(())
}

/// Columns selected by every `files` query, in the order [`row_to_file`] reads.
const FILE_COLUMNS: &str =
    "id, path, parent_path, inode, size, mtime_ns, content_hash, indexed_at, deleted_at";

fn row_to_file(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
    let path: String = row.get(1)?;
    let parent_path: String = row.get(2)?;
    let inode: i64 = row.get(3)?;
    let size: i64 = row.get(4)?;
    Ok(FileRecord {
        id: row.get(0)?,
        path: PathBuf::from(path),
        parent_path: PathBuf::from(parent_path),
        inode: inode as u64,
        size: size as u64,
        mtime_ns: row.get(5)?,
        content_hash: row.get(6)?,
        indexed_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl MetadataQuery for SqliteStore {
    fn get_file(&self, path: &Path) -> Result<Option<FileRecord>> {
        let connection = self.connection();
        // `prepare_cached` reuses the compiled statement across calls (these are
        // hot paths once the P3 indexer runs); the SQL text is the cache key.
        let mut statement = connection
            .prepare_cached(&format!("SELECT {FILE_COLUMNS} FROM files WHERE path = ?1"))?;
        let record = statement
            .query_row([path_str(path)], row_to_file)
            .optional()?;
        Ok(record)
    }

    fn list_children(&self, parent: &Path) -> Result<Vec<FileRecord>> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE parent_path = ?1 ORDER BY path"
        ))?;
        let rows = statement.query_map([path_str(parent)], row_to_file)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn list_changed_since(&self, ts_ns: i64) -> Result<Vec<FileRecord>> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files \
             WHERE mtime_ns > ?1 OR (deleted_at IS NOT NULL AND deleted_at > ?1) \
             ORDER BY mtime_ns"
        ))?;
        let rows = statement.query_map([ts_ns], row_to_file)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn find_by_inode(&self, inode: u64) -> Result<Option<FileRecord>> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE inode = ?1 LIMIT 1"
        ))?;
        let record = statement
            .query_row([inode as i64], row_to_file)
            .optional()?;
        Ok(record)
    }
}

impl MetadataStore for SqliteStore {
    fn upsert_file(&self, entry: &FileUpsert) -> Result<FileId> {
        let connection = self.connection();
        // `excluded` refers to the row that would have been inserted; on a path
        // conflict we refresh the changeable columns but leave indexed_at /
        // deleted_at to the dedicated methods.
        let mut statement = connection.prepare_cached(
            "INSERT INTO files (path, parent_path, inode, size, mtime_ns, content_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(path) DO UPDATE SET \
                 parent_path = excluded.parent_path, \
                 inode = excluded.inode, \
                 size = excluded.size, \
                 mtime_ns = excluded.mtime_ns, \
                 content_hash = excluded.content_hash \
             RETURNING id",
        )?;
        let id = statement.query_row(
            params![
                path_str(&entry.path),
                path_str(&entry.parent_path),
                entry.inode as i64,
                entry.size as i64,
                entry.mtime_ns,
                entry.content_hash,
            ],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    fn delete_file(&self, path: &Path) -> Result<()> {
        let connection = self.connection();
        connection
            .prepare_cached("DELETE FROM files WHERE path = ?1")?
            .execute([path_str(path)])?;
        Ok(())
    }

    fn mark_indexed(&self, id: FileId, indexed_at_ns: i64) -> Result<()> {
        let connection = self.connection();
        connection
            .prepare_cached("UPDATE files SET indexed_at = ?2 WHERE id = ?1")?
            .execute(params![id, indexed_at_ns])?;
        Ok(())
    }
}

impl HistoryStore for SqliteStore {
    fn record(&self, entry: HistoryEntry) -> Result<()> {
        let connection = self.connection();
        connection
            .prepare_cached("INSERT INTO history (kind, payload, occurred_at) VALUES (?1, ?2, ?3)")?
            .execute(params![
                entry.kind.as_str(),
                entry.payload,
                entry.occurred_at
            ])?;
        Ok(())
    }

    fn list(&self, kind: HistoryKind, limit: usize) -> Result<Vec<HistoryEntry>> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(
            "SELECT kind, payload, occurred_at FROM history \
             WHERE kind = ?1 ORDER BY occurred_at DESC LIMIT ?2",
        )?;
        // Saturate rather than wrap: a `usize` past `i64::MAX` would otherwise
        // become a negative LIMIT, which SQLite treats as "no limit".
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement.query_map(params![kind.as_str(), limit], |row| {
            let kind_text: String = row.get(0)?;
            Ok(HistoryEntry {
                // The kind came straight from our own enum, so it always parses.
                kind: HistoryKind::parse(&kind_text).unwrap_or(kind),
                payload: row.get(1)?,
                occurred_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn store() -> SqliteStore {
        SqliteStore::open_in_memory(&StoreLogConfig::default()).unwrap()
    }

    fn sample(path: &str, inode: u64, mtime_ns: i64) -> FileUpsert {
        FileUpsert {
            path: PathBuf::from(path),
            parent_path: PathBuf::from("/home/user"),
            inode,
            size: 10,
            mtime_ns,
            content_hash: None,
        }
    }

    #[test]
    fn upsert_then_get_round_trips() {
        let store = store();
        let id = store
            .upsert_file(&sample("/home/user/a.txt", 1, 100))
            .unwrap();
        let record = store
            .get_file(Path::new("/home/user/a.txt"))
            .unwrap()
            .unwrap();
        assert_eq!(record.id, id);
        assert_eq!(record.inode, 1);
        assert_eq!(record.mtime_ns, 100);
        assert_eq!(record.indexed_at, None);
    }

    #[test]
    fn upsert_updates_existing_row_in_place() {
        let store = store();
        let first = store
            .upsert_file(&sample("/home/user/a.txt", 1, 100))
            .unwrap();
        let second = store
            .upsert_file(&sample("/home/user/a.txt", 1, 200))
            .unwrap();
        assert_eq!(first, second, "same path keeps the same row id");
        let record = store
            .get_file(Path::new("/home/user/a.txt"))
            .unwrap()
            .unwrap();
        assert_eq!(record.mtime_ns, 200);
    }

    #[test]
    fn list_children_and_find_by_inode() {
        let store = store();
        store
            .upsert_file(&sample("/home/user/a.txt", 11, 100))
            .unwrap();
        store
            .upsert_file(&sample("/home/user/b.txt", 12, 100))
            .unwrap();
        let children = store.list_children(Path::new("/home/user")).unwrap();
        assert_eq!(children.len(), 2);
        let found = store.find_by_inode(12).unwrap().unwrap();
        assert_eq!(found.path, PathBuf::from("/home/user/b.txt"));
    }

    #[test]
    fn list_changed_since_filters_by_mtime() {
        let store = store();
        store
            .upsert_file(&sample("/home/user/old.txt", 1, 100))
            .unwrap();
        store
            .upsert_file(&sample("/home/user/new.txt", 2, 300))
            .unwrap();
        let changed = store.list_changed_since(200).unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].path, PathBuf::from("/home/user/new.txt"));
    }

    #[test]
    fn mark_indexed_and_delete() {
        let store = store();
        let id = store
            .upsert_file(&sample("/home/user/a.txt", 1, 100))
            .unwrap();
        store.mark_indexed(id, 500).unwrap();
        let record = store
            .get_file(Path::new("/home/user/a.txt"))
            .unwrap()
            .unwrap();
        assert_eq!(record.indexed_at, Some(500));
        store.delete_file(Path::new("/home/user/a.txt")).unwrap();
        assert!(
            store
                .get_file(Path::new("/home/user/a.txt"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn history_records_and_lists_most_recent_first() {
        let store = store();
        for (payload, at) in [("first", 100), ("second", 200), ("third", 300)] {
            store
                .record(HistoryEntry {
                    kind: HistoryKind::Open,
                    payload: payload.to_string(),
                    occurred_at: at,
                })
                .unwrap();
        }
        store
            .record(HistoryEntry {
                kind: HistoryKind::Search,
                payload: "query".to_string(),
                occurred_at: 250,
            })
            .unwrap();
        let opens = store.list(HistoryKind::Open, 2).unwrap();
        assert_eq!(opens.len(), 2);
        assert_eq!(opens[0].payload, "third");
        assert_eq!(opens[1].payload, "second");
        let searches = store.list(HistoryKind::Search, 10).unwrap();
        assert_eq!(searches.len(), 1);
    }

    #[test]
    fn opening_with_query_logging_enabled_still_works() {
        let log = StoreLogConfig {
            log_all_queries: true,
            slow_query_ms: 1,
            log_redb_ops: false,
        };
        let store = SqliteStore::open_in_memory(&log).unwrap();
        let id = store
            .upsert_file(&sample("/home/user/a.txt", 1, 100))
            .unwrap();
        assert!(
            store
                .get_file(Path::new("/home/user/a.txt"))
                .unwrap()
                .is_some()
        );
        store.mark_indexed(id, 1).unwrap();
    }

    #[test]
    fn data_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        {
            let store = SqliteStore::open(&path, &StoreLogConfig::default()).unwrap();
            store
                .upsert_file(&sample("/home/user/a.txt", 1, 100))
                .unwrap();
        }
        let reopened = SqliteStore::open(&path, &StoreLogConfig::default()).unwrap();
        assert!(
            reopened
                .get_file(Path::new("/home/user/a.txt"))
                .unwrap()
                .is_some()
        );
    }
}
