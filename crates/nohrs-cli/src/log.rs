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
use nohrs_core::telemetry::logging::{current_log_file_name, is_log_file_name};

/// What `noh log` was asked to do.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print the most recent records.
    Show(ShowArgs),
    /// Print where the log files live.
    Path,
    /// Empty the log files.
    Clear {
        /// Empty them without asking.
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
    /// Log files emptied by `clear`, whether unlinked or truncated in place.
    pub cleared: usize,
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
        let files = log_files(self.directory)?;
        if files.is_empty() {
            // Not a failure: a fresh install has never written one, and saying
            // so is the answer to the question that was asked. `--json` exists
            // to be piped into `jq`, though, where a prose line is a parse
            // error; an empty stream is how JSON Lines spells "no records".
            if !args.json {
                writeln!(self.output, "no log files in {}", self.directory.display())?;
            }
            return Ok(());
        }
        // Newest file last, so reading them in order and keeping the tail gives
        // the most recent records across a rotation boundary.
        let mut records = Vec::new();
        for file in &files {
            // Bytes rather than `read_to_string`: a live process is appending
            // to this file, so its tail can be a half-written multibyte
            // character. `read_to_string` rejects the *whole file* for that,
            // which would lose every record over one truncated character —
            // and only while nohrs is busy, which is when the log matters.
            let body = std::fs::read(file)?;
            let body = String::from_utf8_lossy(&body);
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
        for file in log_files(self.directory)? {
            writeln!(self.output, "  {}", file.display())?;
        }
        Ok(())
    }

    fn clear(&mut self, force: bool) -> io::Result<()> {
        let files = log_files(self.directory)?;
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
        let current = current_log_file_name();
        for file in files {
            let is_current = file.file_name().and_then(|name| name.to_str()) == current.as_deref();
            let outcome = if is_current {
                truncate(&file)
            } else {
                std::fs::remove_file(&file)
            };
            match outcome {
                Ok(()) => self.summary.cleared += 1,
                Err(error) => {
                    writeln!(self.output, "{}: {error}", file.display())?;
                    self.summary.failed = true;
                }
            }
        }
        writeln!(self.output, "cleared {} log file(s)", self.summary.cleared)?;
        Ok(())
    }
}

/// Empty a log file without unlinking it.
///
/// A running nohrs — the GUI, typically — holds today's file open and appends to
/// it. Unlinking that file leaves the writer appending to an inode with no name:
/// every record until the next rotation goes nowhere, and the user who cleared
/// the log sees it mysteriously stay empty. Truncation is visible to the writer
/// immediately, and its `O_APPEND` writes simply resume from zero.
fn truncate(file: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(file)
        .map(drop)
}

/// The log files in `directory`, oldest first.
///
/// `tracing-appender` names them `<prefix>.<date>`, so a lexicographic sort is
/// also chronological.
fn log_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        // A directory that was never created is the normal state of a fresh
        // install, and "no log files" is the honest answer. Every other error
        // is propagated: reporting a permission denial as "empty" would send
        // the user hunting for a missing file instead of a wrong mode, and
        // `clear` would cheerfully report success having removed nothing.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        // The name alone is not enough: a directory or a FIFO named like a log
        // file would be read as a record stream, and handed to `clear` to
        // unlink.
        if !entry.file_type()?.is_file() {
            continue;
        }
        if entry.file_name().to_str().is_some_and(is_log_file_name) {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
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
    use nohrs_core::telemetry::logging::LOG_FILE_PREFIX;
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
        assert_eq!(summary.cleared, 0);
        assert!(file.exists(), "nothing should be deleted without --force");

        let (out, summary) = run(directory.path(), &Command::Clear { force: true });
        assert_eq!(summary.cleared, 1, "{out}");
        assert!(!file.exists(), "a closed, rotated file is unlinked");
    }

    #[test]
    fn clear_truncates_the_file_a_live_process_is_appending_to() {
        // Unlinking it would leave a running GUI writing to a nameless inode:
        // its records would vanish until the next rotation, with the log
        // looking permanently empty to the user who cleared it.
        let directory = tempfile::tempdir().unwrap();
        let current = current_log_file_name().expect("today's file name");
        let live = directory.path().join(&current);
        std::fs::write(&live, format!("{}\n", event("live"))).unwrap();
        let rotated = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2020-01-01"));
        std::fs::write(&rotated, format!("{}\n", event("old"))).unwrap();

        let (out, summary) = run(directory.path(), &Command::Clear { force: true });

        assert_eq!(summary.cleared, 2, "{out}");
        assert!(!rotated.exists(), "a closed file is unlinked");
        assert!(live.exists(), "the open file must keep its name");
        assert_eq!(
            std::fs::metadata(&live).unwrap().len(),
            0,
            "the open file must be emptied"
        );
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
        // A hand-made copy kept beside the real ones. It shares the prefix but
        // not the appender's `<prefix>.<date>` shape, and `clear` deleting
        // someone's saved copy of a log is not what they asked for.
        let saved = directory.path().join(format!("{LOG_FILE_PREFIX}.backup"));
        std::fs::write(&saved, "kept on purpose").unwrap();

        let (_, summary) = run(directory.path(), &Command::Clear { force: true });

        assert_eq!(summary.cleared, 1, "only the log file should be removed");
        assert!(stray.exists(), "an unrelated file must survive");
        assert!(saved.exists(), "a hand-made copy must survive");
    }

    #[test]
    fn a_directory_named_like_a_log_file_is_not_treated_as_one() {
        // `clear` would try to unlink it, and `show` to read it as records.
        let directory = tempfile::tempdir().unwrap();
        let decoy = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-07"));
        std::fs::create_dir(&decoy).unwrap();

        let (out, summary) = run(directory.path(), &Command::Clear { force: true });

        assert!(out.contains("no log files"), "{out}");
        assert_eq!(summary.cleared, 0);
        assert!(decoy.is_dir(), "a directory must survive untouched");
    }

    #[test]
    fn a_tail_cut_mid_character_still_yields_the_records_before_it() {
        // The appender is writing while we read, so the last line can stop
        // inside a multibyte character. `read_to_string` would reject the whole
        // file for it and `show` would print nothing at all.
        let directory = tempfile::tempdir().unwrap();
        let file = directory
            .path()
            .join(format!("{LOG_FILE_PREFIX}.2026-09-07"));
        let mut bytes = format!("{}\n", operation("fs.copy", "5ms")).into_bytes();
        bytes.extend_from_slice(br#"{"message":""#);
        // The leading byte of a three-byte sequence, with its two continuation
        // bytes not yet written.
        bytes.push(0xE6);
        std::fs::write(&file, bytes).unwrap();

        let (out, summary) = run(directory.path(), &Command::Show(ShowArgs::default()));

        assert_eq!(summary.printed, 1, "{out}");
        assert!(out.contains("fs.copy"), "{out}");
    }

    #[test]
    fn json_on_an_empty_directory_prints_nothing_parseable_as_prose() {
        // `--json` is a pipe into `jq`; a prose line there is a parse error.
        let directory = tempfile::tempdir().unwrap();
        let args = ShowArgs {
            json: true,
            ..ShowArgs::default()
        };

        let (out, summary) = run(directory.path(), &Command::Show(args));

        assert!(out.is_empty(), "expected an empty stream, got {out:?}");
        assert_eq!(summary.exit_code(), 0);

        // The human mode still says it, because there is no parser there.
        let (out, _) = run(directory.path(), &Command::Show(ShowArgs::default()));
        assert!(out.contains("no log files"), "{out}");
    }

    #[test]
    fn a_directory_that_cannot_be_read_is_an_error_rather_than_reported_as_empty() {
        // Only `NotFound` means "no log files" — a fresh install. Anything else
        // reported as empty would send the user looking for a missing file
        // instead of the real fault, and `clear` would report success having
        // removed nothing. A file where the directory should be stands in for
        // the permission denial, which cannot be staged as root.
        let directory = tempfile::tempdir().unwrap();
        let occupied = directory.path().join("logs");
        std::fs::write(&occupied, "not a directory").unwrap();

        let mut output = Vec::new();
        let error = Session::new(&occupied, &mut output)
            .run(&Command::Path)
            .expect_err("an unreadable directory must not look empty");

        assert_ne!(error.kind(), io::ErrorKind::NotFound, "{error}");
    }
}
