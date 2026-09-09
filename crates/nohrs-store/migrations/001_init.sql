-- Initial schema for the nohrs metadata/history store (docs/persistence.md §2).
-- Forward-only; the `_migrations` bookkeeping table is created by the runner.
-- Host KV (window position, tab/session restore) lives in redb, not here.

CREATE TABLE files (
    id            INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    parent_path   TEXT NOT NULL,
    inode         INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    mtime_ns      INTEGER NOT NULL,
    content_hash  BLOB,                   -- blake3 first-N-KB hash (P3)
    indexed_at    INTEGER,                -- Tantivy indexing time (P3)
    deleted_at    INTEGER                 -- logical delete (P3 watcher)
);
CREATE INDEX idx_files_parent ON files(parent_path);
CREATE INDEX idx_files_inode  ON files(inode);

CREATE TABLE history (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('open', 'search', 'command')),
    payload     TEXT NOT NULL,
    occurred_at INTEGER NOT NULL
);
CREATE INDEX idx_history_kind_time ON history(kind, occurred_at DESC);
