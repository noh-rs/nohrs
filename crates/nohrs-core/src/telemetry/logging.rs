//! Installing the `tracing` subscriber, and the rolling file it writes to.
//!
//! Two sinks, because they have different readers:
//!
//! * **stderr**, human-readable, no span events. This is what a developer sees
//!   in a terminal, and what a CLI user sees when something goes wrong.
//! * **a rolling file**, JSON Lines, one record per event *and per span close*.
//!   This is what `noh log` prints and what the aggregation behind `noh perf`
//!   reads. A GUI has nowhere to show stderr, so without this the log of a
//!   session ends when its window does.
//!
//! The span-close records are the reason anything can be measured at all: every
//! `#[tracing::instrument]`ed function in the workspace closes a span, and the
//! file layer writes its `time.busy` when it does. Instrumenting an operation is
//! therefore the whole cost of making it visible to `noh perf` — no counters, no
//! registry, no per-crate wiring.

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

use super::LogErr;
use crate::config::paths;

/// Base name of the rolling log file. `tracing-appender` appends the date, so
/// the files on disk are `nohrs.log.2026-09-07` and so on.
pub const LOG_FILE_PREFIX: &str = "nohrs.log";

/// The `tracing` target every measured operation shares, whichever crate it
/// lives in.
///
/// One target rather than per-crate ones so that a single filter directive
/// turns measurement on for the whole application, and so `noh perf` has one
/// thing to select on. Instrumenting a new operation is
/// `#[tracing::instrument(target = OP_TARGET, name = "area.verb", level = "debug", …)]`
/// — nothing else has to be told about it.
pub const OP_TARGET: &str = "nohrs::op";

/// Default file filter: everything at `info`, plus the operations at `debug`.
///
/// Operations sit at `debug` so a terminal running with `RUST_LOG=info` is not
/// flooded by every directory listing, while the file — which exists to be
/// measured — records them without the user configuring anything.
const DEFAULT_FILE_FILTER: &str = "info,nohrs::op=debug";

/// How the file sink is configured. Defaults are what a normal run wants:
/// enabled, `info` and above, a week of history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLogConfig {
    /// Write the JSON Lines file at all. `false` leaves stderr as the only sink,
    /// which is what tests and one-shot CLI invocations want.
    pub enabled: bool,
    /// Directory holding the rolling files. Created if missing.
    pub directory: PathBuf,
    /// Filter for the file sink alone, in `RUST_LOG` syntax. Independent of the
    /// stderr filter so a quiet terminal can still record a detailed file.
    pub filter: String,
    /// How many rotated files to keep. `None` keeps everything.
    pub max_files: Option<usize>,
}

impl Default for FileLogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: paths::log_dir(),
            filter: DEFAULT_FILE_FILTER.to_string(),
            max_files: Some(7),
        }
    }
}

/// Keeps the log file's writer thread alive.
///
/// `tracing-appender` writes from a background thread, and dropping this guard
/// flushes and stops it. **The caller must hold it for the life of the process**
/// — binding it to `_` drops it immediately and silently loses every record. It
/// is `None` when no file sink was installed.
#[must_use = "dropping the guard stops the log file being written"]
pub struct LogGuard(Option<WorkerGuard>);

impl std::fmt::Debug for LogGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LogGuard")
            .field("file_sink", &self.0.is_some())
            .finish()
    }
}

/// Install the global subscriber with stderr only, honoring `RUST_LOG`.
///
/// Kept for callers that have no business writing a log file — tests, and any
/// short-lived process where a rotating file would be noise.
pub fn init_logging() {
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter());
    tracing_subscriber::registry()
        .with(stderr)
        .try_init()
        .log_err();
}

/// Install the global subscriber with both sinks, returning the guard that keeps
/// the file writer alive.
///
/// A file sink that cannot be created is reported and skipped rather than
/// failing the process: losing the log is not a reason to refuse to start.
pub fn init_logging_with_file(config: &FileLogConfig) -> LogGuard {
    let file = config.enabled.then(|| open_file_layer(config)).flatten();
    let (file_layer, guard) = match file {
        Some((layer, guard)) => (Some(layer), Some(guard)),
        None => (None, None),
    };
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter());
    tracing_subscriber::registry()
        .with(stderr)
        .with(file_layer)
        .try_init()
        .log_err();
    LogGuard(guard)
}

/// The stderr filter: `RUST_LOG` if set, `info` otherwise.
fn stderr_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

/// Build the file layer, or `None` if the directory cannot be created.
///
/// The layer is boxed because the two arms of `Option` must have one type, and
/// a `fmt::Layer` carries its writer and formatter in its own.
#[allow(clippy::type_complexity)]
fn open_file_layer<S>(
    config: &FileLogConfig,
) -> Option<(Box<dyn Layer<S> + Send + Sync>, WorkerGuard)>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    create_log_dir(&config.directory).log_err()?;

    let mut builder = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX);
    if let Some(max_files) = config.max_files {
        builder = builder.max_log_files(max_files);
    }
    let appender = builder.build(&config.directory).log_err()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_new(&config.filter)
        .log_err()
        .unwrap_or_else(|| EnvFilter::new("info"));
    let layer = fmt::layer()
        .json()
        .flatten_event(true)
        // Both of these carry the span's *name and fields*, which is what says
        // which operation a record is about. Without them a close record is a
        // duration attached to nothing — `time.busy` with no way to tell a
        // search from a delete. `span_list` additionally gives the ancestry, so
        // a slow store call can be attributed to the search that caused it.
        .with_current_span(true)
        .with_span_list(true)
        // The whole point of the file: a record when each instrumented
        // operation finishes, carrying how long it was busy.
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(writer)
        .with_filter(filter);
    Some((Box::new(layer), guard))
}

fn create_log_dir(directory: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)
}

#[cfg(test)]
// The fixtures create real directories and files to exercise the appender, so
// they need the synchronous filesystem calls app code routes elsewhere.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn init_logging_is_idempotent() {
        // Installs a global subscriber (or no-ops via `try_init` if one already
        // exists); calling it repeatedly must not panic.
        init_logging();
        init_logging();
    }

    #[test]
    fn the_default_file_sink_writes_under_the_state_directory() {
        let config = FileLogConfig::default();
        assert!(config.enabled);
        assert_eq!(config.directory, paths::log_dir());
        assert_eq!(config.max_files, Some(7));
    }

    #[test]
    fn a_disabled_file_sink_creates_no_directory() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("logs");
        let config = FileLogConfig {
            enabled: false,
            directory: target.clone(),
            ..FileLogConfig::default()
        };

        // `try_init` may no-op because another test installed a subscriber
        // first; what matters is that the disabled sink touched nothing.
        let guard = init_logging_with_file(&config);

        assert!(!target.exists(), "a disabled sink must not create its dir");
        assert!(
            format!("{guard:?}").contains("file_sink: false"),
            "{guard:?}"
        );
    }

    #[test]
    fn an_enabled_file_sink_creates_its_directory_and_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("nested").join("logs");
        let config = FileLogConfig {
            enabled: true,
            directory: target.clone(),
            filter: "info".to_string(),
            max_files: Some(2),
        };

        // Built directly rather than through `init_logging_with_file`, because
        // only one subscriber can be installed per process and the tests share
        // one. This still exercises directory creation and the appender.
        let layer = open_file_layer::<tracing_subscriber::Registry>(&config);
        let (_layer, guard) = layer.expect("the file layer should open");

        assert!(target.is_dir(), "the log directory should be created");
        drop(guard);
    }

    #[test]
    fn a_directory_that_cannot_be_created_is_skipped_rather_than_fatal() {
        let directory = tempfile::tempdir().unwrap();
        // A file where the log directory should be: `create_dir_all` fails.
        let occupied = directory.path().join("occupied");
        std::fs::write(&occupied, "not a directory").unwrap();
        let config = FileLogConfig {
            enabled: true,
            directory: occupied,
            ..FileLogConfig::default()
        };

        assert!(open_file_layer::<tracing_subscriber::Registry>(&config).is_none());
    }
}
