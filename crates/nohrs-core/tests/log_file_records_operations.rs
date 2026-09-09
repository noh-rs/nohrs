//! The contract between `#[tracing::instrument]` and the log file.
//!
//! `noh perf` aggregates by operation name and duration, both of which come
//! from the span-close records the file layer writes. That only works if the
//! JSON carries the span's *name and fields* alongside `time.busy` — a duration
//! attached to nothing cannot be attributed to a search or a delete. Formatter
//! settings decide that, and they are easy to change without noticing, so this
//! pins the shape end to end rather than trusting the configuration.
//!
//! An integration test rather than a unit test because installing a global
//! subscriber is once per process, and this needs a process to itself.

// Reads back the file the appender wrote, which is the whole assertion.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use nohrs_core::telemetry::logging::{FileLogConfig, init_logging_with_file};

// Deliberately shaped exactly like a real operation: the shared target, and
// `debug`, which is below the `info` the file's filter starts from. If the
// default filter stopped admitting the operation target, this would record
// nothing and the test would fail — which is the point.
#[tracing::instrument(target = "nohrs::op", name = "search.query", level = "debug", skip_all, fields(query = query))]
fn fake_search(query: &str) -> usize {
    std::thread::sleep(std::time::Duration::from_millis(12));
    query.len()
}

#[test]
fn an_instrumented_operation_lands_in_the_file_with_its_name_and_duration() {
    let directory = tempfile::tempdir().unwrap();
    let logs = directory.path().join("logs");
    // The default config, not a hand-tuned one: what a user gets out of the box
    // has to be what records operations.
    let guard = init_logging_with_file(&FileLogConfig {
        directory: logs.clone(),
        max_files: Some(2),
        ..FileLogConfig::default()
    });

    fake_search("hello");
    // The writer thread is what actually puts bytes on disk, and dropping the
    // guard is what flushes it.
    drop(guard);

    let file = std::fs::read_dir(&logs)
        .unwrap()
        .next()
        .expect("the appender should have created a file")
        .unwrap();
    let body = std::fs::read_to_string(file.path()).unwrap();
    let record: serde_json::Value = body
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["message"] == "close")
        .expect("a span-close record");

    assert_eq!(
        record["span"]["name"], "search.query",
        "without the name, `noh perf` cannot say which operation was slow: {record}"
    );
    assert_eq!(
        record["span"]["query"], "hello",
        "instrumented fields have to survive to the file: {record}"
    );
    // Rendered by the formatter as a human duration ("12.2ms", "1.5s", "19µs"),
    // not a number — whatever reads this has to parse the unit.
    let busy = record["time.busy"]
        .as_str()
        .expect("a close record carries how long the span was busy");
    assert!(
        busy.ends_with("ms") || busy.ends_with('s') || busy.ends_with("µs"),
        "unexpected duration format: {busy}"
    );
}
