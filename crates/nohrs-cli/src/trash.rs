//! `noh trash` and `noh restore` — the recoverable half of `noh rm`.
//!
//! Standing in front of `/bin/rm` is only worth doing if what it trashes can
//! come back, so these commands list the trash, put items back where they came
//! from, and (deliberately, explicitly) empty it.
//!
//! The trash itself lives behind [`nohrs_services::fs::trash::Store`], which is
//! what makes the platform difference — Linux and Windows have an OS trash
//! index, macOS does not — invisible here.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use nohrs_core::errors::Error;
use nohrs_services::fs::trash::{Item, Store};

use crate::rm::Confirm;

/// The `noh trash` subcommands.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// List what is in the trash, most recently deleted first.
    List(ListArgs),
    /// Permanently delete items from the trash. This cannot be undone.
    Purge(PurgeArgs),
    /// Permanently delete everything in the trash. This cannot be undone.
    Empty(EmptyArgs),
}

/// Flags for `noh trash list`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct ListArgs {
    /// Show whether each item is a file or a directory.
    #[arg(short, long)]
    pub long: bool,

    /// Print the listing as JSON, one object per line.
    #[arg(long)]
    pub json: bool,

    /// Only list items trashed longer ago than this (e.g. `30d`, `12h`).
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub older_than: Option<Duration>,
}

/// Operands and flags for `noh trash purge`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct PurgeArgs {
    /// Items to purge, named by their original path or their file name.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Purge every item in the trash.
    #[arg(long)]
    pub all: bool,

    /// Only purge items trashed longer ago than this (e.g. `30d`, `12h`).
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub older_than: Option<Duration>,

    /// Do not prompt, and do not complain about operands that match nothing.
    #[arg(short, long)]
    pub force: bool,

    /// Report every purged item on stdout.
    #[arg(short, long)]
    pub verbose: bool,
}

/// Flags for `noh trash empty`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct EmptyArgs {
    /// Do not prompt before deleting.
    #[arg(short, long)]
    pub force: bool,

    /// Report every purged item on stdout.
    #[arg(short, long)]
    pub verbose: bool,
}

impl EmptyArgs {
    /// `empty` is `purge --all` under a name that says what it does.
    pub fn as_purge(&self) -> PurgeArgs {
        PurgeArgs {
            all: true,
            force: self.force,
            verbose: self.verbose,
            ..PurgeArgs::default()
        }
    }
}

/// Operands and flags for `noh restore`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct RestoreArgs {
    /// Items to restore, named by their original path or their file name. With
    /// no operands, the most recently trashed item comes back.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Restore every match rather than only the most recent one; with no
    /// operands, restore everything in the trash.
    #[arg(long)]
    pub all: bool,

    /// Only consider items trashed within this window (e.g. `2h`, `7d`).
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub since: Option<Duration>,

    /// Prompt before restoring each item.
    #[arg(short, long)]
    pub interactive: bool,

    /// Do not complain about operands that match nothing.
    #[arg(short, long)]
    pub force: bool,
}

/// What a run of one of the [`Session`] commands did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Items put back at their original location.
    pub restored: usize,
    /// Items deleted from the trash for good.
    pub purged: usize,
    /// Items printed by a listing.
    pub listed: usize,
    /// Items left alone because a prompt was declined.
    pub skipped: usize,
    /// Operands that matched nothing, and items that could not be acted on.
    pub failed: usize,
}

impl Summary {
    /// The process exit code for this run: `1` if anything failed, else `0`.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed > 0)
    }
}

/// One `noh trash` or `noh restore` invocation: the trash store plus the prompt
/// source and output streams it drives.
pub struct Session<'a> {
    store: &'a mut dyn Store,
    confirm: &'a mut dyn Confirm,
    output: &'a mut dyn Write,
    errors: &'a mut dyn Write,
    summary: Summary,
    now_unix: i64,
}

impl<'a> Session<'a> {
    /// Assemble a session against the real clock.
    pub fn new(
        store: &'a mut dyn Store,
        confirm: &'a mut dyn Confirm,
        output: &'a mut dyn Write,
        errors: &'a mut dyn Write,
    ) -> Self {
        Self {
            store,
            confirm,
            output,
            errors,
            summary: Summary::default(),
            now_unix: now_unix(),
        }
    }

    /// Pin the session's idea of "now", so that age filters and the relative
    /// timestamps in a listing are reproducible under test.
    pub fn at(mut self, now_unix: i64) -> Self {
        self.now_unix = now_unix;
        self
    }

    /// Print what is in the trash.
    pub fn list(mut self, args: &ListArgs) -> io::Result<Summary> {
        let mut items = match self.store.list() {
            Ok(items) => items,
            Err(error) => return self.abort(&error),
        };
        items.sort_by(|left, right| right.deleted_at_unix.cmp(&left.deleted_at_unix));
        if let Some(older_than) = args.older_than {
            let cutoff = self.now_unix - seconds_of(older_than);
            items.retain(|item| item.deleted_at_unix <= cutoff);
        }
        self.summary.listed = items.len();

        if args.json {
            for item in &items {
                writeln!(self.output, "{}", json_line(item))?;
            }
            return Ok(self.summary);
        }
        if items.is_empty() {
            writeln!(self.output, "the trash is empty")?;
            return Ok(self.summary);
        }
        for item in &items {
            let age = humanize_age(self.now_unix - item.deleted_at_unix);
            if args.long {
                let kind = if item.is_dir { "dir " } else { "file" };
                writeln!(
                    self.output,
                    "{age:<10} {kind}  {}",
                    item.original_path.display()
                )?;
            } else {
                writeln!(self.output, "{age:<10} {}", item.original_path.display())?;
            }
        }
        Ok(self.summary)
    }

    /// Delete items from the trash for good.
    pub fn purge(mut self, args: &PurgeArgs) -> io::Result<Summary> {
        if args.paths.is_empty() && !args.all && args.older_than.is_none() {
            self.summary.failed += 1;
            writeln!(
                self.errors,
                "noh trash purge: missing operand (pass --all to purge everything)"
            )?;
            return Ok(self.summary);
        }
        let items = match self.store.list() {
            Ok(items) => items,
            Err(error) => return self.abort(&error),
        };
        let selected = self.select(
            &items,
            &args.paths,
            args.all,
            args.older_than,
            None,
            args.force,
        )?;

        for item in selected {
            if !args.force {
                let question = format!(
                    "permanently delete {} from the trash? ",
                    item.original_path.display()
                );
                match self.confirm.confirm(&question) {
                    Ok(true) => {}
                    Ok(false) => {
                        self.summary.skipped += 1;
                        continue;
                    }
                    Err(error) => {
                        self.fail(&item.original_path, &error.to_string())?;
                        continue;
                    }
                }
            }
            match self.store.purge(&item) {
                Ok(()) => {
                    self.summary.purged += 1;
                    if args.verbose {
                        writeln!(self.output, "purged {}", item.original_path.display())?;
                    }
                }
                Err(error) => self.fail(&item.original_path, &crate::message(&error))?,
            }
        }
        Ok(self.summary)
    }

    /// Put items back where they came from.
    pub fn restore(mut self, args: &RestoreArgs) -> io::Result<Summary> {
        let items = match self.store.list() {
            Ok(items) => items,
            Err(error) => return self.abort(&error),
        };
        // With no operands and no `--all`, the useful default is the one that
        // undoes the mistake the user has just made.
        let newest_only = args.paths.is_empty() && !args.all;
        let since = args.since.map(|since| self.now_unix - seconds_of(since));
        let selected = self.select(&items, &args.paths, args.all, None, since, args.force)?;
        let selected: Vec<Item> = if newest_only {
            selected.into_iter().take(1).collect()
        } else {
            selected
        };
        if selected.is_empty() && args.paths.is_empty() && !args.force {
            self.summary.failed += 1;
            writeln!(self.errors, "noh restore: the trash is empty")?;
            return Ok(self.summary);
        }

        for item in selected {
            if args.interactive {
                let question = format!("restore {}? ", item.original_path.display());
                match self.confirm.confirm(&question) {
                    Ok(true) => {}
                    Ok(false) => {
                        self.summary.skipped += 1;
                        continue;
                    }
                    Err(error) => {
                        self.fail(&item.original_path, &error.to_string())?;
                        continue;
                    }
                }
            }
            match self.store.restore(&item) {
                Ok(()) => {
                    self.summary.restored += 1;
                    // Always reported, `--verbose` or not: a restore puts a file
                    // back on disk and the user needs to know which one.
                    writeln!(self.output, "restored {}", item.original_path.display())?;
                }
                Err(error) => self.fail(&item.original_path, &crate::message(&error))?,
            }
        }
        Ok(self.summary)
    }

    /// The items the operands name, most recently trashed first.
    ///
    /// An operand matches by original path, or — when it is a bare name — by
    /// file name, so `noh restore notes.txt` works without retyping the
    /// directory it was deleted from. With no operands the whole (filtered)
    /// trash is selected.
    fn select(
        &mut self,
        items: &[Item],
        operands: &[PathBuf],
        all: bool,
        older_than: Option<Duration>,
        since: Option<i64>,
        force: bool,
    ) -> io::Result<Vec<Item>> {
        let mut pool: Vec<Item> = items.to_vec();
        pool.sort_by(|left, right| right.deleted_at_unix.cmp(&left.deleted_at_unix));
        if let Some(older_than) = older_than {
            let cutoff = self.now_unix - seconds_of(older_than);
            pool.retain(|item| item.deleted_at_unix <= cutoff);
        }
        if let Some(since) = since {
            pool.retain(|item| item.deleted_at_unix >= since);
        }
        if operands.is_empty() {
            return Ok(pool);
        }

        let mut selected: Vec<Item> = Vec::new();
        for operand in operands {
            let matches: Vec<&Item> = pool.iter().filter(|item| names(item, operand)).collect();
            match matches.split_first() {
                None => {
                    if !force {
                        self.summary.failed += 1;
                        writeln!(
                            self.errors,
                            "noh trash: {}: no such item in the trash",
                            operand.display()
                        )?;
                    }
                }
                Some((newest, older)) => {
                    if all {
                        selected.extend(matches.iter().map(|item| (*item).clone()));
                    } else {
                        selected.push((*newest).clone());
                        if !older.is_empty() {
                            writeln!(
                                self.errors,
                                "noh trash: {}: {} older {} in the trash too (pass --all for all of them)",
                                operand.display(),
                                older.len(),
                                if older.len() == 1 {
                                    "copy is"
                                } else {
                                    "copies are"
                                }
                            )?;
                        }
                    }
                }
            }
        }
        // One item can be named by two operands (its path and its bare name).
        let mut seen = HashSet::new();
        selected.retain(|item| seen.insert(item.id.clone()));
        Ok(selected)
    }

    fn fail(&mut self, path: &Path, message: &str) -> io::Result<()> {
        self.summary.failed += 1;
        writeln!(self.errors, "noh trash: {}: {message}", path.display())
    }

    /// The trash itself could not be read, so there is nothing to iterate over.
    fn abort(mut self, error: &Error) -> io::Result<Summary> {
        self.summary.failed += 1;
        writeln!(self.errors, "noh trash: {}", crate::message(error))?;
        Ok(self.summary)
    }
}

/// Whether `operand` names `item`: either its full original path, or its bare
/// file name.
fn names(item: &Item, operand: &Path) -> bool {
    if operand.is_absolute() || operand.components().count() > 1 {
        return std::path::absolute(operand).is_ok_and(|operand| operand == item.original_path);
    }
    operand.as_os_str() == item.file_name().as_str()
}

fn json_line(item: &Item) -> String {
    serde_json::json!({
        "id": item.id,
        "original_path": item.original_path,
        "deleted_at_unix": item.deleted_at_unix,
        "is_dir": item.is_dir,
    })
    .to_string()
}

/// A relative age, which sidesteps rendering a wall-clock time in the viewer's
/// timezone; `--json` carries the exact epoch seconds for scripts.
fn humanize_age(seconds: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    match seconds {
        // A clock that moved backwards must not print a negative age.
        seconds if seconds < MINUTE => "just now".to_string(),
        seconds if seconds < HOUR => format!("{}m ago", seconds / MINUTE),
        seconds if seconds < DAY => format!("{}h ago", seconds / HOUR),
        seconds if seconds < WEEK => format!("{}d ago", seconds / DAY),
        seconds => format!("{}w ago", seconds / WEEK),
    }
}

fn seconds_of(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or_default()
}

/// Parse an age like `45s`, `30m`, `12h`, `7d`, or `2w`.
fn parse_duration(text: &str) -> std::result::Result<Duration, String> {
    let split = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    let amount: u64 = digits
        .parse()
        .map_err(|_| format!("expected a number with a unit, e.g. 30d (got {text:?})"))?;
    let seconds: u64 = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        "w" => 7 * 24 * 60 * 60,
        "" => return Err(format!("{text:?} needs a unit: s, m, h, d, or w")),
        other => return Err(format!("unknown unit {other:?}: use s, m, h, d, or w")),
    };
    amount
        .checked_mul(seconds)
        .map(Duration::from_secs)
        .ok_or_else(|| format!("{text:?} is too large"))
}

#[cfg(test)]
// The fixtures build real trash directories, so they need the synchronous
// filesystem calls that app code routes through `nohrs-services` instead.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::fs;

    use nohrs_core::errors::Result;
    use nohrs_services::fs::trash::LedgerStore;
    use nohrs_services::fs::trash_ledger::{TrashLedger, TrashRecord};
    use tempfile::{TempDir, tempdir};

    use super::*;

    /// A trash directory and its ledger, wired together the way `noh rm` leaves
    /// them. The commands are exercised against the real [`LedgerStore`] so the
    /// tests cover the platform path macOS actually takes.
    struct Fixture {
        home: TempDir,
        ledger: TrashLedger,
        trash_dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let home = tempdir().unwrap();
            let trash_dir = home.path().join("Trash");
            fs::create_dir(&trash_dir).unwrap();
            let ledger = TrashLedger::at(home.path().join("ledger.jsonl"));
            Self {
                home,
                ledger,
                trash_dir,
            }
        }

        fn trash_file(&self, name: &str, contents: &str) -> PathBuf {
            let origin = self.home.path().join("work").join(name);
            fs::create_dir_all(origin.parent().unwrap()).unwrap();
            fs::write(&origin, contents).unwrap();
            let record = TrashRecord::capture(&origin).unwrap();
            fs::rename(&origin, self.trash_dir.join(name)).unwrap();
            self.ledger.append(&record).unwrap();
            origin
        }

        fn store(&self) -> LedgerStore {
            LedgerStore::new(self.ledger.clone(), self.trash_dir.clone())
        }
    }

    /// A store that fails everything, for the paths where the trash itself is
    /// unreadable.
    struct BrokenStore;

    impl Store for BrokenStore {
        fn list(&mut self) -> Result<Vec<Item>> {
            Err(Error::Other("the trash is on fire".to_string()))
        }
        fn restore(&mut self, _item: &Item) -> Result<()> {
            Err(Error::Other("the trash is on fire".to_string()))
        }
        fn purge(&mut self, _item: &Item) -> Result<()> {
            Err(Error::Other("the trash is on fire".to_string()))
        }
    }

    #[derive(Default)]
    struct ScriptedConfirm {
        answers: Vec<bool>,
        questions: Vec<String>,
    }

    impl ScriptedConfirm {
        fn new(answers: &[bool]) -> Self {
            Self {
                // Answers are popped from the back, so store them reversed.
                answers: answers.iter().rev().copied().collect(),
                questions: Vec::new(),
            }
        }
    }

    impl Confirm for ScriptedConfirm {
        fn confirm(&mut self, question: &str) -> io::Result<bool> {
            self.questions.push(question.to_string());
            Ok(self.answers.pop().unwrap_or(false))
        }
    }

    struct Run {
        summary: Summary,
        stdout: String,
        stderr: String,
        questions: Vec<String>,
    }

    fn execute_at<F>(store: &mut dyn Store, answers: &[bool], now_unix: i64, command: F) -> Run
    where
        F: FnOnce(Session<'_>) -> io::Result<Summary>,
    {
        let mut confirm = ScriptedConfirm::new(answers);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let summary = {
            let session = Session::new(store, &mut confirm, &mut stdout, &mut stderr).at(now_unix);
            command(session).unwrap()
        };
        Run {
            summary,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
            questions: confirm.questions,
        }
    }

    fn execute<F>(store: &mut dyn Store, answers: &[bool], command: F) -> Run
    where
        F: FnOnce(Session<'_>) -> io::Result<Summary>,
    {
        execute_at(store, answers, now_unix(), command)
    }

    #[test]
    fn a_listing_shows_the_original_path_and_a_relative_age() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let run = execute(&mut store, &[], |session| {
            session.list(&ListArgs::default())
        });

        assert_eq!(run.summary.listed, 1);
        let line = run.stdout.trim_end();
        assert!(line.starts_with("just now"), "unexpected listing: {line}");
        assert!(
            line.ends_with(&origin.display().to_string()),
            "unexpected listing: {line}"
        );
    }

    #[test]
    fn an_empty_trash_says_so() {
        let fixture = Fixture::new();
        let mut store = fixture.store();

        let run = execute(&mut store, &[], |session| {
            session.list(&ListArgs::default())
        });

        assert_eq!(run.stdout, "the trash is empty\n");
        assert_eq!(run.summary.exit_code(), 0);
    }

    #[test]
    fn the_long_listing_names_the_kind_of_each_item() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let directory = fixture.home.path().join("work").join("project");
        fs::create_dir_all(&directory).unwrap();
        let record = TrashRecord::capture(&directory).unwrap();
        fs::rename(&directory, fixture.trash_dir.join("project")).unwrap();
        fixture.ledger.append(&record).unwrap();
        let mut store = fixture.store();

        let args = ListArgs {
            long: true,
            ..ListArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.list(&args));

        assert!(run.stdout.contains("file  "), "listing: {}", run.stdout);
        assert!(run.stdout.contains("dir   "), "listing: {}", run.stdout);
    }

    #[test]
    fn the_json_listing_carries_the_exact_timestamp() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let args = ListArgs {
            json: true,
            ..ListArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.list(&args));

        let value: serde_json::Value = serde_json::from_str(run.stdout.trim()).unwrap();
        assert_eq!(value["original_path"], origin.display().to_string());
        assert!(value["deleted_at_unix"].as_i64().unwrap() > 0);
        assert_eq!(value["is_dir"], false);
    }

    #[test]
    fn restore_without_operands_takes_only_the_most_recent_item() {
        let fixture = Fixture::new();
        let older = fixture.trash_file("older.txt", "old");
        let newer = fixture.trash_file("newer.txt", "new");
        let mut store = fixture.store();

        let run = execute(&mut store, &[], |session| {
            session.restore(&RestoreArgs::default())
        });

        assert_eq!(run.summary.restored, 1);
        assert_eq!(run.stdout, format!("restored {}\n", newer.display()));
        assert!(newer.exists(), "the newest deletion comes back first");
        assert!(!older.exists());
    }

    #[test]
    fn restore_all_without_operands_empties_the_trash_back_out() {
        let fixture = Fixture::new();
        let first = fixture.trash_file("a.txt", "a");
        let second = fixture.trash_file("b.txt", "b");
        let mut store = fixture.store();

        let args = RestoreArgs {
            all: true,
            ..RestoreArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.restore(&args));

        assert_eq!(run.summary.restored, 2);
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn an_operand_selects_by_bare_name_or_by_full_path() {
        for use_full_path in [false, true] {
            let fixture = Fixture::new();
            let origin = fixture.trash_file("notes.txt", "payload");
            let other = fixture.trash_file("other.txt", "other");
            let mut store = fixture.store();

            let operand = if use_full_path {
                origin.clone()
            } else {
                PathBuf::from("notes.txt")
            };
            let args = RestoreArgs {
                paths: vec![operand],
                ..RestoreArgs::default()
            };
            let run = execute(&mut store, &[], |session| session.restore(&args));

            assert_eq!(run.summary.restored, 1, "stderr: {}", run.stderr);
            assert!(origin.exists());
            assert!(!other.exists(), "only the named item comes back");
        }
    }

    #[test]
    fn an_operand_that_matches_nothing_is_reported_unless_forced() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let args = RestoreArgs {
            paths: vec![PathBuf::from("ghost.txt")],
            ..RestoreArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.restore(&args));

        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.summary.exit_code(), 1);
        assert!(
            run.stderr.contains("no such item in the trash"),
            "stderr: {}",
            run.stderr
        );

        let args = RestoreArgs {
            force: true,
            ..args
        };
        let run = execute(&mut store, &[], |session| session.restore(&args));
        assert_eq!(run.summary.exit_code(), 0);
        assert!(run.stderr.is_empty());
    }

    #[test]
    fn restoring_from_an_empty_trash_is_an_error() {
        let fixture = Fixture::new();
        let mut store = fixture.store();

        let run = execute(&mut store, &[], |session| {
            session.restore(&RestoreArgs::default())
        });

        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.stderr, "noh restore: the trash is empty\n");
    }

    #[test]
    fn restore_points_at_all_when_several_copies_match() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "one");
        // A second file of the same name, which the trash had to rename.
        let origin = fixture.home.path().join("work").join("notes.txt");
        fs::write(&origin, "two!").unwrap();
        let record = TrashRecord::capture(&origin).unwrap();
        fs::rename(&origin, fixture.trash_dir.join("notes 2.txt")).unwrap();
        fixture.ledger.append(&record).unwrap();
        let mut store = fixture.store();

        let args = RestoreArgs {
            paths: vec![PathBuf::from("notes.txt")],
            ..RestoreArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.restore(&args));

        assert_eq!(run.summary.restored, 1);
        assert!(
            run.stderr.contains("older copy is in the trash too"),
            "stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn restore_since_ignores_older_deletions() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        // A clock a week ahead puts the deletion outside a one-hour window.
        let args = RestoreArgs {
            all: true,
            since: Some(Duration::from_secs(60 * 60)),
            ..RestoreArgs::default()
        };
        let run = execute_at(&mut store, &[], now_unix() + 7 * 24 * 60 * 60, |session| {
            session.restore(&args)
        });

        assert_eq!(run.summary.restored, 0);
    }

    #[test]
    fn restore_interactive_honours_a_refusal() {
        let fixture = Fixture::new();
        let origin = fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let args = RestoreArgs {
            interactive: true,
            ..RestoreArgs::default()
        };
        let run = execute(&mut store, &[false], |session| session.restore(&args));

        assert_eq!(run.summary.skipped, 1);
        assert_eq!(run.summary.restored, 0);
        assert!(!origin.exists());
        assert!(
            run.questions[0].starts_with("restore "),
            "{:?}",
            run.questions
        );
    }

    #[test]
    fn a_failed_restore_is_reported_and_the_run_continues() {
        let fixture = Fixture::new();
        let first = fixture.trash_file("a.txt", "a");
        fixture.trash_file("b.txt", "b");
        // Something is at the first item's original location again, so only it
        // can fail.
        fs::write(&first, "newer").unwrap();
        let mut store = fixture.store();

        let args = RestoreArgs {
            all: true,
            ..RestoreArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.restore(&args));

        assert_eq!(run.summary.restored, 1);
        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.summary.exit_code(), 1);
        assert!(
            run.stderr
                .contains("something else is at the original location"),
            "stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn purge_asks_before_deleting_and_honours_a_refusal() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let args = PurgeArgs {
            all: true,
            ..PurgeArgs::default()
        };
        let run = execute(&mut store, &[false], |session| session.purge(&args));

        assert_eq!(run.summary.skipped, 1);
        assert_eq!(run.summary.purged, 0);
        assert!(fixture.trash_dir.join("notes.txt").exists());
        assert!(
            run.questions[0].contains("permanently delete"),
            "{:?}",
            run.questions
        );
    }

    #[test]
    fn purge_force_deletes_from_the_trash_for_good() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let mut store = fixture.store();

        let args = PurgeArgs {
            all: true,
            force: true,
            verbose: true,
            ..PurgeArgs::default()
        };
        let run = execute(&mut store, &[], |session| session.purge(&args));

        assert_eq!(run.summary.purged, 1);
        assert!(run.stdout.starts_with("purged "), "{}", run.stdout);
        assert!(run.questions.is_empty(), "--force must not prompt");
        assert!(!fixture.trash_dir.join("notes.txt").exists());
        assert!(fixture.ledger.records().unwrap().is_empty());
    }

    #[test]
    fn purge_without_a_selection_is_an_error() {
        let fixture = Fixture::new();
        let mut store = fixture.store();

        let run = execute(&mut store, &[], |session| {
            session.purge(&PurgeArgs::default())
        });

        assert_eq!(run.summary.failed, 1);
        assert!(run.stderr.contains("missing operand"), "{}", run.stderr);
    }

    #[test]
    fn purge_older_than_selects_only_stale_items() {
        let fixture = Fixture::new();
        fixture.trash_file("notes.txt", "payload");
        let args = PurgeArgs {
            older_than: Some(Duration::from_secs(24 * 60 * 60)),
            force: true,
            ..PurgeArgs::default()
        };

        let mut store = fixture.store();
        let run = execute(&mut store, &[], |session| session.purge(&args));
        assert_eq!(run.summary.purged, 0, "a fresh item is not stale");

        let mut store = fixture.store();
        let run = execute_at(&mut store, &[], now_unix() + 7 * 24 * 60 * 60, |session| {
            session.purge(&args)
        });
        assert_eq!(run.summary.purged, 1);
    }

    #[test]
    fn empty_is_purge_all() {
        let purge = EmptyArgs {
            force: true,
            verbose: true,
        }
        .as_purge();

        assert!(purge.all && purge.force && purge.verbose);
        assert!(purge.paths.is_empty());
    }

    #[test]
    fn an_unreadable_trash_is_reported_once() {
        let run = execute(&mut BrokenStore, &[], |session| {
            session.list(&ListArgs::default())
        });

        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.summary.exit_code(), 1);
        assert!(
            run.stderr.contains("the trash is on fire"),
            "{}",
            run.stderr
        );

        let run = execute(&mut BrokenStore, &[], |session| {
            session.restore(&RestoreArgs::default())
        });
        assert_eq!(run.summary.exit_code(), 1);

        let args = PurgeArgs {
            all: true,
            ..PurgeArgs::default()
        };
        let run = execute(&mut BrokenStore, &[], |session| session.purge(&args));
        assert_eq!(run.summary.exit_code(), 1);
    }

    #[test]
    fn ages_are_rendered_relative_to_now() {
        assert_eq!(humanize_age(0), "just now");
        assert_eq!(humanize_age(-5), "just now");
        assert_eq!(humanize_age(90), "1m ago");
        assert_eq!(humanize_age(2 * 60 * 60), "2h ago");
        assert_eq!(humanize_age(3 * 24 * 60 * 60), "3d ago");
        assert_eq!(humanize_age(3 * 7 * 24 * 60 * 60), "3w ago");
    }

    #[test]
    fn durations_parse_with_a_unit_and_only_with_a_unit() {
        assert_eq!(parse_duration("45s").unwrap(), Duration::from_secs(45));
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(1_800));
        assert_eq!(parse_duration("12h").unwrap(), Duration::from_secs(43_200));
        assert_eq!(parse_duration("7d").unwrap(), Duration::from_secs(604_800));
        assert_eq!(
            parse_duration("2w").unwrap(),
            Duration::from_secs(1_209_600)
        );
        for bad in ["30", "d", "", "5y", "-1d", "99999999999999999999d"] {
            assert!(parse_duration(bad).is_err(), "{bad:?} parsed");
        }
    }
}
