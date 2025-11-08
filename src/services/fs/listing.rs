use crate::core::errors::Result;
use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;
use std::time::UNIX_EPOCH;

#[derive(Debug, Serialize, Clone)]
pub struct FileEntryDto {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified: u64,
}

pub struct ListParams<'a> {
    pub path: &'a str,
    pub limit: usize,
    pub cursor: Option<&'a str>,
}

pub struct ListResult {
    pub entries: Vec<FileEntryDto>,
    pub next_cursor: Option<String>,
}

pub async fn list_dir(params: ListParams<'_>) -> Result<ListResult> {
    // Use a blocking task for filesystem IO to avoid blocking async executors.
    let path = params.path.to_string();
    let limit = params.limit;
    let cursor = params.cursor.map(|s| s.to_string());

    task::spawn_blocking(move || list_dir_impl(&path, limit, cursor.as_deref()))
        .await
        .unwrap()
}

/// Synchronous variant for UI contexts where an async runtime is not available.
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
    names.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

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
                let modified = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
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
