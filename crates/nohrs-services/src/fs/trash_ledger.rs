//! A record of what nohrs moved to the trash, so it can be put back later.
//!
//! The operating system trash is not a uniform data source. Linux and Windows
//! store each item's original location next to it (`.trashinfo` files, `$I`
//! files) and the `trash` crate can read that back through its `os_limited`
//! module. macOS keeps the equivalent "Put Back" information inside Finder's
//! private `.DS_Store`, and `os_limited` is not even compiled there — so on the
//! platform nohrs targets first, nothing outside Finder can answer "where did
//! this come from?".
//!
//! nohrs therefore keeps its own ledger: every item the GUI or `noh rm` trashes
//! is appended here with the metadata needed to find it in the trash again and
//! restore it. The ledger is deliberately *beside* the OS trash rather than
//! replacing it, so trashed items still show up in Finder where users expect
//! them (see `docs/cli.md` §2).
//!
//! The format is JSON Lines: appending a record is a single `write`, and a
//! partially written tail costs at most the last entry. It moves into
//! `nohrs-store` when the SQLite layer lands in P2.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nohrs_core::config::paths;
use nohrs_core::errors::{Error, Result};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// File name of the ledger inside the nohrs data directory.
const LEDGER_FILE: &str = "trash-ledger.jsonl";

/// One item nohrs moved to the trash.
///
/// The metadata is captured *before* the move so it describes the item as it
/// was at its original location; that is what lets [`crate::fs::ops`] and the
/// CLI find the item again inside a trash directory that does not record where
/// anything came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashRecord {
    /// Handle for this record, unique within the ledger.
    pub id: String,
    /// Absolute path the item occupied before it was trashed.
    pub original_path: PathBuf,
    /// The item's file name, stored separately so a record whose path became
    /// unreadable text can still be matched against a trash directory entry.
    pub file_name: String,
    /// Size in bytes at the time of the move (`0` for directories).
    pub size: u64,
    /// Modification time as seconds since the Unix epoch, when the filesystem
    /// reports one.
    pub modified_unix: Option<i64>,
    /// When the item was trashed, as seconds since the Unix epoch.
    pub deleted_at_unix: i64,
    /// Whether the item was a directory.
    pub is_dir: bool,
}

impl TrashRecord {
    /// Describe `path` as it is now, before it moves to the trash.
    ///
    /// The location is recorded as an absolute path: the restore will not
    /// necessarily run from the working directory the deletion did, so a
    /// relative one would put the item back somewhere else entirely.
    /// `std::path::absolute` does not resolve symlinks, so a link is recorded as
    /// itself rather than as its target.
    pub fn capture(path: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        let path = &std::path::absolute(path)?;
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| Error::Other(format!("path has no file name: {}", path.display())))?;
        let now = OffsetDateTime::now_utc();
        Ok(Self {
            id: new_id(path, now),
            original_path: path.to_path_buf(),
            file_name,
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_unix: metadata.modified().ok().and_then(unix_seconds),
            deleted_at_unix: now.unix_timestamp(),
            is_dir: metadata.is_dir(),
        })
    }
}

/// The append-only ledger file.
#[derive(Debug, Clone)]
pub struct TrashLedger {
    path: PathBuf,
}

impl TrashLedger {
    /// Where the ledger lives, whether or not it exists yet. Reading this
    /// creates nothing, unlike [`open_default`](Self::open_default).
    pub fn default_path() -> PathBuf {
        paths::data_dir().join(LEDGER_FILE)
    }

    /// Open the ledger in the nohrs data directory, creating the directory if
    /// it does not exist yet.
    pub fn open_default() -> Result<Self> {
        std::fs::create_dir_all(paths::data_dir())?;
        Ok(Self {
            path: Self::default_path(),
        })
    }

    /// A ledger stored at an explicit path. Tests point this at a temporary
    /// directory so they never touch the developer's own trash history.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Where this ledger is stored.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record.
    ///
    /// The file is opened per call rather than kept open: records are written
    /// once per trashed item and read rarely, and holding no handle means a
    /// running GUI and a concurrent `noh rm` cannot fight over a stale one.
    pub fn append(&self, record: &TrashRecord) -> Result<()> {
        let line = encode(record)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Every record in the ledger, oldest first. A ledger that does not exist
    /// yet reads as empty.
    ///
    /// A line that cannot be decoded is skipped with a warning instead of
    /// failing the read: one truncated entry must not hide every other item in
    /// the trash.
    pub fn records(&self) -> Result<Vec<TrashRecord>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Error::Io(error)),
        };
        let mut records = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TrashRecord>(&line) {
                Ok(record) => records.push(record),
                Err(error) => tracing::warn!(
                    ledger = %self.path.display(),
                    line = index + 1,
                    %error,
                    "skipping an unreadable trash ledger entry"
                ),
            }
        }
        Ok(records)
    }

    /// Drop the records carrying these ids, returning how many were removed.
    ///
    /// The file is rewritten through a temporary file and renamed into place, so
    /// an interrupted prune leaves the previous ledger intact rather than a
    /// half-written one.
    pub fn forget(&self, ids: &HashSet<String>) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let records = self.records()?;
        let before = records.len();
        let kept: Vec<TrashRecord> = records
            .into_iter()
            .filter(|record| !ids.contains(&record.id))
            .collect();
        if kept.len() == before {
            return Ok(0);
        }
        let temporary = self.path.with_extension("jsonl.tmp");
        {
            let mut writer = BufWriter::new(File::create(&temporary)?);
            for record in &kept {
                writeln!(writer, "{}", encode(record)?)?;
            }
            writer.flush()?;
        }
        std::fs::rename(&temporary, &self.path)?;
        Ok(before - kept.len())
    }
}

/// The trash directory that items deleted from the home volume land in.
///
/// Items trashed from another volume go to that volume's own trash instead
/// (`/Volumes/<name>/.Trashes/<uid>` on macOS); those are not tracked yet.
pub fn home_trash_dir() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir()
            .ok_or_else(|| Error::Other("could not determine the home directory".to_string()))?;
        Ok(home.join(".Trash"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        // The freedesktop layout, which `paths::data_home` already resolves
        // ($XDG_DATA_HOME, else ~/.local/share).
        Ok(paths::data_home().join("Trash").join("files"))
    }
}

fn encode(record: &TrashRecord) -> Result<String> {
    serde_json::to_string(record)
        .map_err(|error| Error::Other(format!("could not encode a trash record: {error}")))
}

/// A handle that stays unique when several items are trashed in the same
/// second: the timestamp is taken in nanoseconds and combined with a hash of the
/// path, so two files removed by one `noh rm` never collide. The hash only has
/// to be unique, not stable across runs.
fn new_id(path: &Path, now: OffsetDateTime) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}-{:x}", now.unix_timestamp_nanos(), hasher.finish())
}

/// Seconds since the Unix epoch, negative for the pre-epoch timestamps some
/// filesystems still carry.
fn unix_seconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_secs()).ok(),
        Err(error) => i64::try_from(error.duration().as_secs())
            .ok()
            .map(|seconds| -seconds),
    }
}

#[cfg(test)]
// The fixtures build real files to capture metadata from, so they need the
// synchronous filesystem calls that app code routes through this crate instead.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ledger_in(dir: &Path) -> TrashLedger {
        TrashLedger::at(dir.join("ledger.jsonl"))
    }

    fn record_for(path: &Path) -> TrashRecord {
        TrashRecord::capture(path).unwrap()
    }

    #[test]
    fn a_missing_ledger_reads_as_empty() {
        let dir = tempdir().unwrap();
        assert!(ledger_in(dir.path()).records().unwrap().is_empty());
    }

    #[test]
    fn records_round_trip_through_the_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();
        let ledger = ledger_in(dir.path());
        let record = record_for(&file);

        ledger.append(&record).unwrap();

        assert_eq!(ledger.records().unwrap(), vec![record]);
    }

    #[test]
    fn capture_describes_the_item_before_the_move() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();

        let record = record_for(&file);

        assert_eq!(record.original_path, file);
        assert_eq!(record.file_name, "notes.txt");
        assert_eq!(record.size, 7);
        assert!(!record.is_dir);
        assert!(record.modified_unix.is_some());
        assert!(record.deleted_at_unix > 0);
    }

    #[test]
    fn capture_reports_a_directory_as_one_with_no_size() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("project");
        fs::create_dir(&nested).unwrap();

        let record = record_for(&nested);

        assert!(record.is_dir);
        assert_eq!(
            record.size, 0,
            "a directory's own byte count is meaningless"
        );
    }

    #[test]
    fn capture_records_an_absolute_location() {
        // Cargo runs unit tests with the package root as the working directory,
        // so this crate's own manifest is a relative path that exists.
        let record = TrashRecord::capture(Path::new("Cargo.toml")).unwrap();

        assert!(
            record.original_path.is_absolute(),
            "a relative location would restore to the wrong directory: {}",
            record.original_path.display()
        );
        assert!(record.original_path.ends_with("Cargo.toml"));
    }

    #[test]
    fn capture_fails_on_a_missing_path() {
        let dir = tempdir().unwrap();
        assert!(TrashRecord::capture(&dir.path().join("ghost.txt")).is_err());
    }

    #[test]
    fn items_trashed_together_get_distinct_ids() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("a.txt");
        let second = dir.path().join("b.txt");
        fs::write(&first, "a").unwrap();
        fs::write(&second, "b").unwrap();

        assert_ne!(record_for(&first).id, record_for(&second).id);
        // The same path trashed twice is two separate items, so those ids must
        // differ as well.
        assert_ne!(record_for(&first).id, record_for(&first).id);
    }

    #[test]
    fn forget_drops_only_the_named_records() {
        let dir = tempdir().unwrap();
        let ledger = ledger_in(dir.path());
        let mut ids = Vec::new();
        for name in ["a.txt", "b.txt", "c.txt"] {
            let path = dir.path().join(name);
            fs::write(&path, name).unwrap();
            let record = record_for(&path);
            ids.push(record.id.clone());
            ledger.append(&record).unwrap();
        }

        let removed = ledger
            .forget(&HashSet::from([
                ids[1].clone(),
                "not-in-the-ledger".to_string(),
            ]))
            .unwrap();

        assert_eq!(removed, 1);
        let remaining: Vec<String> = ledger
            .records()
            .unwrap()
            .into_iter()
            .map(|record| record.id)
            .collect();
        assert_eq!(remaining, vec![ids[0].clone(), ids[2].clone()]);
    }

    #[test]
    fn forget_without_ids_leaves_the_file_alone() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();
        let ledger = ledger_in(dir.path());
        ledger.append(&record_for(&file)).unwrap();

        assert_eq!(ledger.forget(&HashSet::new()).unwrap(), 0);
        assert_eq!(ledger.records().unwrap().len(), 1);
    }

    #[test]
    fn an_unreadable_line_is_skipped_rather_than_failing_the_read() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();
        let ledger = ledger_in(dir.path());
        ledger.append(&record_for(&file)).unwrap();
        // A crash mid-append leaves a partial line; the entries before it are
        // still perfectly good.
        fs::write(
            ledger.path(),
            format!(
                "{}\n{{\"id\": \"truncated\"\n\n",
                encode(&record_for(&file)).unwrap()
            ),
        )
        .unwrap();

        let records = ledger.records().unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].file_name, "notes.txt");
    }

    #[test]
    fn the_home_trash_directory_is_named_for_the_platform() {
        let trash = home_trash_dir().unwrap();
        if cfg!(target_os = "macos") {
            assert!(trash.ends_with(".Trash"), "{}", trash.display());
        } else {
            assert!(trash.ends_with("Trash/files"), "{}", trash.display());
        }
    }
}
