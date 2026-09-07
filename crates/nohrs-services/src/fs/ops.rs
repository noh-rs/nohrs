//! Filesystem mutation operations: copy, move, rename, create, trash, and
//! permanent delete, with cross-volume awareness and conflict-name resolution.
//!
//! These functions are synchronous filesystem IO. Callers that must stay
//! responsive during large operations should run them on a background executor
//! (mirroring how `search` offloads work via `cx.background_spawn`). The UI
//! layer is expected to route all mutations through this module rather than
//! calling `std::fs` directly (see `docs/explorer-essentials.md` §8).

use crate::fs::trash;
use nohrs_core::errors::{Error, Result};
use nohrs_store::TrashLedger;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// How a [`move_path`] operation was carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    /// The move stayed on a single filesystem and used `rename(2)`.
    Rename,
    /// Source and destination were on different filesystems, so the move was
    /// performed as a recursive copy followed by deleting the source.
    CrossVolume,
}

/// How a name collision at the destination should be resolved when copying or
/// moving. The resolution itself is applied by the caller; this module only
/// provides the building blocks ([`would_conflict`], [`unique_name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep both items by writing to a non-colliding name (see [`unique_name`]).
    Rename,
    /// Replace the existing destination.
    Overwrite,
    /// Leave the destination untouched and skip this item.
    Skip,
}

/// Returns whether `dst` is already occupied, the condition that triggers
/// conflict resolution before a copy or move.
///
/// Unlike [`Path::exists`], this probes the entry itself rather than following
/// symlinks, so a dangling symlink at `dst` still counts as occupied. An
/// ambiguous error (e.g. a permission failure while stat-ing) is treated
/// conservatively as occupied so the caller surfaces the conflict path rather
/// than silently overwriting.
pub fn would_conflict(dst: &Path) -> bool {
    path_occupied(dst)
}

// Whether a filesystem entry exists at `path`, detecting the entry itself
// (including a broken symlink) rather than following links. Ambiguous errors
// are reported as occupied; only a definitive "not found" is reported as free.
fn path_occupied(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

// Validates that `name` is a single, normal path component — not empty, not `.`
// or `..`, and free of path separators — so child-name inputs cannot escape the
// target directory when joined.
fn ensure_plain_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(component)), None) if component == std::ffi::OsStr::new(name) => {
            Ok(())
        }
        _ => Err(Error::Other(format!("invalid file name: {name:?}"))),
    }
}

/// Returns `true` when `src` and `dst_dir` reside on different filesystems, in
/// which case a move cannot use `rename(2)` and must copy then delete.
///
/// On non-Unix platforms device ids are not consulted, so this conservatively
/// returns `false`; [`move_path`] still detects the cross-device error from
/// `rename` and falls back to copy + delete regardless.
pub fn is_cross_volume(src: &Path, dst_dir: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // `rename(2)` acts on the source directory entry itself, so use the
        // entry's own device (don't follow a symlink). The destination is the
        // directory the entry lands in, so its resolved device is what matters.
        let src_dev = fs::symlink_metadata(src)?.dev();
        let dst_dev = fs::metadata(dst_dir)?.dev();
        Ok(src_dev != dst_dev)
    }
    #[cfg(not(unix))]
    {
        let _ = (src, dst_dir);
        Ok(false)
    }
}

/// Produces a file name within `dir` that does not collide with an existing
/// entry, deriving it from `name` by inserting ` (N)` before the extension
/// (`report.pdf` becomes `report (2).pdf`), trying `N = 2, 3, ...` until a free
/// name is found. Returns `name` unchanged when there is no collision.
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !path_occupied(&dir.join(name)) {
        return name.to_string();
    }
    let path = Path::new(name);
    let extension = path.extension().and_then(|ext| ext.to_str());
    // `file_stem` is `None` only for empty names; fall back to the full name so
    // we never panic and always make progress.
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let mut counter: u32 = 2;
    loop {
        let candidate = match extension {
            Some(extension) => format!("{stem} ({counter}).{extension}"),
            None => format!("{stem} ({counter})"),
        };
        if !path_occupied(&dir.join(&candidate)) {
            return candidate;
        }
        counter += 1;
    }
}

/// Recursively copies `src` (a file or directory) to `dst`, where `dst` is the
/// full destination path rather than its parent directory. Missing parent
/// directories are created; an existing destination file is overwritten.
pub fn copy_path(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(src)?;
    if metadata.is_dir() {
        copy_dir_all(src, dst)
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        Ok(())
    }
}

// Recursively copies the contents of directory `src` into `dst`, creating
// `dst` and any intermediate directories. Symlinks are copied via `fs::copy`
// (following the link) rather than recursed into, to avoid cycles.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Moves `src` to `dst`. Uses `rename(2)` when both reside on the same
/// filesystem; otherwise (a cross-volume move) copies `src` recursively to
/// `dst` and then removes the source, reporting which path was taken.
pub fn move_path(src: &Path, dst: &Path) -> Result<MoveKind> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(MoveKind::Rename),
        Err(error) if is_cross_device(&error) => {
            copy_path(src, dst)?;
            delete_permanent(src)?;
            Ok(MoveKind::CrossVolume)
        }
        Err(error) => Err(Error::Io(error)),
    }
}

// Whether an IO error from `rename` indicates the source and destination are on
// different devices, the signal to fall back to copy + delete.
#[cfg(unix)]
fn is_cross_device(error: &std::io::Error) -> bool {
    // EXDEV ("Invalid cross-device link") is 18 on Linux and macOS.
    error.raw_os_error() == Some(18)
}

#[cfg(windows)]
fn is_cross_device(error: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE.
    error.raw_os_error() == Some(17)
}

#[cfg(not(any(unix, windows)))]
fn is_cross_device(_error: &std::io::Error) -> bool {
    false
}

/// Renames `src` to `new_name` within its current directory, returning the new
/// full path. `new_name` must be a bare file name, not a path with separators.
pub fn rename_in_place(src: &Path, new_name: &str) -> Result<PathBuf> {
    ensure_plain_name(new_name)?;
    let parent = src.parent().ok_or_else(|| {
        Error::Other(format!(
            "cannot rename path without a parent: {}",
            src.display()
        ))
    })?;
    let dst = parent.join(new_name);
    fs::rename(src, &dst)?;
    Ok(dst)
}

/// Creates a new directory named `name` inside `parent`, returning its full
/// path. Fails if a file or directory of that name already exists.
pub fn create_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    ensure_plain_name(name)?;
    let dst = parent.join(name);
    fs::create_dir(&dst)?;
    Ok(dst)
}

/// Moves `path` to the operating system's trash/recycle bin.
///
/// `ledger` receives a record of where the item came from, so it can be restored
/// later. Pass `None` where nothing will read it: on Linux and Windows the OS
/// trash keeps the same facts and [`crate::fs::trash::OsStore`] restores from
/// those, so a second copy would grow without bound with no reader.
/// [`crate::fs::trash::OS_INDEX_AVAILABLE`] is how a caller decides.
pub fn trash_path(path: &Path, ledger: Option<&dyn TrashLedger>) -> Result<()> {
    // Captured before the move, while the item is still at its original
    // location — that is the whole point of the record.
    let captured = ledger.map(|_| trash::capture(path));
    ::trash::delete(path)
        .map_err(|error| Error::Other(format!("failed to move to trash: {error}")))?;
    // The item is already in the trash at this point. A bookkeeping failure
    // must not be reported as a failed delete, but it must not vanish either:
    // without the record, the item cannot be restored on this platform.
    if let (Some(ledger), Some(captured)) = (ledger, captured)
        && let Err(error) = record_trashed(ledger, captured)
    {
        tracing::warn!(
            path = %path.display(),
            %error,
            "could not record the trashed item; it will not appear in `noh trash list`"
        );
    }
    Ok(())
}

fn record_trashed(
    ledger: &dyn TrashLedger,
    captured: Result<nohrs_store::TrashEntry>,
) -> Result<()> {
    ledger
        .append(&captured?)
        .map(|_| ())
        .map_err(|error| Error::Other(format!("could not write the trash ledger: {error}")))
}

/// Permanently deletes `path`, whether it is a file, symlink, or directory
/// tree. This cannot be undone.
pub fn delete_permanent(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn unique_name_returns_input_when_no_collision() {
        let dir = tempdir().unwrap();
        assert_eq!(unique_name(dir.path(), "report.pdf"), "report.pdf");
    }

    #[test]
    fn unique_name_inserts_counter_before_extension() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("report.pdf"), "a").unwrap();
        assert_eq!(unique_name(dir.path(), "report.pdf"), "report (2).pdf");

        fs::write(dir.path().join("report (2).pdf"), "b").unwrap();
        assert_eq!(unique_name(dir.path(), "report.pdf"), "report (3).pdf");
    }

    #[test]
    fn unique_name_handles_extensionless_names() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("folder")).unwrap();
        assert_eq!(unique_name(dir.path(), "folder"), "folder (2)");
    }

    #[test]
    fn would_conflict_reflects_existence() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("x");
        assert!(!would_conflict(&target));
        fs::write(&target, "x").unwrap();
        assert!(would_conflict(&target));
    }

    #[test]
    fn copy_path_copies_a_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        fs::write(&src, "hello").unwrap();
        copy_path(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
        assert!(src.exists(), "source is preserved on copy");
    }

    #[test]
    fn copy_path_copies_a_directory_tree() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("top.txt"), "top").unwrap();
        fs::create_dir(src.join("nested")).unwrap();
        fs::write(src.join("nested").join("deep.txt"), "deep").unwrap();

        let dst = dir.path().join("dst");
        copy_path(&src, &dst).unwrap();

        assert_eq!(fs::read_to_string(dst.join("top.txt")).unwrap(), "top");
        assert_eq!(
            fs::read_to_string(dst.join("nested").join("deep.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn move_path_within_volume_renames_and_removes_source() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("sub").join("a.txt");
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(&src, "payload").unwrap();

        let kind = move_path(&src, &dst).unwrap();
        assert_eq!(kind, MoveKind::Rename);
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "payload");
    }

    #[test]
    fn rename_in_place_changes_name_keeps_dir() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("old.txt");
        fs::write(&src, "x").unwrap();

        let dst = rename_in_place(&src, "new.txt").unwrap();
        assert_eq!(dst, dir.path().join("new.txt"));
        assert!(!src.exists());
        assert!(dst.exists());
    }

    #[test]
    fn create_dir_makes_a_new_directory() {
        let dir = tempdir().unwrap();
        let created = create_dir(dir.path(), "fresh").unwrap();
        assert_eq!(created, dir.path().join("fresh"));
        assert!(created.is_dir());
    }

    #[test]
    fn delete_permanent_removes_file_and_tree() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("f.txt");
        fs::write(&file, "x").unwrap();
        delete_permanent(&file).unwrap();
        assert!(!file.exists());

        let tree = dir.path().join("tree");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("inner.txt"), "y").unwrap();
        delete_permanent(&tree).unwrap();
        assert!(!tree.exists());
    }

    #[test]
    fn would_conflict_detects_a_dangling_symlink() {
        let dir = tempdir().unwrap();
        let link = dir.path().join("broken");
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("missing-target"), &link).unwrap();
        #[cfg(not(unix))]
        std::fs::write(&link, "x").unwrap();
        // `Path::exists()` would report `false` for a dangling symlink; the
        // entry is nonetheless occupied.
        assert!(would_conflict(&link));
    }

    #[test]
    fn rename_and_create_reject_path_traversal_names() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("f.txt");
        fs::write(&src, "x").unwrap();
        for bad in ["../escape", "a/b", ".", "..", ""] {
            assert!(
                rename_in_place(&src, bad).is_err(),
                "rename allowed {bad:?}"
            );
            assert!(
                create_dir(dir.path(), bad).is_err(),
                "create allowed {bad:?}"
            );
        }
        // A plain name is still accepted.
        assert!(create_dir(dir.path(), "ok-dir").is_ok());
    }

    #[test]
    fn is_cross_volume_false_within_same_dir() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, "x").unwrap();
        assert!(!is_cross_volume(&src, dir.path()).unwrap());
    }
}
