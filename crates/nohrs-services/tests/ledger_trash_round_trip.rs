//! The ledger half of the trash, through the real OS trash, on both platforms
//! nohrs targets.
//!
//! `ops::trash_path` records where an item came from and then hands it to the
//! OS; [`trash::LedgerStore`] pairs the row back up with what the trash made of
//! the item and moves it home. In production only macOS takes that path, because
//! only macOS keeps no trash index of its own — which would leave the code the
//! explorer's Delete depends on exercised by the macOS CI legs alone. Nothing in
//! it is macOS-specific, so this drives it explicitly on Linux too.
//!
//! Not Windows: `home_trash_dir` resolves the freedesktop layout there while the
//! Recycle Bin is somewhere else entirely, so the ledger row would be written and
//! then never paired with its item. Windows is not a platform nohrs targets, and
//! making that pairing work is separate work from this.
//!
//! The unit tests in `fs::trash` stage a trash directory by hand, so the seam
//! this covers — what `capture` writes being enough for `locate` to find what
//! the OS actually created — is not covered there.
//!
//! Deleting reaches the trash through process-wide state (`XDG_DATA_HOME` on
//! Linux), and `std::env::set_var` is neither safe nor sound next to other
//! tests, so the scenario runs in a child process that is born with the
//! environment it needs: the first test spawns this same binary to run the
//! second. macOS resolves its trash through the real home directory whatever the
//! environment says, so there the fixture does briefly land in the running
//! user's `~/.Trash` — and is restored out of it again, which is the assertion.

// `clippy.toml` bans the synchronous `std::fs` helpers so blocking IO does not
// reach the GPUI foreground thread. A test binary has no UI thread, and staging
// a real file on disk is the point here.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::process::Command;

use nohrs_services::fs::ops;
use nohrs_services::fs::trash::{self, Store};

/// A name no real file would have, so a run that fails between trashing and
/// restoring leaves something identifiable behind rather than a plausible
/// `notes.txt` the user has to puzzle over.
///
/// Unique per run, because on macOS the fixture goes to the real `~/.Trash`: a
/// leftover from an interrupted run is matched by name, size and modification
/// time, so a fixed name would let a later run locate the *stale* copy, pass
/// every assertion, and leave the file it actually trashed behind.
fn fixture_name() -> String {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!(
        "nohrs-ledger-round-trip-fixture-{}-{since_epoch}.txt",
        std::process::id()
    )
}

/// The scenario below, run with the trash and the ledger redirected into a
/// temporary directory.
#[cfg(not(target_os = "windows"))]
#[test]
fn a_ledger_round_trip_survives_the_real_trash() {
    let data = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "the_round_trip",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        // Where the ledger database goes on every platform, and where the trash
        // goes on the platforms that read this.
        .env("XDG_DATA_HOME", data.path())
        .env_remove("RUST_LOG")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the round trip failed in the child process"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
#[ignore = "needs the trash redirected; run by a_ledger_round_trip_survives_the_real_trash"]
fn the_round_trip() {
    let work = tempfile::tempdir().unwrap();
    let file = work.path().join(fixture_name());
    std::fs::write(&file, "payload").unwrap();

    let ledger = trash::open_ledger_at(&trash::ledger_path()).unwrap();
    ops::trash_path(&file, Some(ledger.as_ref())).unwrap();
    assert!(!file.exists(), "the item is still where it was");

    let mut store = trash::LedgerStore::new(ledger, trash::home_trash_dir().unwrap());
    let items = store.list().unwrap();
    assert_eq!(items.len(), 1, "the ledger row did not find its item");
    assert_eq!(
        items[0].original_path, file,
        "the recorded location is not where the item came from"
    );

    store.restore(&items[0]).unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "payload",
        "the file came back, but not with what was in it"
    );
    assert!(
        store.list().unwrap().is_empty(),
        "a restored item is still in the trash"
    );
}
