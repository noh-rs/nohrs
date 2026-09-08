-- Where trashed items came from, so they can be put back (docs/cli.md §7.1).
-- Only written on platforms whose own trash keeps no such record (macOS, where
-- the equivalent lives in Finder's private .DS_Store); Linux and Windows are
-- restored from the OS trash index instead.
--
-- `original_path` is deliberately not unique: the same path can be trashed
-- repeatedly, and each of those is a separate item to restore.

CREATE TABLE trash (
    id            INTEGER PRIMARY KEY,
    original_path TEXT NOT NULL,
    file_name     TEXT NOT NULL,          -- kept separately: the trash renames
                                          -- an item whose name is taken
    size          INTEGER NOT NULL,       -- 0 for directories
    modified_ns   INTEGER,                -- mtime at the time of the move
    trashed_at    INTEGER NOT NULL,
    is_dir        INTEGER NOT NULL
);
CREATE INDEX idx_trash_time ON trash(trashed_at DESC);
