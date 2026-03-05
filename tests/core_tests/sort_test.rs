use nohrs::core::sort::{get_extension, sort_entries};
use nohrs::core::types::SortKey;
use nohrs::services::fs::listing::FileEntryDto;

fn make_entry(name: &str, kind: &str, size: u64, modified: u64) -> FileEntryDto {
    FileEntryDto {
        name: name.to_string(),
        path: format!("/tmp/{}", name),
        kind: kind.to_string(),
        size,
        modified,
    }
}

#[test]
fn test_sort_entries_name_asc() {
    let mut entries = vec![
        make_entry("charlie.txt", "file", 100, 1000),
        make_entry("alpha.txt", "file", 200, 2000),
        make_entry("bravo.txt", "file", 150, 1500),
    ];
    sort_entries(&mut entries, SortKey::Name, true);
    assert_eq!(entries[0].name, "alpha.txt");
    assert_eq!(entries[1].name, "bravo.txt");
    assert_eq!(entries[2].name, "charlie.txt");
}

#[test]
fn test_sort_entries_name_desc() {
    let mut entries = vec![
        make_entry("alpha.txt", "file", 200, 2000),
        make_entry("charlie.txt", "file", 100, 1000),
        make_entry("bravo.txt", "file", 150, 1500),
    ];
    sort_entries(&mut entries, SortKey::Name, false);
    assert_eq!(entries[0].name, "charlie.txt");
    assert_eq!(entries[1].name, "bravo.txt");
    assert_eq!(entries[2].name, "alpha.txt");
}

#[test]
fn test_sort_entries_size() {
    let mut entries = vec![
        make_entry("big.txt", "file", 300, 1000),
        make_entry("small.txt", "file", 50, 1000),
        make_entry("medium.txt", "file", 150, 1000),
    ];
    sort_entries(&mut entries, SortKey::Size, true);
    assert_eq!(entries[0].name, "small.txt");
    assert_eq!(entries[1].name, "medium.txt");
    assert_eq!(entries[2].name, "big.txt");
}

#[test]
fn test_sort_entries_modified() {
    let mut entries = vec![
        make_entry("old.txt", "file", 100, 1000),
        make_entry("new.txt", "file", 100, 3000),
        make_entry("mid.txt", "file", 100, 2000),
    ];
    sort_entries(&mut entries, SortKey::Modified, true);
    assert_eq!(entries[0].name, "old.txt");
    assert_eq!(entries[1].name, "mid.txt");
    assert_eq!(entries[2].name, "new.txt");
}

#[test]
fn test_sort_entries_type() {
    let mut entries = vec![
        make_entry("file.rs", "file", 100, 1000),
        make_entry("file.txt", "file", 100, 1000),
        make_entry("file.md", "file", 100, 1000),
    ];
    sort_entries(&mut entries, SortKey::Type, true);
    assert_eq!(entries[0].name, "file.md");
    assert_eq!(entries[1].name, "file.rs");
    assert_eq!(entries[2].name, "file.txt");
}

#[test]
fn test_sort_entries_directories_always_first() {
    let mut entries = vec![
        make_entry("z_file.txt", "file", 100, 1000),
        make_entry("a_dir", "dir", 0, 1000),
        make_entry("a_file.txt", "file", 100, 1000),
        make_entry("z_dir", "dir", 0, 1000),
    ];
    sort_entries(&mut entries, SortKey::Name, true);
    assert_eq!(entries[0].kind, "dir");
    assert_eq!(entries[1].kind, "dir");
    assert_eq!(entries[2].kind, "file");
    assert_eq!(entries[3].kind, "file");
    // Dirs sorted among themselves
    assert_eq!(entries[0].name, "a_dir");
    assert_eq!(entries[1].name, "z_dir");
}

#[test]
fn test_sort_entries_directories_first_desc() {
    let mut entries = vec![
        make_entry("z_file.txt", "file", 100, 1000),
        make_entry("a_dir", "dir", 0, 1000),
        make_entry("a_file.txt", "file", 100, 1000),
        make_entry("z_dir", "dir", 0, 1000),
    ];
    sort_entries(&mut entries, SortKey::Name, false);
    // Directories still come first even in desc
    assert_eq!(entries[0].kind, "dir");
    assert_eq!(entries[1].kind, "dir");
    // But dirs are sorted in reverse
    assert_eq!(entries[0].name, "z_dir");
    assert_eq!(entries[1].name, "a_dir");
}

#[test]
fn test_sort_entries_empty() {
    let mut entries: Vec<FileEntryDto> = vec![];
    sort_entries(&mut entries, SortKey::Name, true);
    assert!(entries.is_empty());
}

#[test]
fn test_get_extension_file() {
    assert_eq!(get_extension("test.rs", "file"), "rs");
    assert_eq!(get_extension("test.TXT", "file"), "txt");
    assert_eq!(get_extension("archive.tar.gz", "file"), "gz");
}

#[test]
fn test_get_extension_dir() {
    assert_eq!(get_extension("my_dir", "dir"), "0_dir");
    assert_eq!(get_extension("dir.with.dots", "dir"), "0_dir");
}

#[test]
fn test_get_extension_no_ext() {
    assert_eq!(get_extension("Makefile", "file"), "zzz_noext");
    assert_eq!(get_extension("LICENSE", "file"), "zzz_noext");
}

#[test]
fn test_get_extension_other_kind() {
    assert_eq!(get_extension("link", "symlink"), "symlink");
    assert_eq!(get_extension("special", "other"), "other");
}
