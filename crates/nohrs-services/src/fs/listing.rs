use nohrs_core::errors::Result;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// `FileEntryDto` is a pure data type and lives in `nohrs-models`. It is
// re-exported here so existing `nohrs_services::fs::listing::FileEntryDto`
// references keep resolving without forcing callers through `nohrs-models`.
pub use nohrs_models::file_entry::FileEntryDto;

/// Parameters for a single directory-listing request.
pub struct ListParams<'a> {
    /// Absolute or relative path of the directory to list.
    pub path: &'a str,
    /// Maximum number of entries to return for this page.
    pub limit: usize,
    /// Opaque cursor (a decimal offset) from a previous `next_cursor`, or `None` for the first page.
    pub cursor: Option<&'a str>,
}

/// One page of directory entries plus a cursor for the next page.
pub struct ListResult {
    /// The entries in this page, sorted case-insensitively by name.
    pub entries: Vec<FileEntryDto>,
    /// Cursor to fetch the following page, or `None` when the directory is exhausted.
    pub next_cursor: Option<String>,
}

/// Lists a directory. The work is synchronous filesystem IO; callers that must
/// keep the UI thread responsive should run it on GPUI's background executor via
/// `cx.background_spawn` (async-runtime.md §2).
pub fn list_dir_sync(params: ListParams<'_>) -> Result<ListResult> {
    list_dir_impl(params.path, params.limit, params.cursor)
}

fn list_dir_impl(path: &str, limit: usize, cursor: Option<&str>) -> Result<ListResult> {
    let dir = Path::new(path);
    let mut names: Vec<(String, PathBuf)> = Vec::new();

    // Read directory entries: collect names and paths only (cheap), then sort by name for stable paging.
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = os_str_to_string(entry.file_name());
        names.push((file_name, entry.path()));
    }
    names.sort_by_key(|a| a.0.to_lowercase());

    let total = names.len();
    let offset = cursor
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
        .min(total);

    let end = (offset + limit).min(total);
    let slice = &names[offset..end];

    let mut entries = Vec::with_capacity(slice.len());
    for (name, path) in slice.iter() {
        let md = fs::symlink_metadata(path);
        let (kind, size, modified) = match md {
            Ok(md) => {
                let modified = md
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if md.file_type().is_dir() {
                    ("dir".to_string(), 0, modified)
                } else if md.file_type().is_file() {
                    ("file".to_string(), md.len(), modified)
                } else if md.file_type().is_symlink() {
                    ("symlink".to_string(), 0, modified)
                } else {
                    ("other".to_string(), 0, modified)
                }
            }
            Err(_) => ("unknown".to_string(), 0, 0),
        };

        entries.push(FileEntryDto {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            kind,
            size,
            modified,
        });
    }

    let next_cursor = if end < total {
        Some(end.to_string())
    } else {
        None
    };

    Ok(ListResult {
        entries,
        next_cursor,
    })
}

fn os_str_to_string(s: impl AsRef<OsStr>) -> String {
    s.as_ref().to_string_lossy().into_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn list(path: &str, limit: usize, cursor: Option<&str>) -> ListResult {
        list_dir_sync(ListParams {
            path,
            limit,
            cursor,
        })
        .unwrap()
    }

    #[test]
    fn empty_directory_yields_no_entries() {
        let dir = tempdir().unwrap();
        let res = list(&dir.path().to_string_lossy(), 100, None);
        assert!(res.entries.is_empty());
        assert!(res.next_cursor.is_none());
    }

    #[test]
    fn lists_files_and_dirs_sorted_case_insensitively() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Banana"), "yellow").unwrap();
        std::fs::write(dir.path().join("apple"), "x").unwrap();
        std::fs::create_dir(dir.path().join("Cherry")).unwrap();

        let res = list(&dir.path().to_string_lossy(), 100, None);
        let names: Vec<_> = res.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["apple", "Banana", "Cherry"]);

        let banana = res.entries.iter().find(|e| e.name == "Banana").unwrap();
        assert_eq!(banana.kind, "file");
        assert_eq!(banana.size, 6);
        let cherry = res.entries.iter().find(|e| e.name == "Cherry").unwrap();
        assert_eq!(cherry.kind, "dir");
        assert_eq!(cherry.size, 0);
    }

    #[test]
    fn hidden_files_are_included() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".secret"), "x").unwrap();
        let res = list(&dir.path().to_string_lossy(), 100, None);
        assert!(res.entries.iter().any(|e| e.name == ".secret"));
    }

    #[test]
    fn paging_uses_limit_and_cursor() {
        let dir = tempdir().unwrap();
        for n in 0..5 {
            std::fs::write(dir.path().join(format!("f{n}")), "x").unwrap();
        }
        let path = dir.path().to_string_lossy().to_string();

        let first = list(&path, 2, None);
        assert_eq!(first.entries.len(), 2);
        assert_eq!(first.next_cursor.as_deref(), Some("2"));

        let second = list(&path, 2, first.next_cursor.as_deref());
        assert_eq!(second.entries.len(), 2);
        assert_eq!(second.next_cursor.as_deref(), Some("4"));

        // A cursor past the end clamps to the total: an empty final page.
        let beyond = list(&path, 2, Some("99"));
        assert!(beyond.entries.is_empty());
        assert!(beyond.next_cursor.is_none());
    }

    #[test]
    fn nonexistent_path_is_an_error() {
        assert!(
            list_dir_sync(ListParams {
                path: "/no/such/dir/here",
                limit: 10,
                cursor: None,
            })
            .is_err()
        );
    }

    #[test]
    fn os_str_to_string_round_trips_utf8() {
        assert_eq!(os_str_to_string("hello.txt"), "hello.txt");
    }
}
