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
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// How a [`move_path`] operation was carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    /// The move stayed on a single filesystem and used `rename(2)`.
    Rename,
    /// The move was carried out as a recursive copy followed by deleting the
    /// source: source and destination are on different filesystems, or — for
    /// [`move_path_no_replace`] — no no-replace rename was available.
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
fn ensure_plain_name(name: &OsStr) -> Result<()> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(component)), None) if component == name => Ok(()),
        _ => Err(Error::Other(format!("invalid file name: {name:?}"))),
    }
}

/// The path `name` would take inside `dir`, refusing a `name` that is anything
/// but a single plain component.
///
/// Destination paths are assembled from names the caller did not choose — the
/// basename of a clipboard entry, of a dropped path — and joining one blindly is
/// how `..` reaches a directory the user never pointed at. Callers that build a
/// destination inside a directory should come through here rather than calling
/// [`Path::join`], the way [`rename_in_place`] and [`create_dir`] already do.
pub fn destination_in(dir: &Path, name: &OsStr) -> Result<PathBuf> {
    ensure_plain_name(name)?;
    Ok(dir.join(name))
}

/// Refuses an operation whose destination is the source itself or lies inside
/// it.
///
/// Copying a directory into its own subtree walks into the copy it is writing
/// and only stops when the disk is full; copying a file onto itself truncates it
/// before a byte is read. Enforced here rather than in the explorer so every
/// caller — paste today, drag and drop later — is covered by one check.
fn ensure_destination_outside_source(src: &Path, dst: &Path) -> Result<()> {
    let refuse = || {
        Err(Error::Other(format!(
            "cannot copy or move {} into itself: {}",
            src.display(),
            dst.display()
        )))
    };
    // Names can differ while the file does not: a hard link is a second name for
    // the same inode, and a symbolic link resolves to one. Either way a replacing
    // copy opens the destination truncating, which empties the source before a
    // byte of it is read.
    if is_same_file(src, dst) {
        return refuse();
    }
    if destination_path(dst).starts_with(source_path(src)) {
        return refuse();
    }
    Ok(())
}

// Whether two paths lead to one file. Both are resolved through links, because
// that is what the copy itself does.
#[cfg(unix)]
fn is_same_file(one: &Path, other: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(one), fs::metadata(other)) {
        (Ok(one), Ok(other)) => one.dev() == other.dev() && one.ino() == other.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn is_same_file(_one: &Path, _other: &Path) -> bool {
    false
}

// Where the source sits, with its own final component left unresolved: a copy
// recreates a symbolic link rather than following it, so copying a link *into
// the directory it points at* is not copying something into itself, and
// resolving the link would make it look like it was.
fn source_path(src: &Path) -> PathBuf {
    containment_path(src)
}

// Where a write to `dst` would actually land. Unlike the source, a symbolic link
// already sitting there is followed, because `create_dir_all` and `fs::copy`
// follow it too: a link pointing back into the source is a write into the
// source, however unrelated the path it is spelled with looks.
fn destination_path(dst: &Path) -> PathBuf {
    fs::canonicalize(dst).unwrap_or_else(|_| containment_path(dst))
}

// The path to compare containment with: every leading component that exists is
// resolved, so neither `..` nor an intermediate symlink can hide that one path
// is inside the other, and the components that do not exist yet (a destination
// is usually one of them) are appended as written.
fn containment_path(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut trailing = Vec::new();
    let mut current = absolute.as_path();
    loop {
        // `file_name` is `None` exactly where there is no final component to hold
        // back — a path ending in `..`, or the root. Resolving the whole thing is
        // then both safe and necessary: `dir/src/..` *is* `dir`, and left as
        // written it compares as something below `dir/src`, which is how a copy
        // of `dir` into its own child would slip past the guard.
        let Some(name) = current.file_name() else {
            if let Ok(resolved) = fs::canonicalize(current) {
                return with_trailing(resolved, &trailing);
            }
            break;
        };
        let Some(parent) = current.parent() else {
            break;
        };
        trailing.push(name);
        if let Ok(resolved) = fs::canonicalize(parent) {
            return with_trailing(resolved, &trailing);
        }
        current = parent;
    }
    absolute
}

fn with_trailing(mut resolved: PathBuf, trailing: &[&OsStr]) -> PathBuf {
    for name in trailing.iter().rev() {
        resolved.push(name);
    }
    resolved
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

/// Recursively copies `src` (a file, symbolic link, or directory) to `dst`,
/// where `dst` is the full destination path rather than its parent directory.
/// Missing parent directories are created; an existing destination is
/// overwritten. Fails if `dst` is `src` or sits inside it.
#[tracing::instrument(target = "nohrs::op", name = "fs.copy", level = "debug", skip_all, fields(src = %src.display(), dst = %dst.display()))]
pub fn copy_path(src: &Path, dst: &Path) -> Result<()> {
    ensure_destination_outside_source(src, dst)?;
    // Before the parent is created, so a copy of something that is not there
    // leaves no empty directory behind.
    let metadata = fs::symlink_metadata(src)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if metadata.is_symlink() {
        copy_symlink(src, dst, Existing::Replace)
    } else if metadata.is_dir() {
        copy_dir_all(src, dst, Existing::Replace)
    } else {
        fs::copy(src, dst)?;
        Ok(())
    }
}

/// What a recursive copy does about an entry already sitting at a destination
/// path.
///
/// Symbolic links are recreated as links under either, never followed. Following
/// one was what made copying an ordinary folder fail: a link to a directory is
/// not itself a directory, so it went to `fs::copy`, which opened the directory
/// behind it and gave up with `EISDIR` partway through the copy. Recreating also
/// settles the two questions following raises — a link to an ancestor would
/// recurse forever, and one pointing out of the tree would pull in data the user
/// never asked to copy — by never reading through one. It is what `cp -R` and
/// the Finder do with a link inside a folder they copy.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Existing {
    /// Replace it. [`copy_path`] is documented to overwrite, and the
    /// cross-volume half of [`move_path`] has to land where `rename(2)` would
    /// have.
    Replace,
    /// Refuse it, the guarantee [`move_path_no_replace`] rests on: every
    /// destination is claimed by a call that fails if the name is taken.
    Refuse,
}

// Recreates the symbolic link `from` at `to`.
fn copy_symlink(from: &Path, to: &Path, existing: Existing) -> Result<()> {
    let target = fs::read_link(from)?;
    // A directory is left alone: removing one to make room for a link would
    // discard whatever it holds, which is more than an overwrite was asked to do.
    if existing == Existing::Replace
        && let Ok(metadata) = fs::symlink_metadata(to)
        && !metadata.is_dir()
    {
        fs::remove_file(to)?;
    }
    symlink_no_replace(&target, to)
}

// Copies the regular file `from` to `to`.
fn copy_file(from: &Path, to: &Path, existing: Existing) -> Result<()> {
    if existing == Existing::Replace {
        fs::copy(from, to)?;
        return Ok(());
    }
    let mut source = fs::File::open(from)?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    std::io::copy(&mut source, &mut destination)?;
    // After the contents, so a read-only mode cannot stop the copy that has to
    // happen first.
    fs::set_permissions(to, fs::metadata(from)?.permissions())?;
    Ok(())
}

// Recursively copies the contents of directory `src` into `dst`, creating `dst`
// itself. Under `Refuse` every entry is claimed by a call that fails if the name
// is taken — not just the root: a tree takes time to write, and the guarantee is
// worth nothing if something else can create an entry inside it meanwhile and
// have it silently replaced.
fn copy_dir_all(src: &Path, dst: &Path, existing: Existing) -> Result<()> {
    match existing {
        // `create_dir_all` so intermediate directories a caller left out are
        // created, matching what `copy_path` documents.
        Existing::Replace => fs::create_dir_all(dst)?,
        // Refuses an existing directory, and a symbolic link standing in for one,
        // which `create_dir_all` would have followed.
        Existing::Refuse => fs::create_dir(dst)?,
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_symlink() {
            copy_symlink(&from, &to, existing)?;
        } else if file_type.is_dir() {
            copy_dir_all(&from, &to, existing)?;
        } else {
            copy_file(&from, &to, existing)?;
        }
    }
    // `create_dir_all` derives the mode from the umask, so a private `0700`
    // directory would otherwise arrive world-readable and expose what it holds.
    // Applied last: a directory the source made unwritable must not become
    // unwritable here until everything is inside it.
    fs::set_permissions(dst, fs::metadata(src)?.permissions())?;
    Ok(())
}

/// Moves `src` to `dst`. Uses `rename(2)` when both reside on the same
/// filesystem; otherwise (a cross-volume move) copies `src` recursively to
/// `dst` and then removes the source, reporting which path was taken. Fails if
/// `dst` is `src` or sits inside it.
#[tracing::instrument(target = "nohrs::op", name = "fs.move", level = "debug", skip_all, fields(src = %src.display(), dst = %dst.display()))]
pub fn move_path(src: &Path, dst: &Path) -> Result<MoveKind> {
    ensure_destination_outside_source(src, dst)?;
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

/// Moves `src` to `dst` like [`move_path`], but fails instead of replacing
/// anything that is already at `dst`.
///
/// Checking with [`would_conflict`] and then calling [`move_path`] is not the
/// same thing: `rename(2)` replaces, so a file created in between is silently
/// destroyed. Restoring from the trash writes to a path the user last saw empty
/// minutes or days ago, which is exactly when something else may have taken it.
///
/// On Linux and macOS the rename itself carries the no-replace flag, so the
/// whole move is atomic against a concurrent create. Elsewhere — and on
/// filesystems that reject the flag — it falls back to [`copy_path_no_replace`]
/// plus deleting the source, which claims the destination atomically too; a
/// check followed by a replacing `rename` would not, which is the very hole this
/// function exists to close.
pub fn move_path_no_replace(src: &Path, dst: &Path) -> Result<MoveKind> {
    ensure_destination_outside_source(src, dst)?;
    match rename_no_replace(src, dst) {
        Some(Ok(())) => return Ok(MoveKind::Rename),
        Some(Err(error)) if !is_cross_device(&error) => return Err(Error::Io(error)),
        // Cross-device, or no no-replace rename to be had: copy and delete.
        Some(Err(_)) | None => {}
    }
    copy_path_no_replace(src, dst)?;
    delete_permanent(src)?;
    Ok(MoveKind::CrossVolume)
}

/// `rename(2)` with the platform's no-replace flag, or `None` where the
/// platform or the filesystem does not offer one and the caller must fall back.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_no_replace(src: &Path, dst: &Path) -> Option<std::io::Result<()>> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    match renameat_with(CWD, src, CWD, dst, RenameFlags::NOREPLACE) {
        Ok(()) => Some(Ok(())),
        // The flag reaches the filesystem driver, and not every driver
        // implements it: overlayfs and some network mounts report `EINVAL`,
        // a pre-4.9 kernel `ENOSYS`, a non-APFS macOS volume `ENOTSUP`.
        Err(rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS | rustix::io::Errno::NOTSUP) => {
            None
        }
        Err(error) => Some(Err(std::io::Error::from(error))),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_no_replace(_src: &Path, _dst: &Path) -> Option<std::io::Result<()>> {
    None
}

/// Copies `src` to `dst` like [`copy_path`], but claims the destination rather
/// than writing over whatever is there.
///
/// Every branch takes `dst` with a call that fails if the name is taken —
/// `symlink`, `create_dir`, `create_new` — so nothing that appeared since the
/// caller looked is destroyed. Checking with [`would_conflict`] and then calling
/// [`copy_path`] is not the same thing: `fs::copy` opens the destination
/// truncating, so a file created in between loses its contents before a byte is
/// read. Permissions are carried across as well, which is what makes this the
/// copying half of [`move_path_no_replace`]: a move has to hand back what it was
/// given.
pub fn copy_path_no_replace(src: &Path, dst: &Path) -> Result<()> {
    ensure_destination_outside_source(src, dst)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let metadata = fs::symlink_metadata(src)?;
    if metadata.is_symlink() {
        return copy_symlink(src, dst, Existing::Refuse);
    }
    if metadata.is_dir() {
        return copy_dir_all(src, dst, Existing::Refuse);
    }
    copy_file(src, dst, Existing::Refuse)
}

// Recreates a symlink to `target` at `link`, failing if `link` is taken.
#[cfg(unix)]
fn symlink_no_replace(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

// Linux and macOS are the platforms the explorer targets, and the only ones a
// recursive copy can hand a symbolic link back on. Refusing beats the
// alternative of silently replacing a link with a copy of whatever it pointed
// at.
#[cfg(not(unix))]
fn symlink_no_replace(_target: &Path, link: &Path) -> Result<()> {
    Err(Error::Other(format!(
        "cannot recreate a symbolic link on this platform: {}",
        link.display()
    )))
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
#[tracing::instrument(target = "nohrs::op", name = "fs.rename", level = "debug", skip_all, fields(src = %src.display(), new_name))]
pub fn rename_in_place(src: &Path, new_name: &str) -> Result<PathBuf> {
    let parent = src.parent().ok_or_else(|| {
        Error::Other(format!(
            "cannot rename path without a parent: {}",
            src.display()
        ))
    })?;
    let dst = destination_in(parent, OsStr::new(new_name))?;
    fs::rename(src, &dst)?;
    Ok(dst)
}

/// Creates a new directory named `name` inside `parent`, returning its full
/// path. Fails if a file or directory of that name already exists.
#[tracing::instrument(target = "nohrs::op", name = "fs.create_dir", level = "debug", skip_all, fields(parent = %parent.display(), name))]
pub fn create_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    let dst = destination_in(parent, OsStr::new(name))?;
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
#[tracing::instrument(target = "nohrs::op", name = "fs.trash", level = "debug", skip_all, fields(path = %path.display()))]
pub fn trash_path(path: &Path, ledger: Option<&dyn TrashLedger>) -> Result<()> {
    // Captured before the move, while the item is still at its original
    // location — that is the whole point of the record. Failing here fails the
    // whole operation: nothing has moved yet, and deleting an item we already
    // know we cannot record would make it unrestorable on this platform.
    let captured = match ledger {
        Some(_) => Some(trash::capture(path)?),
        None => None,
    };
    ::trash::delete(path)
        .map_err(|error| Error::Other(format!("failed to move to trash: {error}")))?;
    // Past this point the item is in the trash, so a ledger write that fails
    // cannot be reported as a failed delete — but it must not vanish either.
    if let (Some(ledger), Some(captured)) = (ledger, captured)
        && let Err(error) = record_trashed(ledger, &captured)
    {
        tracing::warn!(
            path = %path.display(),
            %error,
            "could not record the trashed item; it will not appear in `noh trash list`"
        );
    }
    Ok(())
}

fn record_trashed(ledger: &dyn TrashLedger, captured: &nohrs_store::TrashEntry) -> Result<()> {
    ledger
        .append(captured)
        .map(|_| ())
        .map_err(|error| Error::Other(format!("could not write the trash ledger: {error}")))
}

/// Permanently deletes `path`, whether it is a file, symlink, or directory
/// tree. This cannot be undone.
#[tracing::instrument(target = "nohrs::op", name = "fs.delete", level = "debug", skip_all, fields(path = %path.display()))]
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

    #[cfg(unix)]
    #[test]
    fn copying_a_tree_gets_past_a_link_to_a_directory() {
        // The link is not a directory, so a copy that followed it handed it to
        // `fs::copy`, which read the directory behind it and failed with
        // `EISDIR` — dragging an ordinary folder was enough to hit this.
        let dir = tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("kept.txt"), "kept").unwrap();

        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("plain.txt"), "plain").unwrap();
        std::os::unix::fs::symlink(&elsewhere, src.join("link-to-dir")).unwrap();

        let dst = dir.path().join("dst");
        copy_path(&src, &dst).unwrap();

        assert_eq!(fs::read_to_string(dst.join("plain.txt")).unwrap(), "plain");
        let copied_link = dst.join("link-to-dir");
        assert!(fs::symlink_metadata(&copied_link).unwrap().is_symlink());
        assert_eq!(fs::read_link(&copied_link).unwrap(), elsewhere);
        // Recreated rather than walked into: the copy holds a link, not a second
        // copy of the directory behind it.
        assert!(!fs::symlink_metadata(&copied_link).unwrap().is_dir());
        assert!(elsewhere.join("kept.txt").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_pointing_out_of_the_tree_is_not_followed_out_of_it() {
        // Nothing outside the source is read or written, so a link to an
        // ancestor cannot recurse and one to another volume cannot drag it in.
        let dir = tempdir().unwrap();
        let outside = dir.path().join("outside.txt");
        fs::write(&outside, "not mine to copy").unwrap();

        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        std::os::unix::fs::symlink(&outside, src.join("escape")).unwrap();
        std::os::unix::fs::symlink(dir.path(), src.join("up")).unwrap();

        let dst = dir.path().join("dst");
        copy_path(&src, &dst).unwrap();

        assert!(
            fs::symlink_metadata(dst.join("escape"))
                .unwrap()
                .is_symlink()
        );
        assert!(fs::symlink_metadata(dst.join("up")).unwrap().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn copying_a_link_into_the_directory_it_points_at_is_allowed() {
        // The containment guard must not mistake this for copying something
        // into itself: only the link is copied, never what it points at.
        let dir = tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let dst = target.join("link");
        copy_path(&link, &dst).unwrap();
        assert!(fs::symlink_metadata(&dst).unwrap().is_symlink());
    }

    #[test]
    fn copying_or_moving_a_directory_into_itself_is_refused() {
        // Without this the recursion walks into the copy it is writing and only
        // stops when the volume is full.
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("nested").join("deep.txt"), "deep").unwrap();

        for dst in [
            src.join("copy-of-src"),
            src.join("nested").join("copy-of-src"),
            // `..` and a `.` must not be able to dress the same path up as a
            // different one.
            src.join("nested").join("..").join("copy-of-src"),
            src.clone(),
        ] {
            assert!(copy_path(&src, &dst).is_err(), "copy allowed {dst:?}");
            assert!(move_path(&src, &dst).is_err(), "move allowed {dst:?}");
            assert!(
                move_path_no_replace(&src, &dst).is_err(),
                "no-replace move allowed {dst:?}"
            );
        }
        assert_eq!(
            fs::read_to_string(src.join("nested").join("deep.txt")).unwrap(),
            "deep",
            "a refused operation leaves the source alone"
        );

        // A sibling destination is still fine.
        copy_path(&src, &dir.path().join("beside")).unwrap();
    }

    #[test]
    fn a_trailing_dot_dot_cannot_dress_a_source_up_as_something_else() {
        // `dir/src/..` is `dir`, so this is copying `dir` into its own child.
        // `Path::file_name` is `None` for it, which is what made the guard leave
        // it unresolved and compare it as something *below* `dir/src`.
        let dir = tempdir().unwrap();
        let inner = dir.path().join("src");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("deep.txt"), "deep").unwrap();

        let disguised = inner.join("..");
        let dst = inner.join("copy-of-dir");
        assert!(copy_path(&disguised, &dst).is_err());
        assert!(move_path(&disguised, &dst).is_err());
        assert!(move_path_no_replace(&disguised, &dst).is_err());
        assert!(!dst.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_destination_link_pointing_back_into_the_source_is_refused() {
        // The destination's own final component is followed, unlike the
        // source's: `create_dir_all` and `fs::copy` follow it, so a link back
        // into the source is a write into the source however unrelated the path
        // it is spelled with looks. Left unchecked this copied a directory into
        // itself, and copied a file onto itself.
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("kept.txt"), "kept").unwrap();

        let link_to_source = dir.path().join("link-to-source");
        std::os::unix::fs::symlink(&source, &link_to_source).unwrap();
        assert!(copy_path(&source, &link_to_source).is_err());

        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();
        let link_to_file = dir.path().join("link-to-notes");
        std::os::unix::fs::symlink(&file, &link_to_file).unwrap();
        assert!(copy_path(&file, &link_to_file).is_err());

        assert_eq!(fs::read_to_string(&file).unwrap(), "payload");
        assert_eq!(fs::read_to_string(source.join("kept.txt")).unwrap(), "kept");
    }

    #[cfg(unix)]
    #[test]
    fn a_second_name_for_the_same_file_is_refused() {
        // A hard link is a different path and the same inode. A replacing copy
        // opens the destination truncating, so the source is emptied before a
        // byte of it is read — the same loss as copying a file onto itself, with
        // nothing in the two paths to hint at it.
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();
        let hard_link = dir.path().join("also-notes.txt");
        fs::hard_link(&file, &hard_link).unwrap();

        assert!(copy_path(&file, &hard_link).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "payload");
    }

    #[test]
    fn the_no_replace_copy_refuses_a_name_taken_below_its_root_too() {
        // Claiming only the root is not enough: a tree takes time to write, and
        // an entry that appears inside it meanwhile used to be replaced by
        // `fs::copy` / `create_dir_all` on the way past.
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested").join("deep.txt"), "mine").unwrap();

        // Stand in for the concurrent creator by pre-making what the recursion
        // is about to write, one level down from the root it claimed.
        let dst = dir.path().join("dst");
        fs::create_dir_all(dst.join("nested")).unwrap();
        fs::write(dst.join("nested").join("deep.txt"), "theirs").unwrap();

        assert!(copy_path_no_replace(&source, &dst).is_err());
        assert_eq!(
            fs::read_to_string(dst.join("nested").join("deep.txt")).unwrap(),
            "theirs",
            "what was already there has to survive"
        );
    }

    #[test]
    fn copying_a_file_onto_itself_is_refused() {
        // `fs::copy` opens the destination truncating, so with the same path on
        // both sides the contents are gone before a byte is read.
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "payload").unwrap();

        assert!(copy_path(&file, &file).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "payload");
    }

    #[test]
    fn destination_in_rejects_anything_but_a_plain_name() {
        let dir = tempdir().unwrap();
        for bad in ["../escape", "a/b", ".", "..", "", "/etc"] {
            assert!(
                destination_in(dir.path(), OsStr::new(bad)).is_err(),
                "allowed {bad:?}"
            );
        }
        assert_eq!(
            destination_in(dir.path(), OsStr::new("report.pdf")).unwrap(),
            dir.path().join("report.pdf")
        );
    }

    #[cfg(unix)]
    #[test]
    fn deleting_a_link_permanently_leaves_its_target_alone() {
        // `remove_dir_all` on a link to a directory would empty the directory it
        // points at; keying off `symlink_metadata` is what stops that.
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target-dir");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("inside.txt"), "inside").unwrap();
        let target_file = dir.path().join("target.txt");
        fs::write(&target_file, "pointed at").unwrap();

        let dir_link = dir.path().join("dir-link");
        std::os::unix::fs::symlink(&target_dir, &dir_link).unwrap();
        let file_link = dir.path().join("file-link");
        std::os::unix::fs::symlink(&target_file, &file_link).unwrap();

        delete_permanent(&dir_link).unwrap();
        delete_permanent(&file_link).unwrap();

        assert!(fs::symlink_metadata(&dir_link).is_err());
        assert!(fs::symlink_metadata(&file_link).is_err());
        assert_eq!(
            fs::read_to_string(target_dir.join("inside.txt")).unwrap(),
            "inside"
        );
        assert_eq!(fs::read_to_string(&target_file).unwrap(), "pointed at");
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
    fn move_path_no_replace_moves_when_the_destination_is_free() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        fs::write(&src, "payload").unwrap();

        // Which of the two it takes depends on whether this filesystem accepts
        // the no-replace flag, so only the outcome is asserted.
        move_path_no_replace(&src, &dst).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "payload");
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_move_keeps_a_symlink_a_symlink_and_a_mode_a_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let target = dir.path().join("target.txt");
        fs::write(&target, "pointed at").unwrap();

        // A copy that followed the link would restore the target's bytes and
        // leave the link behind as a plain file.
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let moved_link = dir.path().join("moved-link.txt");
        copy_path_no_replace(&link, &moved_link).unwrap();
        assert!(fs::symlink_metadata(&moved_link).unwrap().is_symlink());
        assert_eq!(fs::read_link(&moved_link).unwrap(), target);

        // And the same for one nested inside a moved directory.
        let tree = dir.path().join("tree");
        fs::create_dir(&tree).unwrap();
        std::os::unix::fs::symlink(&target, tree.join("inner-link.txt")).unwrap();
        let moved_tree = dir.path().join("moved-tree");
        copy_path_no_replace(&tree, &moved_tree).unwrap();
        let inner = moved_tree.join("inner-link.txt");
        assert!(fs::symlink_metadata(&inner).unwrap().is_symlink());
        assert_eq!(fs::read_link(&inner).unwrap(), target);

        let script = dir.path().join("script.sh");
        fs::write(&script, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let moved_script = dir.path().join("moved-script.sh");
        copy_path_no_replace(&script, &moved_script).unwrap();
        assert_eq!(
            fs::metadata(&moved_script).unwrap().permissions().mode() & 0o777,
            0o755,
            "an executable must not come back from the trash unexecutable"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_move_keeps_a_private_directory_private_at_every_depth() {
        use std::os::unix::fs::PermissionsExt;

        // `create_dir_all` takes its mode from the umask, so without carrying
        // the source's across, a restored `0700` directory would come back
        // readable by anyone with an account on the machine.
        let dir = tempdir().unwrap();
        let outer = dir.path().join("secrets");
        let inner = outer.join("deeper");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("key.txt"), "private").unwrap();
        fs::set_permissions(&inner, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&outer, fs::Permissions::from_mode(0o750)).unwrap();

        let moved = dir.path().join("moved-secrets");
        copy_path_no_replace(&outer, &moved).unwrap();

        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&moved), 0o750);
        assert_eq!(mode(&moved.join("deeper")), 0o700);
        assert_eq!(
            fs::read_to_string(moved.join("deeper").join("key.txt")).unwrap(),
            "private",
            "the contents still have to arrive, mode applied last"
        );
    }

    #[test]
    fn move_path_no_replace_refuses_an_occupied_destination() {
        // The difference from `move_path` that restoring depends on: whatever
        // is already there survives, and the source is still where it was.
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        fs::write(&src, "restored").unwrap();
        fs::write(&dst, "newer").unwrap();

        let error = move_path_no_replace(&src, &dst).unwrap_err();

        assert!(
            matches!(&error, Error::Io(io) if io.kind() == std::io::ErrorKind::AlreadyExists),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&dst).unwrap(), "newer");
        assert_eq!(fs::read_to_string(&src).unwrap(), "restored");
    }

    #[test]
    fn move_path_no_replace_refuses_an_occupied_directory_destination() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("inside.txt"), "mine").unwrap();
        fs::create_dir(&dst).unwrap();
        fs::write(dst.join("theirs.txt"), "theirs").unwrap();

        assert!(move_path_no_replace(&src, &dst).is_err());

        assert!(dst.join("theirs.txt").is_file());
        assert!(!dst.join("inside.txt").exists());
        assert!(src.join("inside.txt").is_file());
    }

    #[test]
    fn the_fallback_move_claims_the_destination_before_writing_to_it() {
        // Exercised directly: a test has one filesystem, so `rename` never
        // returns `EXDEV` and the copy path is otherwise unreachable here. It
        // is what runs wherever no no-replace rename exists, so it has to be as
        // refusing as the rename is.
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("top.txt"), "top").unwrap();
        let dst = dir.path().join("nested").join("dst");

        // A missing parent is created, the way `copy_path` does it.
        copy_path_no_replace(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(dst.join("top.txt")).unwrap(), "top");

        assert!(copy_path_no_replace(&src, &dst).is_err());

        let file = dir.path().join("file.txt");
        fs::write(&file, "mine").unwrap();
        let taken = dir.path().join("taken.txt");
        fs::write(&taken, "theirs").unwrap();
        assert!(copy_path_no_replace(&file, &taken).is_err());
        assert_eq!(fs::read_to_string(&taken).unwrap(), "theirs");
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
