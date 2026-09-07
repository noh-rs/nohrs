//! The trash as something that can be listed, restored from, and emptied.
//!
//! Where the information behind that comes from depends on the platform, which
//! is what the [`Store`] trait abstracts:
//!
//! * Linux and Windows record each item's original location in the trash itself
//!   (`.trashinfo` files, `$I` files) and the `trash` crate exposes it through
//!   `os_limited`. [`OsStore`] delegates there, so a listing also covers items
//!   other applications trashed and restoring cleans up the OS bookkeeping.
//! * macOS keeps the equivalent inside Finder's private `.DS_Store` and compiles
//!   no `os_limited` at all, so [`LedgerStore`] reads the `trash` table in
//!   `nohrs-store` ([`TrashLedger`]) — written by [`ops::trash_path`] as items
//!   go in — and finds each one in `~/.Trash` again by its recorded name, size,
//!   and modification time.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

// Only the freedesktop branch of `home_trash_dir` resolves an XDG base
// directory; macOS reads the home directory instead.
#[cfg(not(target_os = "macos"))]
use nohrs_core::config::paths;
use nohrs_core::errors::{Error, Result};
use nohrs_store::{TrashEntry, TrashId, TrashLedger};

use crate::fs::ops;

/// One item currently in the trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Handle used to address this item within its [`Store`].
    pub id: String,
    /// Where the item was before it was trashed.
    pub original_path: PathBuf,
    /// When it was trashed, as seconds since the Unix epoch. Seconds because
    /// that is all an OS trash index records; ties are broken by the order
    /// [`Store::list`] returns items in, not by this field.
    pub deleted_at_unix: i64,
    /// Whether the item is a directory.
    pub is_dir: bool,
}

impl Item {
    /// The name the item had at its original location.
    pub fn file_name(&self) -> String {
        self.original_path
            .file_name()
            .unwrap_or(self.original_path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

/// What can be done to the trash.
///
/// [`restore`](Store::restore) and [`purge`](Store::purge) address items by the
/// ids handed out by the preceding [`list`](Store::list) call, so a caller
/// always lists before it acts. `list` returns items most recently trashed
/// first.
pub trait Store {
    /// Every item currently in the trash, most recently trashed first.
    fn list(&mut self) -> Result<Vec<Item>>;
    /// Move `item` back to its original location, which must be free.
    fn restore(&mut self, item: &Item) -> Result<()>;
    /// Delete `item` from the trash for good.
    fn purge(&mut self, item: &Item) -> Result<()>;
}

/// A [`Store`] backed by the nohrs trash ledger, for platforms where the OS does
/// not record where a trashed item came from.
pub struct LedgerStore {
    ledger: Arc<dyn TrashLedger>,
    trash_dir: PathBuf,
    /// Where each listed item sits inside `trash_dir`, and which ledger row it
    /// came from. Filled in by [`Store::list`].
    located: HashMap<String, Located>,
}

struct Located {
    row: TrashId,
    path: PathBuf,
}

impl LedgerStore {
    /// A store over a ledger and a trash directory.
    pub fn new(ledger: Arc<dyn TrashLedger>, trash_dir: impl Into<PathBuf>) -> Self {
        Self {
            ledger,
            trash_dir: trash_dir.into(),
            located: HashMap::new(),
        }
    }

    fn source_of(&self, item: &Item) -> Result<&Located> {
        self.located
            .get(&item.id)
            .ok_or_else(|| Error::Other("no longer in the trash".to_string()))
    }

    fn forget(&mut self, item: &Item, row: TrashId) -> Result<()> {
        self.located.remove(&item.id);
        forget_rows(&self.ledger, &[row])
    }
}

impl Store for LedgerStore {
    /// Read the ledger and pair every row with the entry it became inside the
    /// trash directory.
    ///
    /// Rows that cannot be paired are dropped: an item that is not in the trash
    /// any more (emptied from Finder, say) can be neither restored nor purged,
    /// so keeping its row would only make the table grow forever. A trash
    /// directory that does not exist is the one case left alone, because
    /// "empty" and "misconfigured" are indistinguishable from here.
    fn list(&mut self) -> Result<Vec<Item>> {
        // Ordered most recently trashed first, with the row id breaking ties a
        // timestamp cannot.
        let records = self
            .ledger
            .entries()
            .map_err(|error| Error::Other(format!("could not read the trash ledger: {error}")))?;

        let prunable = self.trash_dir.is_dir();
        let entries = read_entries(&self.trash_dir)?;
        let mut claimed: HashSet<&Path> = HashSet::new();
        let mut items = Vec::new();
        let mut lost = Vec::new();
        let mut located = HashMap::new();
        // Claimed *oldest* first, because that is the order the trash named them
        // in: the first arrival keeps the original name and later ones are
        // renamed. So when two rows could take the same entry and the clock is
        // too coarse to separate them, the older row takes the original name —
        // which is what actually happened.
        for record in records.into_iter().rev() {
            match locate(&record.entry, &entries, &claimed) {
                Some(entry) => {
                    claimed.insert(entry.path.as_path());
                    let id = record.id.to_string();
                    located.insert(
                        id.clone(),
                        Located {
                            row: record.id,
                            path: entry.path.clone(),
                        },
                    );
                    items.push(Item {
                        id,
                        original_path: record.entry.original_path,
                        deleted_at_unix: record.entry.trashed_at / NANOS_PER_SECOND,
                        is_dir: record.entry.is_dir,
                    });
                }
                None => lost.push(record.id),
            }
        }
        self.located = located;
        if prunable && !lost.is_empty() {
            // Not silent: an item trashed from another volume lands in that
            // volume's own trash, which this store does not scan, so its row is
            // dropped here even though the item still exists somewhere.
            tracing::warn!(
                rows = lost.len(),
                trash_dir = %self.trash_dir.display(),
                "dropping trash ledger rows whose items are not in this trash directory"
            );
            forget_rows(&self.ledger, &lost)?;
        }
        // Back to newest-first, the order every caller reports in.
        items.reverse();
        Ok(items)
    }

    fn restore(&mut self, item: &Item) -> Result<()> {
        let source = self.source_of(item)?;
        let (row, source) = (source.row, source.path.clone());
        let destination = free_destination(&item.original_path)?;
        // `free_destination` reports an occupied path in the words the CLI
        // wants; this makes the move itself refuse one, so nothing that appears
        // between the two is overwritten.
        ops::move_path_no_replace(&source, &destination).map_err(occupied_destination)?;
        self.forget(item, row)
    }

    fn purge(&mut self, item: &Item) -> Result<()> {
        let source = self.source_of(item)?;
        let (row, source) = (source.row, source.path.clone());
        ops::delete_permanent(&source)?;
        self.forget(item, row)
    }
}

fn forget_rows(ledger: &Arc<dyn TrashLedger>, rows: &[TrashId]) -> Result<()> {
    ledger
        .forget(rows)
        .map(|_| ())
        .map_err(|error| Error::Other(format!("could not update the trash ledger: {error}")))
}

// The same predicate the `trash` crate uses to gate its `os_limited` module.
#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
mod os_store {
    use super::{Error, Item, Result, Store, free_destination};

    /// A [`Store`] backed by the operating system's own trash index, which on
    /// these platforms already knows where every item came from.
    #[derive(Debug, Default)]
    pub struct OsStore {
        /// The entries handed out by the last `list`: restoring and purging need
        /// the OS handle, not our summary of it.
        listed: Vec<trash::TrashItem>,
    }

    impl OsStore {
        fn take(&mut self, item: &Item) -> Result<trash::TrashItem> {
            let index = self
                .listed
                .iter()
                .position(|entry| entry.id.to_string_lossy() == item.id)
                .ok_or_else(|| Error::Other("no longer in the trash".to_string()))?;
            Ok(self.listed.swap_remove(index))
        }
    }

    /// Whether a trashed item is a directory. The OS index does not say
    /// directly, so this asks for the item's size, which the crate reports as an
    /// entry count for directories. An item whose metadata cannot be read is
    /// reported as a file, which only affects how a listing labels it.
    fn is_dir(entry: &trash::TrashItem) -> bool {
        trash::os_limited::metadata(entry)
            .is_ok_and(|metadata| matches!(metadata.size, trash::TrashItemSize::Entries(_)))
    }

    impl Store for OsStore {
        fn list(&mut self) -> Result<Vec<Item>> {
            self.listed = trash::os_limited::list()
                .map_err(|error| Error::Other(format!("could not read the trash: {error}")))?;
            self.listed
                .sort_by_key(|entry| std::cmp::Reverse(entry.time_deleted));
            Ok(self
                .listed
                .iter()
                .map(|entry| Item {
                    id: entry.id.to_string_lossy().into_owned(),
                    original_path: entry.original_path(),
                    deleted_at_unix: entry.time_deleted,
                    is_dir: is_dir(entry),
                })
                .collect())
        }

        fn restore(&mut self, item: &Item) -> Result<()> {
            let entry = self.take(item)?;
            // Refuses an occupied destination and recreates a missing parent
            // exactly as the ledger store does; the move itself belongs to the
            // OS, which clears its own bookkeeping with it.
            free_destination(&item.original_path)?;
            trash::os_limited::restore_all([entry])
                .map_err(|error| Error::Other(format!("could not restore: {error}")))
        }

        fn purge(&mut self, item: &Item) -> Result<()> {
            let entry = self.take(item)?;
            trash::os_limited::purge_all([entry])
                .map_err(|error| Error::Other(format!("could not purge: {error}")))
        }
    }
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
pub use os_store::OsStore;

/// Whether this platform exposes an OS trash index, and so whether a listing
/// covers items nohrs did not trash itself.
///
/// Where it is `true` nothing writes the ledger, because the OS already records
/// the same facts (see [`ops::trash_path`]).
pub const OS_INDEX_AVAILABLE: bool = cfg!(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
));

/// The store for this platform: the OS trash index where there is one, and the
/// nohrs ledger where there is not.
///
/// `open_ledger` is called only in the second case, so a caller on Linux or
/// Windows never pays for opening a database it will not read.
pub fn default_store<F>(open_ledger: F) -> Result<Box<dyn Store>>
where
    F: FnOnce() -> Result<Arc<dyn TrashLedger>>,
{
    #[cfg(any(
        target_os = "windows",
        all(
            unix,
            not(target_os = "macos"),
            not(target_os = "ios"),
            not(target_os = "android")
        )
    ))]
    {
        drop(open_ledger);
        Ok(Box::new(OsStore::default()))
    }
    #[cfg(not(any(
        target_os = "windows",
        all(
            unix,
            not(target_os = "macos"),
            not(target_os = "ios"),
            not(target_os = "android")
        )
    )))]
    {
        Ok(Box::new(LedgerStore::new(
            open_ledger()?,
            home_trash_dir()?,
        )))
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

/// Describe `path` as it is now, for the ledger, before it moves to the trash.
///
/// The location is recorded as an absolute path: the restore will not
/// necessarily run from the working directory the deletion did, so a relative
/// one would put the item back somewhere else entirely. `std::path::absolute`
/// does not resolve symlinks, so a link is recorded as itself rather than as its
/// target.
pub fn capture(path: &Path) -> Result<TrashEntry> {
    let metadata = std::fs::symlink_metadata(path)?;
    let path = std::path::absolute(path)?;
    // The ledger stores paths as text, so a path that is not valid UTF-8 could
    // only be written after mangling it — and a mangled record restores to the
    // wrong name. Refuse rather than corrupt: `ops::trash_path` turns this into
    // a refused delete rather than an unrestorable one. Rare on macOS, where
    // APFS and HFS+ reject invalid UTF-8 outright, but a Unix name is bytes and
    // a mounted exFAT, NFS, or SMB volume can hand one over.
    if path.to_str().is_none() {
        return Err(Error::Other(format!(
            "cannot record a path that is not valid UTF-8: {}",
            path.display()
        )));
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Other(format!("path has no file name: {}", path.display())))?;
    Ok(TrashEntry {
        file_name,
        size: if metadata.is_dir() { 0 } else { metadata.len() },
        modified_ns: metadata.modified().ok().and_then(unix_nanos),
        trashed_at: unix_nanos(SystemTime::now()).unwrap_or_default(),
        is_dir: metadata.is_dir(),
        original_path: path,
    })
}

const NANOS_PER_SECOND: i64 = 1_000_000_000;

/// Nanoseconds since the Unix epoch, negative for the pre-epoch timestamps some
/// filesystems still carry.
fn unix_nanos(time: SystemTime) -> Option<i64> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_nanos()).ok(),
        Err(error) => i64::try_from(error.duration().as_nanos())
            .ok()
            .map(|nanos| -nanos),
    }
}

/// What the CLI says when the original location is taken. The path is not part
/// of it: `noh trash` prefixes every diagnostic with the operand it is about.
const DESTINATION_OCCUPIED: &str =
    "something else is at the original location; move it aside first";

/// Restates a move that lost the race for the destination in the same words
/// [`free_destination`] uses, so the user cannot tell which check caught it.
fn occupied_destination(error: Error) -> Error {
    match &error {
        Error::Io(io) if io.kind() == io::ErrorKind::AlreadyExists => {
            Error::Other(DESTINATION_OCCUPIED.to_string())
        }
        _ => error,
    }
}

/// Where a restore should land: the original path, refusing to overwrite
/// whatever occupies it now and recreating the directory it lived in if that has
/// since been removed.
fn free_destination(original: &Path) -> Result<PathBuf> {
    if ops::would_conflict(original) {
        return Err(Error::Other(DESTINATION_OCCUPIED.to_string()));
    }
    let parent = original
        .parent()
        .ok_or_else(|| Error::Other("cannot restore to a path with no parent".to_string()))?;
    std::fs::create_dir_all(parent)?;
    Ok(original.to_path_buf())
}

/// One entry of a trash directory: as much of it as matching a record needs.
struct Entry {
    path: PathBuf,
    file_name: String,
    is_dir: bool,
    size: u64,
    modified_ns: Option<i64>,
    /// When this entry last changed on disk, which a move into the trash
    /// updates — so it is, in effect, when the item arrived there.
    changed_ns: Option<i64>,
}

fn read_entries(trash_dir: &Path) -> Result<Vec<Entry>> {
    let listing = match std::fs::read_dir(trash_dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::Io(error)),
    };
    let mut entries = Vec::new();
    for entry in listing {
        let entry = entry?;
        let metadata = entry.metadata()?;
        entries.push(Entry {
            path: entry.path(),
            file_name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_ns: metadata.modified().ok().and_then(unix_nanos),
            changed_ns: changed_ns(&metadata),
        });
    }
    Ok(entries)
}

/// Find the trash entry a ledger row became.
///
/// [`score`] narrows the field to entries that *could* be this row — a move
/// preserves size and modification time, and the trash renames an item whose
/// name is already taken (`notes.txt` becomes `notes 2.txt`).
///
/// Among those, the winner is the entry that physically arrived in the trash
/// closest to when the row was written, not the one whose name matches best.
/// The order matters: the trash gives the *first* arrival the original name, so
/// for two copies with identical name, size and modification time, preferring
/// the exact name would hand the newer row the older file — and restore its
/// contents. Arrival time is the only thing that still separates them. Where the
/// platform reports no change time, every gap is the same sentinel and the name
/// decides, as before.
fn locate<'a>(
    entry: &TrashEntry,
    entries: &'a [Entry],
    claimed: &HashSet<&Path>,
) -> Option<&'a Entry> {
    entries
        .iter()
        .filter(|candidate| !claimed.contains(candidate.path.as_path()))
        .filter_map(|candidate| score(entry, candidate).map(|score| (score, candidate)))
        .min_by_key(|(score, candidate)| (arrival_gap(entry, candidate), Reverse(*score)))
        .map(|(_, candidate)| candidate)
}

/// How far a candidate's arrival in the trash sits from when this row was
/// written. `u64::MAX` when the platform reports no change time, which makes
/// every candidate equally distant and leaves the decision to [`score`] — as
/// does a change time too coarse to separate two deletions. Both fall back on
/// the claiming order in [`Store::list`], which is why that runs oldest-first.
fn arrival_gap(entry: &TrashEntry, candidate: &Entry) -> u64 {
    match candidate.changed_ns {
        Some(changed) => changed.saturating_sub(entry.trashed_at).unsigned_abs(),
        None => u64::MAX,
    }
}

/// When a filesystem entry last changed, in nanoseconds since the Unix epoch.
/// A rename updates it, which is what makes it stand in for "when this arrived
/// in the trash".
#[cfg(unix)]
fn changed_ns(metadata: &std::fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    metadata
        .ctime()
        .checked_mul(NANOS_PER_SECOND)
        .map(|seconds| seconds.saturating_add(metadata.ctime_nsec()))
}

#[cfg(not(unix))]
fn changed_ns(_metadata: &std::fs::Metadata) -> Option<i64> {
    None
}

fn score(entry: &TrashEntry, candidate: &Entry) -> Option<u32> {
    if candidate.is_dir != entry.is_dir {
        return None;
    }
    // A file keeps its byte count across a move, so a different size means a
    // different file however similar the name looks.
    if !entry.is_dir && candidate.size != entry.size {
        return None;
    }
    let mut score = if candidate.file_name == entry.file_name {
        4
    } else if renamed_from(&entry.file_name, &candidate.file_name) {
        1
    } else {
        return None;
    };
    if entry.modified_ns.is_some() && candidate.modified_ns == entry.modified_ns {
        score += 2;
    }
    Some(score)
}

/// Whether `candidate` looks like the trash's renaming of `original`.
///
/// The trash only ever appends to the stem, and only in the two forms it uses
/// to break a name collision: a counter (`notes 2.txt`) or the time of day
/// (`notes 10.30.15 AM.txt`). Anything else that merely begins with the same
/// stem — `notes-backup.txt`, `notes_old.txt`, `notes draft.txt` — is a
/// different file, and matching it would put it at risk of being restored over
/// or purged in the deleted file's place.
fn renamed_from(original: &str, candidate: &str) -> bool {
    let original = Path::new(original);
    let candidate = Path::new(candidate);
    if original.extension() != candidate.extension() {
        return false;
    }
    match (
        original.file_stem().and_then(|stem| stem.to_str()),
        candidate.file_stem().and_then(|stem| stem.to_str()),
    ) {
        (Some(original), Some(candidate)) if !original.is_empty() => candidate
            .strip_prefix(original)
            .and_then(|suffix| suffix.strip_prefix(' '))
            .is_some_and(is_collision_suffix),
        _ => false,
    }
}

/// Whether `suffix` is one the trash appends to break a name collision: a
/// counter (`2`) or a time of day (`10.30.15 AM`).
fn is_collision_suffix(suffix: &str) -> bool {
    if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    match suffix.split_once(' ') {
        Some((clock, "AM" | "PM")) => is_clock(clock),
        _ => false,
    }
}

/// Whether `clock` is a wall-clock time the trash could have written: a 12-hour
/// hour, then two-digit minutes and seconds. Checked to the value, not just the
/// shape — `99.99.99 AM` is a name someone chose, not one the trash produced,
/// and treating it as one puts that file at risk of being restored over or
/// purged in another's place.
fn is_clock(clock: &str) -> bool {
    let mut parts = clock.split('.');
    let (Some(hour), Some(minute), Some(second), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    // Parsed by hand rather than with `str::parse`, which also accepts a `+`.
    let value = |part: &str, digits: std::ops::RangeInclusive<usize>| {
        (digits.contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())).then(
            || {
                part.bytes()
                    .fold(0u32, |value, byte| value * 10 + u32::from(byte - b'0'))
            },
        )
    };
    matches!(value(hour, 1..=2), Some(1..=12))
        && matches!(value(minute, 2..=2), Some(0..=59))
        && matches!(value(second, 2..=2), Some(0..=59))
}

#[cfg(test)]
// The fixtures build real trash directories, so they need the synchronous
// filesystem calls that app code routes through this module instead.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use nohrs_store::{SqliteStore, StoreLogConfig};
    use std::fs;
    use tempfile::{TempDir, tempdir};

    /// A trash directory plus the ledger describing it, wired together the way
    /// [`ops::trash_path`] leaves them. The ledger is a real in-memory
    /// `SqliteStore`, so these exercise the code macOS actually runs.
    pub(crate) struct Fixture {
        pub home: TempDir,
        pub ledger: Arc<dyn TrashLedger>,
        pub trash_dir: PathBuf,
    }

    impl Fixture {
        pub fn new() -> Self {
            let home = tempdir().unwrap();
            let trash_dir = home.path().join("Trash");
            fs::create_dir(&trash_dir).unwrap();
            let ledger = SqliteStore::open_in_memory(&StoreLogConfig::default()).unwrap();
            Self {
                home,
                ledger: Arc::new(ledger),
                trash_dir,
            }
        }

        pub fn origin(&self, name: &str) -> PathBuf {
            self.home.path().join("work").join(name)
        }

        /// Create a file at its original location and move it into the trash as
        /// `trash_name`, which differs from `name` when the trash had to rename
        /// it on the way in.
        pub fn trash_file(&self, name: &str, contents: &str, trash_name: &str) -> PathBuf {
            let origin = self.origin(name);
            fs::create_dir_all(origin.parent().unwrap()).unwrap();
            fs::write(&origin, contents).unwrap();
            let entry = capture(&origin).unwrap();
            fs::rename(&origin, self.trash_dir.join(trash_name)).unwrap();
            self.ledger.append(&entry).unwrap();
            origin
        }

        /// The same, for a directory with a file inside it.
        pub fn trash_directory(&self, name: &str) -> PathBuf {
            let origin = self.origin(name);
            fs::create_dir_all(&origin).unwrap();
            fs::write(origin.join("inner.txt"), "inner").unwrap();
            let entry = capture(&origin).unwrap();
            fs::rename(&origin, self.trash_dir.join(name)).unwrap();
            self.ledger.append(&entry).unwrap();
            origin
        }

        pub fn store(&self) -> LedgerStore {
            LedgerStore::new(Arc::clone(&self.ledger), self.trash_dir.clone())
        }
    }

    #[test]
    fn a_trashed_file_is_listed_with_its_original_path() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload", "notes.txt");
        let mut store = fixture.store();

        let items = store.list().unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].original_path, origin);
        assert_eq!(items[0].file_name(), "notes.txt");
        assert!(!items[0].is_dir);
        assert!(items[0].deleted_at_unix > 0, "the timestamp is in seconds");
    }

    #[test]
    fn capture_records_an_absolute_location() {
        // Cargo runs unit tests with the package root as the working directory,
        // so this crate's own manifest is a relative path that exists.
        let entry = capture(Path::new("Cargo.toml")).unwrap();

        assert!(
            entry.original_path.is_absolute(),
            "a relative location would restore to the wrong directory: {}",
            entry.original_path.display()
        );
        assert!(entry.original_path.ends_with("Cargo.toml"));
        assert!(entry.trashed_at > 0);
    }

    #[test]
    fn capture_reports_a_directory_as_one_with_no_size() {
        let fixture = Fixture::new();
        let origin = fixture.origin("project");
        fs::create_dir_all(&origin).unwrap();

        let entry = capture(&origin).unwrap();

        assert!(entry.is_dir);
        assert_eq!(entry.size, 0, "a directory's own byte count is meaningless");
    }

    // Not on macOS: APFS and HFS+ reject an invalid-UTF-8 name outright
    // (`EILSEQ`), so the fixture cannot even be created there. The guard still
    // matters on macOS for paths on a mounted volume that does allow one — that
    // case just cannot be built in a test.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn capture_refuses_a_path_it_could_only_record_by_mangling_it() {
        use std::os::unix::ffi::OsStrExt;

        let fixture = Fixture::new();
        let origin = fixture
            .home
            .path()
            .join(std::ffi::OsStr::from_bytes(b"not-\xff-utf8"));
        fs::write(&origin, "payload").unwrap();

        let error = capture(&origin).unwrap_err().to_string();

        assert!(
            error.contains("not valid UTF-8"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn capture_fails_on_a_missing_path() {
        let fixture = Fixture::new();
        assert!(capture(&fixture.origin("ghost.txt")).is_err());
    }

    #[test]
    fn an_item_renamed_on_the_way_into_the_trash_is_still_found() {
        let fixture = Fixture::new();
        // The trash already held a `notes.txt`, so this one landed beside it
        // under a different name.
        let origin = fixture.trash_file("notes.txt", "payload", "notes 2.txt");
        let mut store = fixture.store();

        let items = store.list().unwrap();
        store.restore(&items[0]).unwrap();

        assert_eq!(fs::read_to_string(&origin).unwrap(), "payload");
    }

    #[test]
    fn items_trashed_in_the_same_instant_stay_in_deletion_order() {
        let fixture = Fixture::new();
        fixture.trash_file("older.txt", "old", "older.txt");
        fixture.trash_file("newer.txt", "new", "newer.txt");
        let mut store = fixture.store();

        let items = store.list().unwrap();

        assert_eq!(
            items[0].file_name(),
            "newer.txt",
            "the ledger's row order has to decide when the clock cannot"
        );
    }

    #[test]
    fn two_rows_never_claim_the_same_trash_entry() {
        let fixture = Fixture::new();
        // The same path trashed twice, but only one file is left in the trash:
        // the older row has nothing to point at.
        fixture.trash_file("notes.txt", "one", "notes.txt");
        fs::remove_file(fixture.trash_dir.join("notes.txt")).unwrap();
        fixture.trash_file("notes.txt", "two", "notes.txt");
        let mut store = fixture.store();

        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn a_row_whose_item_left_the_trash_is_dropped_from_the_ledger() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload", "notes.txt");
        fs::remove_file(fixture.trash_dir.join("notes.txt")).unwrap();
        let mut store = fixture.store();

        assert!(store.list().unwrap().is_empty());
        assert!(
            fixture.ledger.entries().unwrap().is_empty(),
            "an unrestorable row must not stay in the ledger forever"
        );
    }

    #[test]
    fn a_missing_trash_directory_leaves_the_ledger_alone() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload", "notes.txt");
        let mut store = LedgerStore::new(
            Arc::clone(&fixture.ledger),
            fixture.home.path().join("gone"),
        );

        assert!(store.list().unwrap().is_empty());
        assert_eq!(
            fixture.ledger.entries().unwrap().len(),
            1,
            "a trash directory we cannot read is not evidence the item is gone"
        );
    }

    #[test]
    fn restore_puts_a_file_back_and_forgets_it() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload", "notes.txt");
        let mut store = fixture.store();

        let items = store.list().unwrap();
        store.restore(&items[0]).unwrap();

        assert_eq!(fs::read_to_string(&origin).unwrap(), "payload");
        assert!(fixture.ledger.entries().unwrap().is_empty());
        assert!(!fixture.trash_dir.join("notes.txt").exists());
    }

    #[test]
    fn restore_recreates_a_directory_that_was_removed_in_the_meantime() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload", "notes.txt");
        fs::remove_dir_all(origin.parent().unwrap()).unwrap();
        let mut store = fixture.store();

        let items = store.list().unwrap();
        store.restore(&items[0]).unwrap();

        assert!(origin.exists());
    }

    #[test]
    fn restore_refuses_to_overwrite_what_is_there_now() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload", "notes.txt");
        fs::write(&origin, "newer").unwrap();
        let mut store = fixture.store();

        let items = store.list().unwrap();
        let error = store.restore(&items[0]).unwrap_err().to_string();

        assert!(
            error.contains("something else is at the original location"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read_to_string(&origin).unwrap(),
            "newer",
            "the file that is there now must survive"
        );
    }

    #[test]
    fn losing_the_race_for_the_destination_reads_like_finding_it_taken() {
        // The move refuses a destination that appeared after `free_destination`
        // looked. That arrives as a raw `AlreadyExists`, and the user should not
        // have to tell the two checks apart.
        let raced = occupied_destination(Error::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "/work/notes.txt already exists",
        )));
        assert!(raced.to_string().contains(DESTINATION_OCCUPIED), "{raced}");
        assert!(
            !raced.to_string().contains("already exists"),
            "the raw IO error should not reach the user: {raced}"
        );

        // Anything else is passed through untouched.
        let other = occupied_destination(Error::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "denied",
        )));
        assert!(other.to_string().contains("denied"), "{other}");
    }

    #[test]
    fn a_directory_survives_the_round_trip() {
        let fixture = Fixture::new();
        let origin = fixture.trash_directory("project");
        let mut store = fixture.store();

        let items = store.list().unwrap();
        assert!(items[0].is_dir);
        store.restore(&items[0]).unwrap();

        assert_eq!(
            fs::read_to_string(origin.join("inner.txt")).unwrap(),
            "inner"
        );
    }

    #[test]
    fn purge_deletes_from_the_trash_for_good() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload", "notes.txt");
        let mut store = fixture.store();

        let items = store.list().unwrap();
        store.purge(&items[0]).unwrap();

        assert!(!fixture.trash_dir.join("notes.txt").exists());
        assert!(fixture.ledger.entries().unwrap().is_empty());
    }

    #[test]
    fn acting_on_an_item_that_was_never_listed_fails_rather_than_guessing() {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        let item = Item {
            id: "made-up".to_string(),
            original_path: fixture.origin("notes.txt"),
            deleted_at_unix: 0,
            is_dir: false,
        };

        assert!(store.restore(&item).is_err());
        assert!(store.purge(&item).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn two_copies_with_identical_metadata_are_told_apart_by_when_they_arrived() {
        use std::fs::File;
        use std::time::Duration;

        let fixture = Fixture::new();
        let origin = fixture.origin("notes.txt");
        fs::create_dir_all(origin.parent().unwrap()).unwrap();
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);

        // Two copies of the same path, same size, forced to the same
        // modification time. Name, size and mtime are identical, so nothing but
        // the moment each entered the trash can tell the rows apart.
        let trash_copy = |contents: &str, trash_name: &str| {
            fs::write(&origin, contents).unwrap();
            File::options()
                .write(true)
                .open(&origin)
                .unwrap()
                .set_modified(modified)
                .unwrap();
            let entry = capture(&origin).unwrap();
            fs::rename(&origin, fixture.trash_dir.join(trash_name)).unwrap();
            fixture.ledger.append(&entry).unwrap();
        };
        // The trash keeps the original name for the first arrival and renames
        // the second.
        trash_copy("aaa", "notes.txt");
        std::thread::sleep(Duration::from_millis(20));
        trash_copy("bbb", "notes 2.txt");

        let mut store = fixture.store();
        let items = store.list().unwrap();
        assert_eq!(items.len(), 2);
        store.restore(&items[0]).unwrap();

        assert_eq!(
            fs::read_to_string(&origin).unwrap(),
            "bbb",
            "the newest row must restore the copy that went in last, not the one \
             that happens to still hold the original name"
        );
    }

    #[test]
    fn a_file_of_a_different_size_is_never_the_same_item() {
        let fixture = Fixture::new();
        let origin = fixture.origin("notes.txt");
        fs::create_dir_all(origin.parent().unwrap()).unwrap();
        fs::write(&origin, "payload").unwrap();
        let entry = capture(&origin).unwrap();
        fs::remove_file(&origin).unwrap();
        // Something else of the same name, but not our file.
        fs::write(fixture.trash_dir.join("notes.txt"), "a different payload").unwrap();
        fixture.ledger.append(&entry).unwrap();
        let mut store = fixture.store();

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn a_clock_too_coarse_to_separate_arrivals_falls_back_to_the_naming_order() {
        // Two entries the trash renamed on collision, on a filesystem whose
        // change time cannot tell the two deletions apart.
        fn entry(name: &str, changed_ns: Option<i64>) -> Entry {
            Entry {
                path: PathBuf::from("/trash").join(name),
                file_name: name.to_string(),
                is_dir: false,
                size: 7,
                modified_ns: Some(500),
                changed_ns,
            }
        }
        fn row(trashed_at: i64) -> TrashEntry {
            TrashEntry {
                original_path: PathBuf::from("/work/notes.txt"),
                file_name: "notes.txt".to_string(),
                size: 7,
                modified_ns: Some(500),
                trashed_at,
                is_dir: false,
            }
        }
        let entries = [
            entry("notes.txt", Some(1_000)),
            entry("notes 2.txt", Some(1_000)),
        ];

        // `Store::list` claims oldest-first, so replay that order here.
        let mut claimed = HashSet::new();
        let first = locate(&row(900), &entries, &claimed).unwrap();
        assert_eq!(
            first.file_name, "notes.txt",
            "the first arrival is the one that kept the original name"
        );
        claimed.insert(first.path.as_path());
        let second = locate(&row(950), &entries, &claimed).unwrap();
        assert_eq!(second.file_name, "notes 2.txt");
    }

    #[test]
    fn a_renamed_candidate_must_share_the_extension_and_the_stem() {
        assert!(renamed_from("notes.txt", "notes 2.txt"));
        assert!(renamed_from("notes.txt", "notes 10.30.15 AM.txt"));
        assert!(!renamed_from("notes.txt", "notes 2.md"));
        assert!(!renamed_from("notes.txt", "other.txt"));
        assert!(!renamed_from("notes.txt", "notes.txt.bak"));
    }

    #[test]
    fn a_file_that_merely_starts_the_same_is_not_a_renaming() {
        // The trash separates its suffix with a space. Without that check any
        // longer name would match, and `locate` could restore a neighbouring
        // file of the same size over the deleted one.
        assert!(!renamed_from("notes.txt", "notes-backup.txt"));
        assert!(!renamed_from("notes.txt", "notes_old.txt"));
        assert!(!renamed_from("notes.txt", "notes2.txt"));
        // The identical name is `score`'s exact-match branch, not a renaming.
        assert!(!renamed_from("notes.txt", "notes.txt"));
        assert!(!renamed_from("notes.txt", "notes .txt"));
        // A space is not enough either: the suffix has to be one the trash
        // writes, or a file the user named this way is at risk.
        assert!(!renamed_from("notes.txt", "notes backup.txt"));
        assert!(!renamed_from("notes.txt", "notes 2 old.txt"));
    }

    #[test]
    fn only_the_suffixes_the_trash_appends_count_as_a_collision_name() {
        assert!(is_collision_suffix("2"));
        assert!(is_collision_suffix("10"));
        assert!(is_collision_suffix("10.30.15 AM"));
        assert!(is_collision_suffix("1.05.00 PM"));

        assert!(!is_collision_suffix(""));
        assert!(!is_collision_suffix("backup"));
        assert!(!is_collision_suffix("2b"));
        assert!(!is_collision_suffix("10.30 AM"));
        assert!(!is_collision_suffix("10.30.15.20 AM"));
        assert!(!is_collision_suffix("10.30.15 XM"));
        assert!(!is_collision_suffix("10..15 AM"));
    }

    #[test]
    fn a_clock_suffix_has_to_be_a_time_a_clock_could_show() {
        assert!(is_clock("1.05.00"));
        assert!(is_clock("12.59.59"));
        assert!(is_clock("01.05.00"));

        // Digits in the right places are not enough: a name shaped like a time
        // but reading as none is a file someone named that way.
        assert!(!is_clock("99.99.99"));
        assert!(!is_clock("0.30.15"));
        assert!(!is_clock("13.30.15"));
        assert!(!is_clock("10.60.15"));
        assert!(!is_clock("10.30.60"));
        assert!(!is_clock("10.3.15"));
        assert!(!is_clock("10.030.15"));
        assert!(!is_clock("+1.30.15"));
        assert!(is_collision_suffix("10.30.15 PM"));
    }

    #[test]
    fn a_neighbour_of_the_same_size_is_never_restored_in_place_of_the_deletion() {
        // `notes-backup.txt` was in the trash first, so it is the nearer
        // arrival; only the name rules it out.
        let entries = [Entry {
            path: PathBuf::from("/trash/notes-backup.txt"),
            file_name: "notes-backup.txt".to_string(),
            is_dir: false,
            size: 7,
            modified_ns: Some(500),
            changed_ns: Some(1_000),
        }];
        let row = TrashEntry {
            original_path: PathBuf::from("/work/notes.txt"),
            file_name: "notes.txt".to_string(),
            size: 7,
            modified_ns: Some(500),
            trashed_at: 1_000,
            is_dir: false,
        };

        assert!(locate(&row, &entries, &HashSet::new()).is_none());
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

    // On macOS the default store is the ledger one, and building it would need a
    // real database; the OS-index store does no I/O until it is asked to list,
    // so it is safe to construct here.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_default_store_is_the_os_index_where_the_platform_has_one() {
        const { assert!(OS_INDEX_AVAILABLE) };
        let store = default_store(|| panic!("the ledger must not be opened on this platform"));
        assert!(store.is_ok());
    }
}
