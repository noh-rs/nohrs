//! What `noh rm` trashes has to come back, through the real OS trash.
//!
//! This is the one test that crosses every seam the recoverable half of `rm`
//! rests on: `ops::trash_path` hands the item to the platform's trash and, where
//! the platform keeps no index of its own, writes the ledger row; `noh trash
//! list` and `noh restore` read it back through whichever [`trash::Store`] the
//! platform selected. Neither half can be checked in isolation — the unit tests
//! stage a trash directory by hand rather than letting the OS make one — and it
//! is exactly the seam the explorer's Delete now sits on top of: it calls the
//! same `ops::trash_path` with the same ledger, so a break here is a break there.
//!
//! Runs on both platforms and covers what each actually does: the freedesktop
//! trash index on Linux, the nohrs ledger on macOS.
//!
//! Goes through the real binary because that is where the platform decision is
//! made (`ledger::open_if_needed`); calling the library directly would let the
//! test choose the answer it wants to check. `XDG_DATA_HOME` redirects the
//! ledger database, and on Linux the trash with it. macOS resolves its trash
//! through the real home directory whatever the environment says, so there the
//! item does briefly land in the running user's `~/.Trash` — and is restored out
//! of it again, which is the whole assertion.

// `clippy.toml` bans the synchronous `std::fs` helpers so blocking IO does not
// reach the GPUI foreground thread. A test binary has no UI thread, and staging
// a real file on disk is the point here.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::path::Path;
use std::process::Command;

/// A name no real file would have, so a run that fails between trashing and
/// restoring leaves something identifiable behind rather than a plausible
/// `notes.txt` the user has to puzzle over.
const FIXTURE_NAME: &str = "nohrs-trash-round-trip-fixture.txt";

/// Run `noh` with the data and state directories redirected into temporaries.
fn noh(data: &Path, state: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_noh"))
        .args(args)
        .env("XDG_DATA_HOME", data)
        .env("XDG_STATE_HOME", state)
        // The stderr sink honours `RUST_LOG`; an inherited one would change what
        // the child prints and has nothing to do with what is asserted.
        .env_remove("RUST_LOG")
        .output()
        .unwrap()
}

#[test]
fn what_rm_trashes_is_listed_and_comes_back_where_it_was() {
    let data = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let file = work.path().join(FIXTURE_NAME);
    std::fs::write(&file, "payload").unwrap();
    let path = file.to_str().unwrap();

    let removed = noh(data.path(), state.path(), &["rm", path]);
    assert!(removed.status.success(), "rm failed: {removed:?}");
    assert!(!file.exists(), "rm left the file where it was");

    // Where the platform has no trash index, this is the ledger row
    // `ops::trash_path` wrote; where it has one, it is the OS's own record.
    let listed = noh(data.path(), state.path(), &["trash", "list"]);
    assert!(listed.status.success(), "trash list failed: {listed:?}");
    let listing = String::from_utf8(listed.stdout).unwrap();
    assert!(
        listing.contains(FIXTURE_NAME),
        "trashed item is not in the listing: {listing}"
    );

    let restored = noh(data.path(), state.path(), &["restore", path]);
    assert!(restored.status.success(), "restore failed: {restored:?}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "payload",
        "the file came back, but not with what was in it"
    );

    let after = noh(data.path(), state.path(), &["trash", "list"]);
    let listing = String::from_utf8(after.stdout).unwrap();
    assert!(
        !listing.contains(FIXTURE_NAME),
        "a restored item is still in the trash: {listing}"
    );
}
