//! `noh log`: read back what nohrs recorded about itself.
//!
//! The GUI writes to a rolling JSON Lines file because it has nowhere to show
//! stderr (`nohrs_core::telemetry::logging`). This is the reader for it: the
//! same records, rendered for a terminal, so "what did it just do, and how long
//! did it take" is answerable without opening the file by hand.
//!
//! Every record is one JSON object per line. A line that does not parse is
//! skipped rather than fatal — the file is appended to by a live process, so
//! the last line can be half-written when we read it.

// `clippy.toml` bans the synchronous `std::fs` readers so that blocking IO does
// not land on the GPUI foreground thread (`docs/explorer-essentials.md` §8).
// `noh` is a one-shot CLI process with no such thread: reading the log *is* the
// command, and there is no UI left to keep responsive. Routing it through
// `nohrs-services::fs` would only add an indirection with no reader.
#![allow(clippy::disallowed_methods)]

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use nohrs_core::telemetry::logging::LOG_FILE_PREFIX;

/// What `noh log` was asked to do.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print the most recent records.
    Show(ShowArgs),
    /// Print where the log files live.
    Path,
    /// Delete the log files.
    Clear {
        /// Delete without asking.
        #[arg(short, long)]
        force: bool,
    },
}

/// Number of records `show` prints when `--lines` is not given.
const DEFAULT_LINES: usize = 50;

/// Options for `noh log show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// How many records to print, most recent last.
    #[arg(short = 'n', long, default_value_t = DEFAULT_LINES, value_name = "N")]
    pub lines: usize,
    /// Only records for a completed operation, with their durations.
    #[arg(long)]
    pub ops: bool,
    /// Print the raw JSON lines instead of the rendered form.
    #[arg(long)]
    pub json: bool,
}

// Written out rather than derived: `#[derive(Default)]` would give `lines: 0`,
// which prints nothing, while clap gives `DEFAULT_LINES`. A `Default` that
// disagrees with the parsed default is a trap for every caller that builds this
// struct directly.
impl Default for ShowArgs {
    fn default() -> Self {
        Self {
            lines: DEFAULT_LINES,
            ops: false,
            json: false,
        }
    }
}

/// What a run of `noh log` did, which decides the exit code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Records printed.
    pub printed: usize,
    /// Files removed by `clear`.
    pub removed: usize,
    /// Whether the command could not do what was asked.
    pub failed: bool,
}

impl Summary {
    /// `0` when the command did its job, `1` otherwise.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed)
    }
}

/// One run of `noh log`, against a log directory and an output sink.
///
/// The directory is injected rather than resolved internally so the tests run
/// against a temporary one; the real caller passes
/// `nohrs_core::config::paths::log_dir()`.
pub struct Session<'a> {
    directory: &'a Path,
    output: &'a mut dyn Write,
    summary: Summary,
}

impl<'a> Session<'a> {
    /// A session writing to `output` and reading logs from `directory`.
    pub fn new(directory: &'a Path, output: &'a mut dyn Write) -> Self {
        Self {
            directory,
            output,
            summary: Summary::default(),
        }
    }

    /// Run `command`, returning what happened.
    pub fn run(mut self, command: &Command) -> io::Result<Summary> {
        match command {
            Command::Show(args) => self.show(args)?,
            Command::Path => self.path()?,
            Command::Clear { force } => self.clear(*force)?,
        }
        Ok(self.summary)
    }

    fn show(&mut self, args: &ShowArgs) -> io::Result<()> {
        let files = log_files(self.directory);
        if files.is_empty() {
            writeln!(self.output, "no log files in {}", self.directory.display())?;
            // Not a failure: a fresh install has never written one, and saying
            // so is the answer to the question that was asked.
            return Ok(());
        }
        // Newest file last, so reading them in order and keeping the tail gives
        // the most recent records across a rotation boundary.
        let mut records = Vec::new();
        for file in &files {
            let body = std::fs::read_to_string(file)?;
            records.extend(body.lines().filter_map(Record::parse));
        }
        if args.ops {
            records.retain(Record::is_operation);
        }
        let start = records.len().saturating_sub(args.lines);
        for record in &records[start..] {
            if args.json {
                writeln!(self.output, "{}", record.raw)?;
            } else {
                writeln!(self.output, "{}", record.render())?;
            }
            self.summary.printed += 1;
        }
        Ok(())
    }

    fn path(&mut self) -> io::Result<()> {
        writeln!(self.output, "{}", self.directory.display())?;
        for file in log_files(self.directory) {
            writeln!(self.output, "  {}", file.display())?;
        }
        Ok(())
    }

    fn clear(&mut self, force: bool) -> io::Result<()> {
        let files = log_files(self.directory);
        if files.is_empty() {
            writeln!(self.output, "no log files to remove")?;
            return Ok(());
        }
        if !force {
            // No prompt here: `noh log clear` is not destructive to user data,
            // but it is still an unasked-for deletion, so it needs the flag.
            writeln!(
                self.output,
                "{} log file(s) in {}; pass --force to delete them",
                files.len(),
                self.directory.display()
            )?;
            return Ok(());
        }
        for file in files {
            match std::fs::remove_file(&file) {
                Ok(()) => self.summary.removed += 1,
                Err(error) => {
                    writeln!(self.output, "{}: {error}", file.display())?;
                    self.summary.failed = true;
                }
            }
        }
        writeln!(self.output, "removed {} log file(s)", self.summary.removed)?;
        Ok(())
    }
}

/// The log files in `directory`, oldest first.
///
/// `tracing-appender` names them `<prefix>.<date>`, so a lexicographic sort is
/// also chronological. A directory that cannot be read is reported as empty:
/// the caller's message ("no log files in …") is the useful thing to say either
/// way.
fn log_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX))
        })
        .collect();
    files.sort();
    files
}

/// One parsed line of the log file.
struct Record {
    raw: String,
    value: serde_json::Value,
}

impl Record {
    fn parse(line: &str) -> Option<Self> {
        // A half-written trailing line is normal while a process is running.
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        Some(Self {
            raw: line.to_string(),
            value,
        })
    }

    /// Whether this is the close of a measured operation rather than an
    /// ordinary log event.
    fn is_operation(&self) -> bool {
        self.value["message"] == "close" && self.value["time.busy"].is_string()
    }

    fn field(&self, name: &str) -> &str {
        self.value[name].as_str().unwrap_or_default()
    }

    /// Render for a terminal: time, then either the operation and its duration,
    /// or the level and message.
    fn render(&self) -> String {
        let time = time_of_day(self.field("timestamp"));
        if self.is_operation() {
            let name = self.value["span"]["name"].as_str().unwrap_or("?");
            return format!(
                "{time}  {name:<24} {:>10}{}",
                self.field("time.busy"),
                span_fields(&self.value["span"])
            );
        }
        format!(
            "{time}  {:<5} {}: {}",
            self.field("level"),
            self.field("target"),
            self.field("message")
        )
    }
}

/// `2026-09-07T14:14:17.283200Z` becomes `14:14:17`. Anything unexpected is
/// passed through, because a wrong-looking timestamp is worth seeing.
fn time_of_day(timestamp: &str) -> &str {
    match timestamp.split_once('T') {
        Some((_, rest)) => rest.split('.').next().unwrap_or(timestamp),
        None => timestamp,
    }
}

/// The instrumented fields of a span, minus the name, as ` key=value` pairs.
fn span_fields(span: &serde_json::Value) -> String {
    let Some(fields) = span.as_object() else {
        return String::new();
    };
    fields
        .iter()
        .filter(|(key, _)| key.as_str() != "name")
        .map(|(key, value)| match value.as_str() {
            Some(text) => format!(" {key}={text}"),
            None => format!(" {key}={value}"),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A log directory holding `lines` as one file.
    fn fixture(lines: &[&str]) -> TempDir {
        let directory = tempfile::tempdir().unwrap();
        let file = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-07"));
        std::fs::write(&file, lines.join("\n")).unwrap();
        directory
    }

    fn operation(name: &str, busy: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-09-07T14:14:17.283200Z","level":"INFO","message":"close","time.busy":"{busy}","target":"nohrs::op","span":{{"query":"hello","name":"{name}"}}}}"#
        )
    }

    fn event(message: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-09-07T14:14:18.000000Z","level":"WARN","message":"{message}","target":"nohrs_services"}}"#
        )
    }

    fn run(directory: &Path, command: &Command) -> (String, Summary) {
        let mut output = Vec::new();
        let summary = Session::new(directory, &mut output).run(command).unwrap();
        (String::from_utf8(output).unwrap(), summary)
    }

    #[test]
    fn the_hand_written_default_matches_what_clap_parses() {
        // Guards the trap the derive would reintroduce: a `lines` of 0 prints
        // nothing, and every direct constructor would silently get it.
        assert_eq!(ShowArgs::default().lines, DEFAULT_LINES);
        const { assert!(DEFAULT_LINES > 0) };
    }

    #[test]
    fn show_renders_an_operation_with_its_duration_and_fields() {
        let directory = fixture(&[&operation("search.query", "12.2ms")]);

        let (out, summary) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert!(out.contains("14:14:17"), "{out}");
        assert!(out.contains("search.query"), "{out}");
        assert!(out.contains("12.2ms"), "{out}");
        assert!(out.contains("query=hello"), "{out}");
        assert_eq!(summary.printed, 1);
        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn show_renders_an_ordinary_event_as_level_target_message() {
        let directory = fixture(&[&event("could not open the ledger")]);

        let (out, _) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert!(out.contains("WARN"), "{out}");
        assert!(out.contains("nohrs_services"), "{out}");
        assert!(out.contains("could not open the ledger"), "{out}");
    }

    #[test]
    fn ops_keeps_only_the_measured_operations() {
        let directory = fixture(&[
            &event("noise"),
            &operation("fs.move", "3ms"),
            &event("more noise"),
        ]);

        let args = ShowArgs {
            ops: true,
            ..ShowArgs::default()
        };
        let (out, summary) = run(directory.path(), &Command::Show(args));

        assert_eq!(summary.printed, 1, "{out}");
        assert!(out.contains("fs.move"), "{out}");
        assert!(!out.contains("noise"), "{out}");
    }

    #[test]
    fn lines_keeps_the_most_recent_records() {
        let directory = fixture(&[
            &operation("first", "1ms"),
            &operation("second", "2ms"),
            &operation("third", "3ms"),
        ]);

        let args = ShowArgs {
            lines: 2,
            ..ShowArgs::default()
        };
        let (out, summary) = run(directory.path(), &Command::Show(args));

        assert_eq!(summary.printed, 2);
        assert!(
            !out.contains("first"),
            "the oldest should be dropped: {out}"
        );
        assert!(out.contains("second") && out.contains("third"), "{out}");
    }

    #[test]
    fn a_half_written_line_is_skipped_rather_than_fatal() {
        // The file is appended to by a live process, so the last line can be
        // truncated mid-write when we read it.
        let directory = fixture(&[&operation("fs.copy", "5ms"), r#"{"timestamp":"2026-09-0"#]);

        let (out, summary) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert_eq!(summary.printed, 1, "{out}");
        assert!(out.contains("fs.copy"), "{out}");
    }

    #[test]
    fn json_prints_the_line_untouched() {
        let line = operation("index.build_home", "1.5s");
        let directory = fixture(&[&line]);

        let args = ShowArgs {
            json: true,
            ..ShowArgs::default()
        };
        let (out, _) = run(directory.path(), &Command::Show(args));

        assert_eq!(out.trim(), line);
    }

    #[test]
    fn records_are_read_across_a_rotation_in_date_order() {
        let directory = tempfile::tempdir().unwrap();
        // Written newest-first to prove the ordering comes from the name, not
        // from the order the directory happens to enumerate.
        for (date, name) in [("2026-09-08", "newer"), ("2026-09-07", "older")] {
            let file = directory.path().join(format!("{LOG_FILE_PREFIX}.{date}"));
            std::fs::write(&file, operation(name, "1ms")).unwrap();
        }

        let (out, summary) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert_eq!(summary.printed, 2);
        let older = out.find("older").expect("older record");
        let newer = out.find("newer").expect("newer record");
        assert!(older < newer, "records should read oldest first: {out}");
    }

    #[test]
    fn an_empty_directory_says_so_without_failing() {
        let directory = tempfile::tempdir().unwrap();

        let (out, summary) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert!(out.contains("no log files"), "{out}");
        assert_eq!(summary.exit_code(), 0, "a fresh install is not an error");
    }

    #[test]
    fn a_missing_directory_is_reported_like_an_empty_one() {
        let directory = tempfile::tempdir().unwrap();
        let absent = directory.path().join("never-created");

        let (out, summary) = run(&absent, &Command::Show(ShowArgs::default()));

        assert!(out.contains("no log files"), "{out}");
        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn path_names_the_directory_and_its_files() {
        let directory = fixture(&[&event("x")]);

        let (out, _) = run(directory.path(), &Command::Path);

        assert!(
            out.contains(&directory.path().display().to_string()),
            "{out}"
        );
        assert!(out.contains(LOG_FILE_PREFIX), "{out}");
    }

    #[test]
    fn clear_refuses_without_force_and_deletes_with_it() {
        let directory = fixture(&[&event("x")]);
        let file = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-07"));

        let (out, summary) = run(directory.path(), &Command::Clear { force: false });
        assert!(out.contains("--force"), "{out}");
        assert_eq!(summary.removed, 0);
        assert!(file.exists(), "nothing should be deleted without --force");

        let (out, summary) = run(directory.path(), &Command::Clear { force: true });
        assert_eq!(summary.removed, 1, "{out}");
        assert!(!file.exists());
    }

    #[test]
    fn clear_on_an_empty_directory_says_so() {
        let directory = tempfile::tempdir().unwrap();

        let (out, summary) = run(directory.path(), &Command::Clear { force: true });

        assert!(out.contains("no log files"), "{out}");
        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn unrelated_files_in_the_directory_are_left_alone() {
        let directory = fixture(&[&event("x")]);
        let stray = directory.path().join("notes.txt");
        std::fs::write(&stray, "not a log").unwrap();

        let (_, summary) = run(directory.path(), &Command::Clear { force: true });

        assert_eq!(summary.removed, 1, "only the log file should be removed");
        assert!(stray.exists(), "an unrelated file must survive");
    }
}
