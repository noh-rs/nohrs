//! On-disk lifecycle for `config.toml`: writing the default file, backing up an
//! existing one before overwriting, resetting, and the (P1-minimal) migration
//! scaffold.
//!
//! Migration support is intentionally a skeleton: only `schema_version = 1`
//! exists today, so [`needs_migration`] is always `false`. The
//! peek-version → backup → overwrite shape is in place so future bumps slot in
//! without reworking callers (config.md §7).

use super::settings::{CURRENT_SCHEMA_VERSION, Config};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ensure the parent directory of `path` exists.
fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Write the default `config.toml` (with `#:schema` header) to `path`, creating
/// parent directories as needed. Overwrites any existing file — callers that
/// must preserve the old contents should [`backup`] first.
pub fn write_default(path: &Path) -> io::Result<()> {
    ensure_parent_dir(path)?;
    // Config is loaded/written synchronously at startup, before the GPUI
    // foreground loop runs, so blocking I/O here cannot stall rendering.
    #[allow(clippy::disallowed_methods)]
    std::fs::write(path, Config::default_toml_template())
}

/// Create the config file with defaults only if it does not already exist.
/// Returns `true` if a new file was written.
pub fn ensure_exists(path: &Path) -> io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    write_default(path)?;
    Ok(true)
}

/// Copy `path` to a timestamped sibling `config.toml.bak-<unix-seconds>` so a
/// destructive operation is recoverable. Returns the backup path, or `None` if
/// the source does not exist.
pub fn backup(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".to_string());
    let backup_path = path.with_file_name(format!("{file_name}.bak-{seconds}"));
    std::fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}

/// Reset configuration to defaults, backing up any existing file first. Returns
/// the backup path (if one was made).
pub fn reset(path: &Path) -> io::Result<Option<PathBuf>> {
    let backup_path = backup(path)?;
    write_default(path)?;
    Ok(backup_path)
}

/// Whether a file declaring `version` requires migration before this build can
/// use it. P1 only knows v1, so this is always `false`; kept as the hook for
/// future schema bumps.
pub fn needs_migration(version: u64) -> bool {
    version < CURRENT_SCHEMA_VERSION
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_default_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        write_default(&path).unwrap();
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.starts_with("#:schema"));
    }

    #[test]
    fn ensure_exists_is_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(ensure_exists(&path).unwrap());
        std::fs::write(&path, "schema_version = 1\n").unwrap();
        // Second call must not overwrite the user's edits.
        assert!(!ensure_exists(&path).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "schema_version = 1\n"
        );
    }

    #[test]
    fn reset_backs_up_then_restores_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "schema_version = 1\nmangled").unwrap();
        let backup_path = reset(&path).unwrap().unwrap();
        assert!(backup_path.exists());
        assert_eq!(
            std::fs::read_to_string(&backup_path).unwrap(),
            "schema_version = 1\nmangled"
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("#:schema")
        );
    }

    #[test]
    fn backup_of_missing_file_is_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(backup(&path).unwrap().is_none());
    }

    #[test]
    fn write_default_creates_deep_parents_and_overwrites() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a/b/c/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "stale").unwrap();
        write_default(&path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.starts_with("#:schema"));
        assert!(!contents.contains("stale"));
    }

    #[test]
    fn ensure_exists_reports_creation_then_no_op() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(ensure_exists(&path).unwrap(), "first call creates the file");
        assert!(
            !ensure_exists(&path).unwrap(),
            "second call leaves it untouched"
        );
    }

    #[test]
    fn backup_preserves_original_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "schema_version = 1\naccent = \"teal\"").unwrap();
        let backup_path = backup(&path).unwrap().expect("backup created");
        assert_eq!(
            std::fs::read_to_string(&backup_path).unwrap(),
            "schema_version = 1\naccent = \"teal\""
        );
        // The original is left in place by `backup`.
        assert!(path.exists());
    }

    #[test]
    fn reset_without_existing_file_writes_default_and_no_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let backup_path = reset(&path).unwrap();
        assert!(backup_path.is_none());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("#:schema")
        );
    }

    #[test]
    fn needs_migration_only_for_older_versions() {
        assert!(needs_migration(0));
        assert!(!needs_migration(CURRENT_SCHEMA_VERSION));
        assert!(!needs_migration(CURRENT_SCHEMA_VERSION + 1));
        assert!(!needs_migration(u64::MAX));
    }
}
