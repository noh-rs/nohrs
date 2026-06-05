//! Centralized configuration shared across the application.
//!
//! Two distinct things live under `config`:
//!
//! * The compile-time UI layout constants below (window/table sizing), kept
//!   here so behaviour can be tuned in one place.
//! * The user-facing `config.toml` system in the [`settings`], [`paths`],
//!   [`loader`], and [`watcher`] submodules: schema, XDG paths, the
//!   4-layer override merge, lenient validation, hot-reload watching, and the
//!   on-disk lifecycle (see `docs/config.md`).

use std::time::Duration;

pub mod loader;
pub mod paths;
pub mod settings;
pub mod watcher;

pub use loader::{backup, ensure_exists, needs_migration, reset, write_default};
pub use settings::{
    CURRENT_SCHEMA_VERSION, Config, ConfigOverride, Diagnostic, DiagnosticLevel, Diagnostics,
    DiagnosticsStore, Indexing, IndexingExclude, IndexingMode, Keybindings, Launcher, Plugins,
    SCHEMA_URL, Search, SearchBackend, SortOrder, Theme, ThemeMode, Ui, json_schema_string,
    load_from_path, report_diagnostics,
};
pub use watcher::ConfigWatcher;

/// Default window width on first launch (logical pixels).
pub const WINDOW_WIDTH: f32 = 1280.0;
/// Default window height on first launch (logical pixels).
pub const WINDOW_HEIGHT: f32 = 780.0;

/// Initial width of the name column in the explorer table (logical pixels).
pub const COL_NAME_WIDTH: f32 = 400.0;
/// Initial width of the type column in the explorer table (logical pixels).
pub const COL_TYPE_WIDTH: f32 = 120.0;
/// Initial width of the size column in the explorer table (logical pixels).
pub const COL_SIZE_WIDTH: f32 = 120.0;
/// Initial width of the modified column in the explorer table (logical pixels).
pub const COL_MODIFIED_WIDTH: f32 = 180.0;
/// Initial width of the action column in the explorer table (logical pixels).
pub const COL_ACTION_WIDTH: f32 = 60.0;

/// Smallest width a column may be resized to (logical pixels).
pub const MIN_COLUMN_WIDTH: f32 = 80.0;

/// Extra horizontal padding added to the sum of column widths to obtain the
/// total table width (left + right row padding).
pub const TABLE_HORIZONTAL_PADDING: f32 = 48.0;

/// Height of the header row in the explorer table (logical pixels).
pub const HEADER_ROW_HEIGHT: f32 = 48.0;
/// Height of a regular entry row in the explorer table (logical pixels).
pub const BASE_ROW_HEIGHT: f32 = 32.0;
/// Height of a match snippet row under an expanded result (logical pixels).
pub const SNIPPET_ROW_HEIGHT: f32 = 24.0;

/// Maximum number of match snippets shown under an expanded search result.
pub const MAX_SNIPPETS: usize = 10;

/// Files larger than this are not previewed inline (bytes).
pub const PREVIEW_MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Maximum number of entries fetched in a single directory listing page.
pub const DIR_LISTING_LIMIT: usize = 1000;

/// Window during which a `Confirm` event following a double-click is suppressed
/// so a double-click does not both preview and re-activate the same row.
pub const CONFIRM_SUPPRESS_WINDOW: Duration = Duration::from_millis(300);

/// Lines longer than this are skipped when grepping file contents, to bound
/// memory use on minified or binary-ish files.
pub const SEARCH_MAX_LINE_LEN: usize = 1000;

/// Upper bound on matches collected from a single file during a content search.
pub const SEARCH_MAX_MATCHES_PER_FILE: usize = 1000;

/// Process-wide serialization for tests that mutate the environment.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    // `std::env::set_var`/`remove_var` mutate process-global state that any
    // thread's `getenv` can observe, so every env-mutating test in this crate —
    // across all `config` submodules, which compile into one test binary — must
    // serialize through a single lock, not a per-module one. Recover from a
    // poisoned lock (a panic in another env test) so the offending test's own
    // assertion surfaces instead of an opaque poison panic in later env tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the shared environment lock for the duration of an env-mutating test.
    pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
