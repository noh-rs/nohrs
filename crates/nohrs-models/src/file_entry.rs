use serde::Serialize;

/// A filesystem entry as produced by the services layer and consumed by the UI.
///
/// Field types are intentionally primitive (`String`, `u64`) so this type stays
/// free of any toolkit or service dependency and can live at the bottom of the
/// dependency graph in `nohrs-models`.
#[derive(Debug, Serialize, Clone)]
pub struct FileEntryDto {
    /// File or directory name (final path component).
    pub name: String,
    /// Full path to the entry.
    pub path: String,
    /// Entry kind as a string (`"file"`, `"dir"`, or `"symlink"`), matching the
    /// values emitted by the listing service.
    pub kind: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modification time as a Unix timestamp in seconds.
    pub modified: u64,
}

/// A filesystem entry with a strongly typed [`FileKind`].
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// File or directory name (final path component).
    pub name: String,
    /// Full path to the entry.
    pub path: String,
    /// Kind of filesystem entry.
    pub kind: FileKind,
    /// Size in bytes.
    pub size: u64,
}

/// The kind of a filesystem entry.
#[derive(Debug, Clone)]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link.
    Link,
}
