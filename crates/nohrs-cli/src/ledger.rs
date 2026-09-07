//! Opening the trash ledger the CLI writes to and restores from.
//!
//! The ledger is the `trash` table of the nohrs metadata database
//! (`$XDG_DATA_HOME/nohrs/db.sqlite`, see `docs/persistence.md` §2). It is only
//! consulted where the operating system keeps no trash index of its own — macOS
//! — so on every other platform the database is never even opened.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nohrs_core::config::paths;
use nohrs_core::errors::{Error, Result};
use nohrs_services::fs::trash;
use nohrs_store::{SqliteStore, StoreLogConfig, TrashLedger};

/// File name of the metadata database inside the nohrs data directory.
const DATABASE_FILE: &str = "db.sqlite";

/// Where the ledger lives, whether or not it exists yet. Reading this creates
/// nothing.
pub fn path() -> PathBuf {
    paths::data_dir().join(DATABASE_FILE)
}

/// Whether restores on this platform go through the ledger rather than an OS
/// trash index.
pub fn required() -> bool {
    !trash::OS_INDEX_AVAILABLE
}

/// Open the ledger, creating the data directory and the database if needed.
pub fn open() -> Result<Arc<dyn TrashLedger>> {
    open_at(&path())
}

/// Open the ledger held in the database at `path`, creating its directory and
/// running any pending migrations.
pub fn open_at(path: &Path) -> Result<Arc<dyn TrashLedger>> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    let store = SqliteStore::open(path, &StoreLogConfig::default())
        .map_err(|error| Error::Other(format!("{}: {error}", path.display())))?;
    Ok(Arc::new(store))
}

/// The ledger, or `None` where this platform does not use one — in which case
/// nothing is opened and no data directory is created.
pub fn open_if_needed() -> Result<Option<Arc<dyn TrashLedger>>> {
    if required() {
        open().map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use nohrs_store::TrashEntry;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn the_ledger_lives_in_the_metadata_database() {
        assert!(path().ends_with("nohrs/db.sqlite"), "{}", path().display());
    }

    #[test]
    fn opening_creates_the_database_and_its_directory() {
        let directory = tempdir().unwrap();
        // A directory that does not exist yet, the state of a first run.
        let path = directory.path().join("data").join("db.sqlite");

        let ledger = open_at(&path).unwrap();

        assert!(path.is_file());
        assert!(ledger.entries().unwrap().is_empty());
        ledger
            .append(&TrashEntry {
                original_path: directory.path().join("notes.txt"),
                file_name: "notes.txt".to_string(),
                size: 7,
                modified_ns: None,
                trashed_at: 1,
                is_dir: false,
            })
            .unwrap();
        // Reopening reads back what the first handle wrote, which is what makes
        // a restore in a later process possible at all.
        assert_eq!(open_at(&path).unwrap().entries().unwrap().len(), 1);
    }

    #[test]
    fn a_path_that_cannot_hold_a_database_is_an_error() {
        let directory = tempdir().unwrap();
        // A directory where the database file should be.
        let path = directory.path().join("occupied");
        std::fs::create_dir(&path).unwrap();

        assert!(open_at(&path).is_err());
    }

    #[test]
    fn nothing_is_opened_where_the_os_keeps_its_own_index() {
        // Guards the property the rest of the CLI relies on: on Linux and
        // Windows `noh rm` must not create a database it will never read.
        assert_eq!(required(), !trash::OS_INDEX_AVAILABLE);
        if !required() {
            assert!(open_if_needed().unwrap().is_none());
        }
    }
}
