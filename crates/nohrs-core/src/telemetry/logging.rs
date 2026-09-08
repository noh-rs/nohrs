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
    // The diagnostics are *carried*, not logged, because nothing is listening
    // yet: reporting them here would go to a subscriber that does not exist, and
    // the file sink would vanish with no diagnostic at all. They are emitted
    // below, once stderr is installed.
    let mut notes = Vec::new();
    let (file_layer, guard) = match config.enabled {
        false => (None, None),
        true => match open_file_layer(config) {
            Ok((layer, guard, warning)) => {
                notes.extend(warning);
                (Some(layer), Some(guard))
            }
            Err(failure) => {
                notes.push(format!("{failure}; continuing without a log file"));
                (None, None)
            }
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
    for note in notes {
        tracing::warn!("{note}");
    }
    LogGuard(guard)
}

/// Name of the file the appender writes to right now, if it can be determined.
///
/// Rotation is daily on the **UTC** date, so this names the one file a live
/// process can still append to. `noh log clear` needs the distinction:
/// unlinking that file would leave a running GUI appending to an inode with no
/// name, losing every record until the next rotation, where truncating it is
/// seen by the writer at once.
///
/// A writer idle across midnight still *holds* yesterday's file open, and this
/// already names today's — deliberately. `RollingFileAppender::write` tests for
/// rollover **before** each write, so that writer's next record opens today's
/// file and lands there; nothing is ever appended to yesterday's inode again,
/// and unlinking it loses nothing. Naming the open-but-finished file instead
/// would leave a stale file that `clear` never empties.
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
) -> Result<(Box<dyn Layer<S> + Send + Sync>, WorkerGuard, Option<String>), String>
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
    // it is tightened afterwards. Not fatal if it fails: the directory's own
    // `0700` already denies traversal, which is what actually keeps the records
    // private, and refusing to start over a redundant permission bit would trade
    // a working application for nothing. The warning is carried out to
    // `init_logging_with_file`, which has a subscriber to report it through.
    let warning = restrict_log_files(directory).err().map(|error| {
        format!(
            "could not restrict the log files in {}: {error}",
            directory.display()
        )
    });
    let appender = OwnerOnly::wrap(appender, directory.clone());
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
    Ok((Box::new(layer), guard, warning))
}

/// Wraps the appender so that each file it *rolls over to* is tightened too.
///
/// Tightening once at startup only covers the file open at the time. A GUI left
/// running past midnight rolls over to a file the appender creates itself, under
/// whatever umask the session has — so the documented `0600` would hold for the
/// first day of a session and quietly stop holding after it.
///
/// The date is read **before** the inner write and the tightening happens
/// **after** it, and both halves of that are load-bearing:
///
/// * before, because the inner write is where the rollover happens. Reading the
///   date afterwards lets a record written at 23:59:59.999 see 00:00:00.001 and
///   mark the new day done — and the file the *next* record actually rolls over
///   to is then skipped for a whole day.
/// * after, because the file only exists once the rollover has created it.
///
/// This wrapper's clock and the appender's are still two separate reads, so
/// midnight can fall between them and the rollover can beat the marker by one
/// record. That resolves itself: the marker is still on yesterday, so the very
/// next write tightens. A bounded one-record window, rather than a day.
///
/// The date comparison is what keeps this to one `read_dir` a day rather than
/// one per record.
#[cfg(unix)]
struct OwnerOnly<W> {
    inner: W,
    directory: PathBuf,
    /// The last date whose files were successfully tightened. `None` until the
    /// first write, so startup never has to guess — including when it straddles
    /// midnight itself.
    restricted_on: Option<time::Date>,
    /// Injected so the midnight boundary can be tested. It is the only
    /// interesting case here and there is no way to wait for it.
    now: Box<dyn FnMut() -> time::Date + Send>,
}

#[cfg(unix)]
impl<W: std::io::Write> OwnerOnly<W> {
    fn wrap(inner: W, directory: PathBuf) -> Self {
        Self {
            inner,
            directory,
            restricted_on: None,
            now: Box::new(|| time::OffsetDateTime::now_utc().date()),
        }
    }
}

#[cfg(unix)]
impl<W: std::io::Write> std::io::Write for OwnerOnly<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let today = (self.now)();
        let rolled = self.restricted_on != Some(today);
        let written = self.inner.write(buf)?;
        if rolled {
            // Only on success: marking the day done after a failed chmod would
            // suppress every retry and leave the new file at its umask mode
            // until tomorrow.
            if restrict_log_files(&self.directory).log_err().is_some() {
                self.restricted_on = Some(today);
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// On Windows there is no mode to set, so the wrapper is the appender itself.
#[cfg(not(unix))]
struct OwnerOnly;

#[cfg(not(unix))]
impl OwnerOnly {
    fn wrap<W>(inner: W, _directory: PathBuf) -> W {
        inner
    }
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

/// Tighten every log file in `directory` to owner-only.
///
/// Reports rather than swallows a failure, so that a directory the process
/// cannot scan is diagnosable. Whether to *act* on that is the caller's call:
/// both call sites treat it as a warning, since the files are still behind a
/// `0700` directory and refusing to run over a redundant permission bit would
/// trade a working application for nothing.
#[cfg(unix)]
fn restrict_log_files(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // One file that cannot be chmodded must not leave every later one at its
    // umask mode, so the loop runs to the end and reports the first failure
    // rather than stopping at it.
    let mut failure = None;
    for entry in std::fs::read_dir(directory)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failure.get_or_insert(error);
                continue;
            }
        };
        if entry.file_name().to_str().is_some_and(is_log_file_name)
            && let Err(error) =
                std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600))
        {
            failure.get_or_insert(error);
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn restrict_log_files(_directory: &Path) -> std::io::Result<()> {
    Ok(())
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
        let (_layer, guard, warning) = layer.expect("the file layer should open");
        assert_eq!(warning, None, "a fresh directory has nothing to warn about");

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

        let (_layer, guard, _) =
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

        let (_layer, guard, _) =
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
    fn a_file_created_by_a_rollover_is_tightened_too() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        // The appender creates each day's file itself, under whatever umask the
        // session has. Tightening only at startup would hold for the first day
        // of a long-running GUI and quietly stop holding after midnight.
        let directory = tempfile::tempdir().unwrap();
        let rolled = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-08"));
        std::fs::write(&rolled, "{}\n").unwrap();
        std::fs::set_permissions(&rolled, std::fs::Permissions::from_mode(0o644)).unwrap();

        let mut writer = OwnerOnly::wrap(std::io::sink(), directory.path().to_path_buf());
        writer.write_all(b"a record after midnight").unwrap();

        let mode = std::fs::metadata(&rolled).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a rolled-over file must be tightened too");
    }

    /// A stand-in for the appender, sharing one clock with the wrapper.
    ///
    /// Every read of that clock advances it a tick, and `schedule` says which
    /// day-of-month each tick falls on (the last value repeats). The writer then
    /// rolls over the way the real appender does: it writes to the file for
    /// whatever date the clock says *at the moment of its own write*.
    ///
    /// That shared, advancing clock is the whole point. The wrapper's read and
    /// the appender's read are separate ticks, so a schedule can put midnight
    /// between them — which is exactly the case the ordering has to survive, and
    /// a clock that only counted calls could not express it.
    #[cfg(unix)]
    struct Appenderish {
        clock: TestClock,
        directory: std::path::PathBuf,
    }

    #[cfg(unix)]
    #[derive(Clone)]
    struct TestClock {
        ticks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        schedule: std::sync::Arc<Vec<u8>>,
    }

    #[cfg(unix)]
    impl TestClock {
        fn new(schedule: Vec<u8>) -> Self {
            Self {
                ticks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                schedule: std::sync::Arc::new(schedule),
            }
        }

        /// The day this read falls on, advancing the clock by one tick.
        fn tick(&self) -> u8 {
            let index = self.ticks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.schedule
                .get(index)
                .or_else(|| self.schedule.last())
                .copied()
                .unwrap_or(1)
        }

        fn date(&self) -> time::Date {
            let day = self.tick();
            time::Date::from_calendar_date(2026, time::Month::September, day)
                .unwrap_or(time::Date::MIN)
        }
    }

    #[cfg(unix)]
    impl std::io::Write for Appenderish {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            use std::os::unix::fs::PermissionsExt;
            let path = self
                .directory
                .join(format!("{LOG_FILE_PREFIX}.2026-09-0{}", self.clock.tick()));
            if path.exists() {
                return Ok(buf.len());
            }
            // The rollover: a file the appender creates itself, under the
            // session's umask rather than the mode nohrs documents.
            std::fs::write(&path, "{}\n")?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// An `OwnerOnly` scanning `scanned`, over an appender writing into
    /// `written_to`, both driven by one `schedule`. The two directories differ
    /// only where a test needs the tightening to fail while the write succeeds.
    #[cfg(unix)]
    fn wrapped_appender(
        scanned: &std::path::Path,
        written_to: &std::path::Path,
        schedule: Vec<u8>,
    ) -> OwnerOnly<Appenderish> {
        let clock = TestClock::new(schedule);
        OwnerOnly {
            inner: Appenderish {
                clock: clock.clone(),
                directory: written_to.to_path_buf(),
            },
            directory: scanned.to_path_buf(),
            restricted_on: None,
            now: Box::new(move || clock.date()),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_record_written_across_midnight_does_not_skip_the_next_days_file() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        // The date has to be read *before* the inner write, because the inner
        // write is where the rollover happens. Reading it after lets the first
        // record — issued at 23:59:59.999, its date landing on the next day —
        // mark the new day done, and the file the *second* record actually rolls
        // over to is then never tightened.
        //
        // Ticks: 1 is the wrapper's read for the first record, 2 the appender's
        // for it, and 3 and 4 the same pair for the second. Midnight falls
        // between ticks 1 and 2 — between the wrapper and the appender.
        let directory = tempfile::tempdir().unwrap();
        let mut writer = wrapped_appender(directory.path(), directory.path(), vec![1, 2, 2, 2]);

        writer.write_all(b"just before midnight").unwrap();
        writer.write_all(b"just after midnight").unwrap();

        let files: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(!files.is_empty(), "the appender should have rolled over");
        for file in files {
            let mode = std::fs::metadata(file.path()).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "{:?} was left at its umask mode",
                file.file_name()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_tightening_is_retried_on_the_next_record() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        // Marking the day done after a failed chmod would suppress every retry
        // and leave the new file readable until tomorrow. The scanned directory
        // is absent for the first record, so the tightening fails while the
        // write itself succeeds — and the second record, on the *same* day, has
        // to try again.
        let root = tempfile::tempdir().unwrap();
        let scanned = root.path().join("scanned");
        let written_to = root.path().join("written-to");
        std::fs::create_dir_all(&written_to).unwrap();
        let mut writer = wrapped_appender(&scanned, &written_to, vec![1]);

        writer.write_all(b"first").unwrap();
        assert_eq!(
            writer.restricted_on, None,
            "a failed tightening must not count as done"
        );

        // The rolled-over file, still at its umask mode, now reachable.
        std::fs::rename(&written_to, &scanned).unwrap();
        writer.inner.directory = scanned.clone();
        writer.write_all(b"second").unwrap();

        let rolled = scanned.join(format!("{LOG_FILE_PREFIX}.2026-09-01"));
        let mode = std::fs::metadata(&rolled).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the retry must happen on the same day");
    }

    #[cfg(unix)]
    #[test]
    fn one_unchmoddable_file_does_not_leave_the_rest_permissive() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        // A directory carrying a log file's exact name: `set_permissions` on it
        // succeeds, so instead make the *later* file the readable one and the
        // earlier a name whose chmod fails — a dangling symlink.
        let broken = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-01"));
        std::os::unix::fs::symlink(directory.path().join("nowhere"), &broken).unwrap();
        let good = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-02"));
        std::fs::write(&good, "{}\n").unwrap();
        std::fs::set_permissions(&good, std::fs::Permissions::from_mode(0o644)).unwrap();

        let result = restrict_log_files(directory.path());

        assert!(result.is_err(), "the failure still has to be reported");
        let mode = std::fs::metadata(&good).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a later file must still be tightened");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_cannot_be_scanned_is_reported_not_swallowed() {
        // Silently returning would leave a failed tightening undiagnosable,
        // which is the one thing the caller needs to be able to say.
        let directory = tempfile::tempdir().unwrap();
        let occupied = directory.path().join("logs");
        std::fs::write(&occupied, "not a directory").unwrap();

        assert!(restrict_log_files(&occupied).is_err());
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
