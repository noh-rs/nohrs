use nohrs::core::search_utils::{group_results, results_to_entries};
use nohrs::services::search::SearchResult;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_result(path: &str, line_number: usize, line_content: &str) -> SearchResult {
    SearchResult {
        path: PathBuf::from(path),
        line_number,
        line_content: line_content.to_string(),
        match_start: 0,
        match_end: 0,
    }
}

#[test]
fn test_group_results_same_file_grouped() {
    let results = vec![
        make_result("/tmp/test/lib.rs", 1, "use std::collections::HashMap;"),
        make_result("/tmp/test/lib.rs", 10, "let map = HashMap::new();"),
    ];
    let grouped = group_results(results);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].matches.len(), 2);
    assert_eq!(grouped[0].filename, "lib.rs");
}

#[test]
fn test_group_results_different_files() {
    let results = vec![
        make_result("/tmp/test/a.rs", 3, "fn search() {}"),
        make_result("/tmp/test/b.rs", 7, "fn search_inner() {}"),
    ];
    let grouped = group_results(results);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].filename, "a.rs");
    assert_eq!(grouped[1].filename, "b.rs");
}

#[test]
fn test_group_results_line_number_zero_skipped() {
    let results = vec![
        make_result("/tmp/test/dir", 0, ""),
        make_result("/tmp/test/file.rs", 5, "content"),
    ];
    let grouped = group_results(results);
    // dir (line_number=0) should have no matches
    let dir_result = grouped.iter().find(|r| r.filename == "dir").unwrap();
    assert!(dir_result.matches.is_empty());
    // file should have one match
    let file_result = grouped.iter().find(|r| r.filename == "file.rs").unwrap();
    assert_eq!(file_result.matches.len(), 1);
}

#[test]
fn test_group_results_sorted_by_path() {
    let results = vec![
        make_result("/tmp/c.rs", 1, "c"),
        make_result("/tmp/a.rs", 1, "a"),
        make_result("/tmp/b.rs", 1, "b"),
    ];
    let grouped = group_results(results);
    assert_eq!(grouped[0].path, "/tmp/a.rs");
    assert_eq!(grouped[1].path, "/tmp/b.rs");
    assert_eq!(grouped[2].path, "/tmp/c.rs");
}

#[test]
fn test_group_results_empty() {
    let results: Vec<SearchResult> = vec![];
    let grouped = group_results(results);
    assert!(grouped.is_empty());
}

#[test]
fn test_group_results_folder_extraction() {
    let results = vec![make_result("/home/user/project/src/main.rs", 1, "fn main()")];
    let grouped = group_results(results);
    assert_eq!(grouped[0].folder, "/home/user/project/src");
    assert_eq!(grouped[0].filename, "main.rs");
}

#[test]
fn test_results_to_entries_real_file() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "Hello world content here").unwrap();

    let search_results = vec![nohrs::core::types::SearchFileResult {
        path: file_path.to_string_lossy().to_string(),
        folder: tmp.path().to_string_lossy().to_string(),
        filename: "test.txt".to_string(),
        matches: vec![],
    }];

    let entries = results_to_entries(&search_results);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "file");
    assert!(entries[0].size > 0);
    assert!(entries[0].modified > 0);
}

#[test]
fn test_results_to_entries_nonexistent_file_no_panic() {
    let search_results = vec![nohrs::core::types::SearchFileResult {
        path: "/nonexistent/path/file.txt".to_string(),
        folder: "/nonexistent/path".to_string(),
        filename: "file.txt".to_string(),
        matches: vec![],
    }];

    let entries = results_to_entries(&search_results);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, 0);
    assert_eq!(entries[0].modified, 0);
}
