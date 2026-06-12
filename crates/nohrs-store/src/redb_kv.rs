//! redb backend: host key/value state (`docs/persistence.md` §3).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use redb::{Database, ReadableDatabase, TableDefinition, TableError};

use crate::{KvOp, KvStore, Result, StoreLogConfig};

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
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let started = Instant::now();
        let read_txn = self.database.begin_read()?;
        let value = match read_txn.open_table(HOST_KV) {
            Ok(table) => table.get(key)?.map(|guard| guard.value().to_vec()),
            // No writes have happened yet: an absent table means an absent key.
            Err(TableError::TableDoesNotExist(_)) => None,
            Err(error) => return Err(error.into()),
        };
        self.trace("get", started);
        Ok(value)
    }

    fn put(&self, key: &str, value: &[u8]) -> Result<()> {
        let started = Instant::now();
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(HOST_KV)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        self.trace("put", started);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let started = Instant::now();
        let write_txn = self.database.begin_write()?;
        {
            let mut table = write_txn.open_table(HOST_KV)?;
            table.remove(key)?;
        }
        write_txn.commit()?;
        self.trace("delete", started);
        Ok(())
    }

    fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let started = Instant::now();
        let read_txn = self.database.begin_read()?;
        let table = match read_txn.open_table(HOST_KV) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut matches = Vec::new();
        // Keys are ordered, so once one stops matching the prefix none after it can.
        for entry in table.range(prefix..)? {
            let (key, value) = entry?;
            let key = key.value();
            if !key.starts_with(prefix) {
                break;
            }
            matches.push((key.to_string(), value.value().to_vec()));
        }
        self.trace("list_prefix", started);
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

    fn store() -> RedbKvStore {
        RedbKvStore::open_in_memory(&StoreLogConfig::default()).unwrap()
    }

    #[test]
    fn get_missing_key_before_any_write() {
        let store = store();
        assert_eq!(store.get("absent").unwrap(), None);
        assert!(store.list_prefix("any").unwrap().is_empty());
    }

    #[test]
    fn put_get_delete_round_trip() {
        let store = store();
        store.put("window.position", b"1,2,3,4").unwrap();
        assert_eq!(
            store.get("window.position").unwrap().as_deref(),
            Some(&b"1,2,3,4"[..])
        );
        store.delete("window.position").unwrap();
        assert_eq!(store.get("window.position").unwrap(), None);
    }

    #[test]
    fn list_prefix_returns_only_matching_keys() {
        let store = store();
        store.put("session.tabs", b"a").unwrap();
        store.put("session.active", b"b").unwrap();
        store.put("window.position", b"c").unwrap();
        let mut session = store.list_prefix("session.").unwrap();
        session.sort();
        assert_eq!(session.len(), 2);
        assert_eq!(session[0].0, "session.active");
        assert_eq!(session[1].0, "session.tabs");
    }

    #[test]
    fn batch_is_applied_atomically() {
        let store = store();
        store.put("keep", b"x").unwrap();
        store
            .batch(vec![
                KvOp::Put {
                    key: "a".to_string(),
                    value: b"1".to_vec(),
                },
                KvOp::Put {
                    key: "b".to_string(),
                    value: b"2".to_vec(),
                },
                KvOp::Delete {
                    key: "keep".to_string(),
                },
            ])
            .unwrap();
        assert_eq!(store.get("a").unwrap().as_deref(), Some(&b"1"[..]));
        assert_eq!(store.get("b").unwrap().as_deref(), Some(&b"2"[..]));
        assert_eq!(store.get("keep").unwrap(), None);
    }

    #[test]
    fn opening_with_op_logging_enabled_still_works() {
        let log = StoreLogConfig {
            log_redb_ops: true,
            ..Default::default()
        };
        let store = RedbKvStore::open_in_memory(&log).unwrap();
        store.put("k", b"v").unwrap();
        store
            .batch(vec![KvOp::Put {
                key: "x".to_string(),
                value: b"y".to_vec(),
            }])
            .unwrap();
        assert_eq!(store.get("k").unwrap().as_deref(), Some(&b"v"[..]));
        assert_eq!(store.list_prefix("").unwrap().len(), 2);
    }

    #[test]
    fn data_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.redb");
        {
            let store = RedbKvStore::open(&path, &StoreLogConfig::default()).unwrap();
            store.put("session.tabs", b"restored").unwrap();
        }
        let reopened = RedbKvStore::open(&path, &StoreLogConfig::default()).unwrap();
        assert_eq!(
            reopened.get("session.tabs").unwrap().as_deref(),
            Some(&b"restored"[..])
        );
    }
}
