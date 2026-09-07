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

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

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
        // Already ordered most recently trashed first, with the row id breaking
        // ties a timestamp cannot.
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
        for record in records {
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
        if prunable {
            forget_rows(&self.ledger, &lost)?;
        }
        Ok(items)
    }

    fn restore(&mut self, item: &Item) -> Result<()> {
        let source = self.source_of(item)?;
        let (row, source) = (source.row, source.path.clone());
        let destination = free_destination(&item.original_path)?;
        ops::move_path(&source, &destination)?;
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

/// Where a restore should land: the original path, refusing to overwrite
/// whatever occupies it now and recreating the directory it lived in if that has
/// since been removed.
fn free_destination(original: &Path) -> Result<PathBuf> {
    if ops::would_conflict(original) {
        // The path, not the message, is the caller's to report: `noh trash`
        // prefixes every diagnostic with the operand it is about.
        return Err(Error::Other(
            "something else is at the original location; move it aside first".to_string(),
        ));
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
        });
    }
    Ok(entries)
}

/// Find the trash entry a ledger row became.
///
/// The trash renames an item whose name is already taken (`notes.txt` becomes
/// `notes 2.txt`), so an exact name match wins and a renamed candidate is
/// accepted only when the rest of the metadata agrees. A move preserves size and
/// modification time, which is what makes this reliable enough to restore from.
fn locate<'a>(
    entry: &TrashEntry,
    entries: &'a [Entry],
    claimed: &HashSet<&Path>,
) -> Option<&'a Entry> {
    entries
        .iter()
        .filter(|candidate| !claimed.contains(candidate.path.as_path()))
        .filter_map(|candidate| score(entry, candidate).map(|score| (score, candidate)))
        .max_by_key(|(score, candidate)| (*score, candidate.modified_ns))
        .map(|(_, candidate)| candidate)
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

/// Whether `candidate` looks like the trash's renaming of `original`: the same
/// extension, and a stem the original's is a prefix of.
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
        (Some(original), Some(candidate)) => {
            !original.is_empty() && candidate.starts_with(original)
        }
        _ => false,
    }
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
    fn a_renamed_candidate_must_share_the_extension_and_the_stem() {
        assert!(renamed_from("notes.txt", "notes 2.txt"));
        assert!(renamed_from("notes.txt", "notes 10.30.15 AM.txt"));
        assert!(!renamed_from("notes.txt", "notes 2.md"));
        assert!(!renamed_from("notes.txt", "other.txt"));
        assert!(!renamed_from("notes.txt", "notes.txt.bak"));
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
