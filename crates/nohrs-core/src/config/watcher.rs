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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind};
    use std::sync::mpsc;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn is_relevant_matches_only_mutating_events() {
        assert!(is_relevant(&Event::new(EventKind::Create(CreateKind::Any))));
        assert!(is_relevant(&Event::new(EventKind::Modify(ModifyKind::Any))));
        assert!(is_relevant(&Event::new(EventKind::Remove(
            notify::event::RemoveKind::Any
        ))));
        assert!(!is_relevant(&Event::new(EventKind::Access(
            AccessKind::Any
        ))));
    }

    #[test]
    fn new_invokes_callback_on_config_change() {
        let dir = tempdir().unwrap();
        // Canonicalize so the watched path matches the (canonical) paths the OS
        // backend reports — on macOS FSEvents resolves `/tmp` → `/private/tmp`,
        // which would otherwise never match the watcher's exact-path filter.
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let config = root.join("config.toml");
        std::fs::write(&config, "schema_version = 1").unwrap();

        let (sender, receiver) = mpsc::channel();
        let _watcher = ConfigWatcher::new(&config, move || {
            sender.send(()).ok();
        })
        .unwrap();

        // Give the OS watch a moment to start (FSEvents has a startup latency)
        // before mutating, then wait generously for the event. std mpsc + sleep
        // are fine here — this is a plain `#[test]`, not a `#[gpui::test]`.
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(&config, "schema_version = 1\n# edited").unwrap();
        assert!(
            receiver.recv_timeout(Duration::from_secs(10)).is_ok(),
            "watcher did not report the config change"
        );
    }
}
