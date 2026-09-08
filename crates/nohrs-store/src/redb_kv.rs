//! redb backend: host key/value state (`docs/persistence.md` §3).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use redb::{Database, ReadableDatabase, TableDefinition, TableError};

use crate::{KvKey, KvOp, KvStore, Result, StoreLogConfig};

/// The single host KV table. Keys are namespaced strings (e.g.
/// `"window.position"`, `"session.tabs"`); values are opaque blobs the caller
/// serializes (JSON / MessagePack / …).
const HOST_KV: TableDefinition<'static, &str, &[u8]> = TableDefinition::new("kv");

/// redb-backed host [`KvStore`] (`state.redb`).
pub struct RedbKvStore {
    database: Arc<Database>,
    log_ops: bool,
}

impl RedbKvStore {
    /// Open (creating if needed) the database at `path`.
    pub fn open(path: &Path, log: &StoreLogConfig) -> Result<Self> {
        Ok(Self {
            database: Arc::new(Database::create(path)?),
            log_ops: log.log_redb_ops,
        })
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory(log: &StoreLogConfig) -> Result<Self> {
        let database =
            Database::builder().create_with_backend(redb::backends::InMemoryBackend::new())?;
        Ok(Self {
            database: Arc::new(database),
            log_ops: log.log_redb_ops,
        })
    }

    fn trace(&self, op: &str, started: Instant) {
        if self.log_ops {
            let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            tracing::debug!(target: "nohrs_store::redb", op, elapsed_us, "kv op");
        }
    }
}

impl KvStore for RedbKvStore {
    fn get(&self, key: &KvKey) -> Result<Option<Vec<u8>>> {
        let started = Instant::now();
        let read_txn = self.database.begin_read()?;
        let value = match read_txn.open_table(HOST_KV) {
            Ok(table) => table.get(key.as_str())?.map(|guard| guard.value().to_vec()),
            // No writes have happened yet: an absent table means an absent key.
            Err(TableError::TableDoesNotExist(_)) => None,
            Err(error) => return Err(error.into()),
        };
        self.trace("get", started);
        Ok(value)
    }

    fn put(&self, key: &KvKey, value: &[u8]) -> Result<()> {
        let started = Instant::now();
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(HOST_KV)?;
            table.insert(key.as_str(), value)?;
        }
        write_txn.commit()?;
        self.trace("put", started);
        Ok(())
    }

    fn delete(&self, key: &KvKey) -> Result<()> {
        let started = Instant::now();
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(HOST_KV)?;
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        self.trace("delete", started);
        Ok(())
    }

    fn list_namespace(&self, namespace: &str) -> Result<Vec<(KvKey, Vec<u8>)>> {
        let started = Instant::now();
        // The trailing dot is what confines the scan to the namespace itself:
        // scanning from `window` alone would also walk into a `window_backup`
        // namespace, since `window_backup.x` sorts after `window`.
        let prefix = format!("{namespace}.");
        let read_txn = self.database.begin_read()?;
        let table = match read_txn.open_table(HOST_KV) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut matches = Vec::new();
        // Keys are ordered, so once one stops matching the prefix none after it can.
        for entry in table.range(prefix.as_str()..)? {
            let (key, value) = entry?;
            let key = key.value();
            if !key.starts_with(&prefix) {
                break;
            }
            // Every key written through this API is a `KvKey`, so a row that
            // does not parse predates the validation (or came from another
            // writer). It is skipped rather than returned, because the caller
            // asked for keys and this is not one — but *reported*, since a row
            // vanishing from a listing with no explanation is the kind of thing
            // that gets diagnosed as data loss. Failing the whole listing
            // instead would let one stale row break session restore, which is a
            // worse trade for the same problem.
            let key = match KvKey::parse(key) {
                Ok(key) => key,
                Err(error) => {
                    tracing::warn!(
                        target: "nohrs_store::redb",
                        "skipping unparseable key in namespace {namespace}: {error}"
                    );
                    continue;
                }
            };
            matches.push((key, value.value().to_vec()));
        }
        self.trace("list_namespace", started);
        Ok(matches)
    }

    fn batch(&self, ops: Vec<KvOp>) -> Result<()> {
        let started = Instant::now();
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(HOST_KV)?;
            for op in &ops {
                match op {
                    KvOp::Put { key, value } => {
                        table.insert(key.as_str(), value.as_slice())?;
                    }
                    KvOp::Delete { key } => {
                        table.remove(key.as_str())?;
                    }
                }
            }
        }
        write_txn.commit()?;
        self.trace("batch", started);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::kv_key;

    fn store() -> RedbKvStore {
        RedbKvStore::open_in_memory(&StoreLogConfig::default()).unwrap()
    }

    #[test]
    fn get_missing_key_before_any_write() {
        let store = store();
        assert_eq!(store.get(&kv_key!("window.absent")).unwrap(), None);
        assert!(store.list_namespace("window").unwrap().is_empty());
    }

    #[test]
    fn put_get_delete_round_trip() {
        let store = store();
        let key = kv_key!("window.position");
        store.put(&key, b"1,2,3,4").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(&b"1,2,3,4"[..]));
        store.delete(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
    }

    #[test]
    fn list_namespace_returns_only_that_namespace() {
        let store = store();
        store.put(&kv_key!("session.tabs"), b"a").unwrap();
        store.put(&kv_key!("session.active"), b"b").unwrap();
        store.put(&kv_key!("window.position"), b"c").unwrap();
        let mut session = store.list_namespace("session").unwrap();
        session.sort();
        assert_eq!(session.len(), 2);
        assert_eq!(session[0].0.as_str(), "session.active");
        assert_eq!(session[1].0.as_str(), "session.tabs");
    }

    #[test]
    fn a_row_that_predates_the_validation_is_skipped_not_returned() {
        // An older build could write any string. Such a row is not a `KvKey`, so
        // `list_namespace` cannot hand it back — but the valid rows beside it
        // must still come through, rather than one stale key failing the whole
        // listing and taking session restore with it.
        let store = store();
        store.put(&kv_key!("session.tabs"), b"mine").unwrap();
        // Written past the API, the way an older build would have.
        {
            let write_txn = store.database.begin_write().unwrap();
            {
                let mut table = write_txn.open_table(HOST_KV).unwrap();
                table.insert("session.Legacy", &b"theirs"[..]).unwrap();
            }
            write_txn.commit().unwrap();
        }

        let listed = store.list_namespace("session").unwrap();

        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].0.as_str(), "session.tabs");
        // Still reachable by an exact read, so it is skipped, not destroyed.
        let read_txn = store.database.begin_read().unwrap();
        let table = read_txn.open_table(HOST_KV).unwrap();
        assert!(table.get("session.Legacy").unwrap().is_some());
    }

    #[test]
    fn a_namespace_listing_does_not_straddle_into_a_longer_one() {
        // `range("window"..)` would walk into `window_backup.*`, since it sorts
        // after `window`. The trailing dot is what confines it — and this is the
        // bug a free `list_prefix` invited.
        let store = store();
        store.put(&kv_key!("window.position"), b"mine").unwrap();
        store
            .put(&kv_key!("window_backup.position"), b"theirs")
            .unwrap();

        let listed = store.list_namespace("window").unwrap();

        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].0.as_str(), "window.position");
    }

    #[test]
    fn batch_is_applied_atomically() {
        let store = store();
        store.put(&kv_key!("session.keep"), b"x").unwrap();
        store
            .batch(vec![
                KvOp::Put {
                    key: kv_key!("session.a"),
                    value: b"1".to_vec(),
                },
                KvOp::Put {
                    key: kv_key!("session.b"),
                    value: b"2".to_vec(),
                },
                KvOp::Delete {
                    key: kv_key!("session.keep"),
                },
            ])
            .unwrap();
        assert_eq!(
            store.get(&kv_key!("session.a")).unwrap().as_deref(),
            Some(&b"1"[..])
        );
        assert_eq!(
            store.get(&kv_key!("session.b")).unwrap().as_deref(),
            Some(&b"2"[..])
        );
        assert_eq!(store.get(&kv_key!("session.keep")).unwrap(), None);
    }

    #[test]
    fn opening_with_op_logging_enabled_still_works() {
        let log = StoreLogConfig {
            log_redb_ops: true,
            ..Default::default()
        };
        let store = RedbKvStore::open_in_memory(&log).unwrap();
        store.put(&kv_key!("window.k"), b"v").unwrap();
        store
            .batch(vec![KvOp::Put {
                key: kv_key!("window.x"),
                value: b"y".to_vec(),
            }])
            .unwrap();
        assert_eq!(
            store.get(&kv_key!("window.k")).unwrap().as_deref(),
            Some(&b"v"[..])
        );
        assert_eq!(store.list_namespace("window").unwrap().len(), 2);
    }

    #[test]
    fn data_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.redb");
        {
            let store = RedbKvStore::open(&path, &StoreLogConfig::default()).unwrap();
            store.put(&kv_key!("session.tabs"), b"restored").unwrap();
        }
        let reopened = RedbKvStore::open(&path, &StoreLogConfig::default()).unwrap();
        assert_eq!(
            reopened.get(&kv_key!("session.tabs")).unwrap().as_deref(),
            Some(&b"restored"[..])
        );
    }
}
