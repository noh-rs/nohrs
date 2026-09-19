//! `noh search` against a real directory tree, through the real binary.
//!
//! The unit tests in `search.rs` stand a fake engine behind the command so that
//! what it *prints* can be pinned down; nothing there walks a filesystem. This
//! is the other half: the walk, the exclusions it applies, the working
//! directory it defaults to, and the exit codes a shell script keys on.

// `clippy.toml` bans the synchronous `std::fs` helpers so blocking IO does not
// reach the GPUI foreground thread. A test binary has no UI thread, and staging
// a real tree on disk is the point here.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use std::path::Path;
use std::process::{Command, Output};

/// Run `noh search` with `work` as the working directory, so the command
/// resolves its default operand the way a user in that directory would.
fn search(work: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noh"))
        .arg("search")
        .args(args)
        .current_dir(work)
        // The stderr sink honours `RUST_LOG`; an inherited one would change what
        // the child prints and has nothing to do with what is asserted.
        .env_remove("RUST_LOG")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// A tree with one match in plain sight and two that are excluded by default.
fn tree() -> tempfile::TempDir {
    let work = tempfile::tempdir().unwrap();
    write(
        work.path(),
        "src/main.rs",
        "fn main() {\n    let needle = 1;\n}\n",
    );
    write(work.path(), "docs/notes.md", "nothing here\n");
    write(work.path(), ".hidden.txt", "needle\n");
    // `.ignore` rather than `.gitignore`: git's ignore files only take effect
    // inside a repository, and a temporary directory is not one.
    write(work.path(), ".ignore", "generated.txt\n");
    write(work.path(), "generated.txt", "needle\n");
    work
}

#[test]
fn a_content_search_points_at_the_line_it_found() {
    let work = tree();

    let found = search(work.path(), &["needle"]);

    assert_eq!(found.status.code(), Some(0), "search failed: {found:?}");
    assert_eq!(stdout(&found), "src/main.rs:2:    let needle = 1;\n");
}

#[test]
fn hidden_and_ignored_files_join_in_only_when_asked_for() {
    let work = tree();

    let default = stdout(&search(work.path(), &["-l", "needle"]));
    assert_eq!(default, "src/main.rs\n");

    let everything = stdout(&search(
        work.path(),
        &["-l", "--hidden", "--no-ignore", "needle"],
    ));
    let mut paths: Vec<&str> = everything.lines().collect();
    paths.sort_unstable();
    assert_eq!(paths, vec![".hidden.txt", "generated.txt", "src/main.rs"]);
}

#[test]
fn a_query_matches_names_and_contents_at_once() {
    let work = tree();
    // A file whose name matches, and whose text matches on another line.
    write(
        work.path(),
        "src/needle.rs",
        "// nothing\nlet needle = 2;\n",
    );

    let found = stdout(&search(work.path(), &["needle"]));

    // The name match is the bare path, and it comes immediately before that
    // file's own matching lines.
    assert_eq!(
        found,
        "src/needle.rs\nsrc/needle.rs:2:let needle = 2;\nsrc/main.rs:2:    let needle = 1;\n"
    );
}

#[test]
fn name_and_content_narrow_the_query_to_one_of_the_two() {
    let work = tree();

    let by_name = search(work.path(), &["--name", "notes"]);
    assert_eq!(by_name.status.code(), Some(0));
    assert_eq!(stdout(&by_name), "docs/notes.md\n");

    // `notes` appears in no file's text, so the contents alone find nothing.
    let by_content = search(work.path(), &["--content", "notes"]);
    assert_eq!(by_content.status.code(), Some(0));
    assert!(stdout(&by_content).is_empty());
}

#[test]
fn an_operand_narrows_the_search_to_that_subtree() {
    let work = tree();

    let elsewhere = search(work.path(), &["needle", "docs"]);

    // Nothing under `docs` matches, which is an answer rather than a failure.
    assert_eq!(elsewhere.status.code(), Some(0), "{elsewhere:?}");
    assert!(stdout(&elsewhere).is_empty());
    let note = String::from_utf8(elsewhere.stderr).unwrap();
    assert!(note.contains("no matches"), "unexpected stderr: {note}");
}

#[test]
fn a_pattern_the_engine_cannot_build_is_a_failure() {
    let work = tree();

    let broken = search(work.path(), &["needle("]);

    assert_eq!(broken.status.code(), Some(1), "{broken:?}");
    assert!(stdout(&broken).is_empty());
    let complaint = String::from_utf8(broken.stderr).unwrap();
    assert!(
        complaint.contains("is not a valid pattern"),
        "unexpected stderr: {complaint}"
    );

    // Taken literally it is a perfectly good query — and finds nothing here.
    let literal = search(work.path(), &["--fixed-strings", "needle("]);
    assert_eq!(literal.status.code(), Some(0), "{literal:?}");
}

#[test]
fn json_output_is_one_object_per_match() {
    let work = tree();

    let found = search(work.path(), &["--json", "needle"]);

    assert_eq!(found.status.code(), Some(0), "{found:?}");
    let line = stdout(&found);
    let parsed: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(parsed["kind"], "content");
    assert_eq!(parsed["path"], "src/main.rs");
    assert_eq!(parsed["line_number"], 2);
    assert_eq!(parsed["line_content"], "    let needle = 1;");
}
