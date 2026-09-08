//! `noh log` is the *reader* of the log file, so running it must not write one.
//!
//! These go through the real binary rather than `log::Session` because the bug
//! they pin lives in `main`'s ordering — the subscriber is installed before the
//! command runs, so a unit test on `Session` cannot see it. Cargo builds the
//! binary for integration tests and hands us its path in `CARGO_BIN_EXE_noh`.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::path::Path;
use std::process::Command;

/// Run `noh` with `args`, with the log directory redirected into `state`.
fn noh(state: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_noh"))
        .args(args)
        .env("XDG_STATE_HOME", state)
        // The stderr sink honours `RUST_LOG`; an inherited one would change
        // what the child prints and has nothing to do with what is asserted.
        .env_remove("RUST_LOG")
        .output()
        .unwrap()
}

/// Where `nohrs_core::config::paths::log_dir()` lands under `state`.
fn log_dir(state: &Path) -> std::path::PathBuf {
    state.join("nohrs").join("logs")
}

#[test]
fn show_on_a_fresh_install_reports_nothing_and_creates_nothing() {
    let state = tempfile::tempdir().unwrap();

    let output = noh(state.path(), &["log", "show"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("no log files"), "{stdout}");
    assert!(
        !log_dir(state.path()).exists(),
        "reading the log must not create it: the file sink would make `show` \
         report a directory it had just built itself"
    );
}

#[test]
fn clear_removes_the_files_and_leaves_none_behind() {
    let state = tempfile::tempdir().unwrap();
    let directory = log_dir(state.path());
    std::fs::create_dir_all(&directory).unwrap();
    let rotated = directory.join("nohrs.log.2026-09-06");
    std::fs::write(&rotated, "{}\n").unwrap();

    let output = noh(state.path(), &["log", "clear", "--force"]);

    assert!(output.status.success(), "{output:?}");
    assert!(
        !rotated.exists(),
        "a rotated file no writer holds must be unlinked"
    );
    // The bug this guards: with the sink open, `clear` deletes the file this
    // very process is appending to and the appender then writes to an unlinked
    // inode — so a file reappears, or the removal is silently undone.
    let remaining: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        remaining.is_empty(),
        "clear should leave the directory empty, found {remaining:?}"
    );
}

#[test]
fn a_command_that_is_not_log_does_record_itself() {
    let state = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let target = workspace.path().join("notes.txt");
    std::fs::write(&target, "x").unwrap();

    // Permanent so it does not need a desktop trash, which CI has not got.
    let output = noh(
        state.path(),
        &["rm", "--permanent", &target.display().to_string()],
    );

    assert!(output.status.success(), "{output:?}");
    assert!(!target.exists());
    let files: Vec<_> = std::fs::read_dir(log_dir(state.path()))
        .expect("a command that does real work must open the log")
        .filter_map(Result::ok)
        .collect();
    assert!(!files.is_empty(), "the run should have been recorded");
    let body = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(body.contains("fs.delete"), "{body}");
}
