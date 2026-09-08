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
///
/// Built from [`OP_TARGET`] rather than spelling it again: a filter naming a
/// target that no longer exists is accepted by `EnvFilter` without complaint,
/// so a rename would silently stop recording every operation — the exact defect
/// this filter exists to prevent.
fn default_file_filter() -> String {
    format!("info,{OP_TARGET}=debug")
}

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
            filter: default_file_filter(),
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
    // The failure is *carried*, not logged, because nothing is listening yet:
    // reporting it here would go to a subscriber that does not exist, and the
    // file sink would vanish with no diagnostic at all. It is emitted below,
    // once stderr is installed.
    let (file_layer, guard, failure) = match config.enabled {
        false => (None, None, None),
        true => match open_file_layer(config) {
            Ok((layer, guard)) => (Some(layer), Some(guard), None),
            Err(failure) => (None, None, Some(failure)),
        },
    };
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter());
    tracing_subscriber::registry()
        .with(stderr)
        .with(file_layer)
        .try_init()
        .log_err();
    if let Some(failure) = failure {
        tracing::warn!("{failure}; continuing without a log file");
    }
    LogGuard(guard)
}

/// Name of the file the appender is appending to right now, if it can be
/// determined.
///
/// Rotation is daily on the **UTC** date (`tracing-appender` rounds
/// `now_utc()`), so exactly one file in the directory can be held open by a live
/// process and every other one is closed. `noh log clear` needs the distinction:
/// unlinking the open file would leave a running GUI writing to an inode with no
/// name, losing every record until the next rotation, where truncating it is
/// seen by the writer at once.
///
/// `None` means the date could not be formatted, which leaves the caller to
/// treat every file as closed — the behaviour before this existed.
pub fn current_log_file_name() -> Option<String> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let date = time::OffsetDateTime::now_utc().format(&format).log_err()?;
    Some(format!("{LOG_FILE_PREFIX}.{date}"))
}

/// Whether `name` is one the appender itself wrote.
///
/// A prefix match would be enough to *find* the files, but not to act on them:
/// it would hand `noh log clear` a hand-made `nohrs.log.backup` to unlink, and
/// `noh log show` someone's saved copy to parse. The appender only ever writes
/// `<prefix>.<yyyy-mm-dd>`, so that is the whole shape.
pub fn is_log_file_name(name: &str) -> bool {
    let Some(date) = name
        .strip_prefix(LOG_FILE_PREFIX)
        .and_then(|rest| rest.strip_prefix('.'))
    else {
        return false;
    };
    let bytes = date.as_bytes();
    bytes.len() == "yyyy-mm-dd".len()
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// The stderr filter: `RUST_LOG` if set, `info` otherwise.
fn stderr_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

/// Build the file layer, or describe why it could not be built.
///
/// The error is a `String` rather than a logged warning because this runs before
/// any subscriber exists (see [`init_logging_with_file`]). The layer is boxed
/// because the two arms must have one type, and a `fmt::Layer` carries its
/// writer and formatter in its own.
#[allow(clippy::type_complexity)]
fn open_file_layer<S>(
    config: &FileLogConfig,
) -> Result<(Box<dyn Layer<S> + Send + Sync>, WorkerGuard), String>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let directory = &config.directory;
    create_log_dir(directory)
        .map_err(|error| format!("could not prepare {}: {error}", directory.display()))?;

    let mut builder = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX);
    if let Some(max_files) = config.max_files {
        builder = builder.max_log_files(max_files);
    }
    let appender = builder.build(directory).map_err(|error| {
        format!(
            "could not open a log file in {}: {error}",
            directory.display()
        )
    })?;
    // The appender creates the file itself and offers no way to set its mode, so
    // it is tightened afterwards. The directory's own `0700` already denies
    // traversal during the gap, which is what actually keeps the records
    // private; this is the second lock on the same door.
    restrict_log_files(directory);
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
    Ok((Box::new(layer), guard))
}

/// Create the log directory, owner-only.
///
/// The records name every file the user touched and every search they ran, so
/// on a machine with more than one account the directory mode is the privacy
/// boundary — `0700` denies traversal, which makes the files inside unreachable
/// whatever their own mode says. An existing directory is tightened too: it may
/// have been created under a laxer umask by an older build.
#[cfg(unix)]
fn create_log_dir(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_log_dir(directory: &Path) -> std::io::Result<()> {
    // Windows inherits the parent's ACL, and `$XDG_STATE_HOME` under a user
    // profile is already per-user. There is no mode to set.
    std::fs::create_dir_all(directory)
}

/// Tighten the log files to owner-only, best effort.
///
/// Best effort because it is defence in depth: a failure here still leaves the
/// files behind a `0700` directory, and refusing to start over it would trade a
/// working application for a redundant permission bit.
#[cfg(unix)]
fn restrict_log_files(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str().is_some_and(is_log_file_name) {
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600))
                .log_err();
        }
    }
}

#[cfg(not(unix))]
fn restrict_log_files(_directory: &Path) {}

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
        // Reads `XDG_STATE_HOME` twice (here and inside `Default`), so it has to
        // serialize with the env-mutating tests in `config::paths` — they share
        // one test binary, and a set/remove landing between the two reads would
        // fail this comparison for no reason.
        let _guard = crate::config::test_env::env_lock();
        let config = FileLogConfig::default();
        assert!(config.enabled);
        assert_eq!(config.directory, paths::log_dir());
        assert_eq!(config.max_files, Some(7));
    }

    #[test]
    fn the_default_filter_names_the_target_the_operations_use() {
        // A filter naming a target nothing writes to is still a *valid* filter,
        // so a rename of `OP_TARGET` would silently stop the file recording
        // every operation. Deriving it is the guard; this is the alarm.
        assert!(
            default_file_filter().contains(OP_TARGET),
            "{}",
            default_file_filter()
        );
    }

    #[test]
    fn only_the_appenders_own_file_names_are_recognised() {
        // `noh log clear` unlinks what this accepts, so a hand-made copy kept
        // beside the real ones must not match.
        assert!(is_log_file_name("nohrs.log.2026-09-07"));
        assert!(current_log_file_name().is_some_and(|name| is_log_file_name(&name)));
        for name in [
            "nohrs.log.backup",
            "nohrs.log",
            "nohrs.log.2026-9-07",
            "nohrs.log.2026-09-07.gz",
            "notes.txt",
        ] {
            assert!(!is_log_file_name(name), "{name} was taken for a log file");
        }
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
    fn the_current_file_name_is_the_one_the_appender_opens() {
        // `noh log clear` picks what to truncate by this name. If it drifted
        // from the appender's own spelling, `clear` would unlink the file a
        // running GUI holds open — the exact case the name exists to avoid.
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("logs");
        let config = FileLogConfig {
            enabled: true,
            directory: target.clone(),
            ..FileLogConfig::default()
        };

        let (_layer, guard) =
            open_file_layer::<tracing_subscriber::Registry>(&config).expect("the file layer");
        drop(guard);

        let opened = std::fs::read_dir(&target)
            .unwrap()
            .next()
            .expect("a log file")
            .unwrap()
            .file_name();
        assert_eq!(
            current_log_file_name().as_deref(),
            opened.to_str(),
            "the name must match what the appender created"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_log_directory_and_its_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        // Records name every file touched and every search run, so on a shared
        // machine these modes are the privacy boundary. The assertion holds
        // whatever the umask is, because both modes are set explicitly rather
        // than left to `open(2)`'s default.
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("logs");
        let config = FileLogConfig {
            enabled: true,
            directory: target.clone(),
            ..FileLogConfig::default()
        };

        let (_layer, guard) =
            open_file_layer::<tracing_subscriber::Registry>(&config).expect("the file layer");
        drop(guard);

        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&target), 0o700, "the directory must deny traversal");
        let file = std::fs::read_dir(&target)
            .unwrap()
            .next()
            .expect("a log file")
            .unwrap()
            .path();
        assert_eq!(mode(&file), 0o600, "the file must be owner-only");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_created_under_a_lax_umask_is_tightened_on_reopen() {
        use std::os::unix::fs::PermissionsExt;

        // An older build (or another tool) may have left `0755` behind. The
        // mode passed to `DirBuilder` only applies at creation, so the tightening
        // has to be a separate, unconditional `set_permissions`.
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("logs");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();

        create_log_dir(&target).unwrap();

        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "an existing directory must be tightened");
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

        let failure = open_file_layer::<tracing_subscriber::Registry>(&config)
            .err()
            .expect("a file where the directory should be cannot be prepared");
        assert!(
            failure.contains("could not prepare"),
            "the message has to name what went wrong, since it is all the user sees: {failure}"
        );
    }
}
