//! Minimal `config.toml` file watcher used for hot reload (config.md §5).
//!
//! Editors commonly save by writing a temporary file and renaming it over the
//! target, which means watching the file inode directly misses updates. We
//! therefore watch the containing directory and filter events down to the
//! config file. The watcher runs on a background thread owned by `notify`; the
//! supplied callback is invoked there, so callers must marshal back onto their
//! own thread (e.g. the GPUI foreground) before touching UI state.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};

/// Owns the underlying OS watch. Dropping it stops watching.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Watch `config_file` for create/modify/remove/rename events, invoking
    /// `on_change` for each relevant event. The parent directory is watched
    /// non-recursively so atomic-rename saves are detected.
    pub fn new(config_file: &Path, on_change: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let target: PathBuf = config_file.to_path_buf();
        let watch_dir = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    if is_relevant(&event) && event.paths.iter().any(|path| path == &target) {
                        on_change();
                    }
                }
                Err(error) => tracing::warn!("config watcher error: {error}"),
            })?;

        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

        Ok(Self { _watcher: watcher })
    }
}

fn is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}
