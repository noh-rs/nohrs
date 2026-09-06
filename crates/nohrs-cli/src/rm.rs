//! `noh rm` — a drop-in replacement for `rm(1)` that moves its operands to
//! the trash instead of unlinking them.
//!
//! The flag surface mirrors POSIX `rm` (`-r`, `-d`, `-f`, `-i`, `-v`, `--`) so
//! the binary can shadow `/bin/rm` on `PATH`, with one addition: `--permanent`
//! opts back into a real, unrecoverable delete. `--force` keeps its POSIX
//! meaning (ignore missing operands, never prompt) and deliberately does *not*
//! imply a permanent delete, so an inherited `rm -rf` in a script still lands in
//! the trash.
//!
//! The destructive step goes through [`nohrs_services::fs::ops`] rather than
//! `std::fs`, so the CLI and the GUI share one implementation of "trash this"
//! (see `docs/explorer-essentials.md` §1.1).

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use nohrs_core::errors::Result;
use nohrs_services::fs::ops;

/// Operands and flags for the `rm` subcommand.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct Args {
    /// Files and directories to remove.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Remove directories and their contents recursively.
    #[arg(short = 'r', short_alias = 'R', long)]
    pub recursive: bool,

    /// Remove empty directories as well as files.
    #[arg(short = 'd', long = "dir")]
    pub directory: bool,

    /// Ignore operands that do not exist and never prompt.
    #[arg(short, long)]
    pub force: bool,

    /// Prompt before removing each operand.
    #[arg(short, long)]
    pub interactive: bool,

    /// Report every removal on stdout.
    #[arg(short, long)]
    pub verbose: bool,

    /// Delete permanently instead of moving to the trash. This cannot be undone.
    #[arg(short = 'P', long, alias = "no-trash")]
    pub permanent: bool,
}

/// What a run of [`Session::run`] did, one count per operand.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Operands moved to the trash.
    pub trashed: usize,
    /// Operands deleted permanently.
    pub deleted: usize,
    /// Operands left untouched: declined at the prompt, or missing under `--force`.
    pub skipped: usize,
    /// Operands that could not be removed.
    pub failed: usize,
}

impl Summary {
    /// The process exit code for this run: `1` if any operand failed, else `0`.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed > 0)
    }
}

/// The destructive half of `rm`, kept behind a trait so tests can assert on what
/// *would* be removed: headless CI has no desktop trash directory, so calling
/// the real [`ops::trash_path`] there is not an option.
pub trait Backend {
    /// Move `path` to the operating system's trash.
    fn trash(&mut self, path: &Path) -> Result<()>;
    /// Delete `path` permanently.
    fn delete(&mut self, path: &Path) -> Result<()>;
}

/// The [`Backend`] used in production, delegating to `nohrs-services`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsBackend;

impl Backend for OsBackend {
    fn trash(&mut self, path: &Path) -> Result<()> {
        ops::trash_path(path)
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        ops::delete_permanent(path)
    }
}

/// Source of the yes/no answers `--interactive` asks for.
pub trait Confirm {
    /// Present `question` and report whether the user agreed.
    fn confirm(&mut self, question: &str) -> io::Result<bool>;
}

/// Asks on stderr and reads the answer from stdin, the way `rm -i` does. Any
/// answer that does not start with `y`/`Y` — end of input included — declines.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdinConfirm;

impl Confirm for StdinConfirm {
    fn confirm(&mut self, question: &str) -> io::Result<bool> {
        let mut stderr = io::stderr();
        write!(stderr, "{question}")?;
        stderr.flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        Ok(answer.trim_start().starts_with(['y', 'Y']))
    }
}

/// One `rm` invocation: the parsed [`Args`] plus the effectful parts (removal
/// backend, prompt source, output streams) it drives.
pub struct Session<'a> {
    args: &'a Args,
    backend: &'a mut dyn Backend,
    confirm: &'a mut dyn Confirm,
    output: &'a mut dyn Write,
    errors: &'a mut dyn Write,
    summary: Summary,
}

impl<'a> Session<'a> {
    /// Assemble a session. `output` receives `--verbose` lines, `errors` the
    /// per-operand diagnostics.
    pub fn new(
        args: &'a Args,
        backend: &'a mut dyn Backend,
        confirm: &'a mut dyn Confirm,
        output: &'a mut dyn Write,
        errors: &'a mut dyn Write,
    ) -> Self {
        Self {
            args,
            backend,
            confirm,
            output,
            errors,
            summary: Summary::default(),
        }
    }

    /// Process every operand, continuing past failures, and report what
    /// happened. The `io::Error` case is a failure to write to `output` or
    /// `errors` (a closed pipe), not a failed removal.
    pub fn run(mut self) -> io::Result<Summary> {
        if self.args.paths.is_empty() {
            // POSIX: `rm -f` with no operands succeeds silently, so an unset
            // variable in a script does not abort it.
            if !self.args.force {
                self.summary.failed += 1;
                writeln!(self.errors, "noh rm: missing operand")?;
            }
            return Ok(self.summary);
        }
        for path in &self.args.paths {
            self.remove(path)?;
        }
        Ok(self.summary)
    }

    fn remove(&mut self, path: &Path) -> io::Result<()> {
        if ends_in_dot_segment(path) {
            return self.fail(path, "refusing to remove '.' or '..'");
        }
        // The trash backend resolves paths against the process working
        // directory; make that explicit here so the recorded original location
        // is right, and so the root guard below sees a full path. `absolute`
        // does not resolve symlinks, so a link is still removed as itself.
        let target = match std::path::absolute(path) {
            Ok(target) => target,
            Err(error) => return self.fail(path, &error.to_string()),
        };
        if target.parent().is_none() {
            return self.fail(path, "refusing to remove the root directory");
        }

        let metadata = match std::fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.args.force {
                    self.summary.skipped += 1;
                    return Ok(());
                }
                return self.fail(path, "no such file or directory");
            }
            Err(error) => return self.fail(path, &error.to_string()),
        };

        if metadata.is_dir() {
            if !self.args.recursive && !self.args.directory {
                return self.fail(path, "is a directory (pass -r to remove it)");
            }
            if !self.args.recursive {
                match is_empty_dir(&target) {
                    Ok(true) => {}
                    Ok(false) => return self.fail(path, "directory not empty (pass -r instead)"),
                    Err(error) => return self.fail(path, &error.to_string()),
                }
            }
        }

        // `--force` wins over `--interactive`, matching how a script that sets
        // `-f` expects to run unattended.
        if self.args.interactive && !self.args.force {
            let question = if self.args.permanent {
                format!("permanently delete {}? ", path.display())
            } else {
                format!("move {} to the trash? ", path.display())
            };
            match self.confirm.confirm(&question) {
                Ok(true) => {}
                Ok(false) => {
                    self.summary.skipped += 1;
                    return Ok(());
                }
                Err(error) => return self.fail(path, &error.to_string()),
            }
        }

        let outcome = if self.args.permanent {
            self.backend.delete(&target)
        } else {
            self.backend.trash(&target)
        };
        match outcome {
            Ok(()) => {
                if self.args.permanent {
                    self.summary.deleted += 1;
                } else {
                    self.summary.trashed += 1;
                }
                if self.args.verbose {
                    let verb = if self.args.permanent {
                        "deleted"
                    } else {
                        "trashed"
                    };
                    writeln!(self.output, "{verb} {}", path.display())?;
                }
                Ok(())
            }
            Err(error) => self.fail(path, &error.to_string()),
        }
    }

    fn fail(&mut self, path: &Path, message: &str) -> io::Result<()> {
        self.summary.failed += 1;
        writeln!(self.errors, "noh rm: {}: {message}", path.display())
    }
}

/// Whether the last segment of `path` is `.` or `..`, which POSIX requires `rm`
/// to reject. `Path::components` normalizes an interior `.` away (`foo/.` yields
/// just `foo`), so the raw text is inspected instead.
fn ends_in_dot_segment(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(std::path::is_separator);
    let segment = match trimmed.rfind(std::path::is_separator) {
        // Separators are single-byte ASCII, so this index is a char boundary.
        Some(index) => &trimmed[index + 1..],
        None => trimmed,
    };
    segment == "." || segment == ".."
}

fn is_empty_dir(path: &Path) -> io::Result<bool> {
    // A failure to read the first entry is reported as such: folding it into
    // `false` would surface as a misleading "directory not empty".
    match std::fs::read_dir(path)?.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(error)) => Err(error),
    }
}

#[cfg(test)]
// The fixtures build real directory trees, so they need the synchronous
// filesystem calls that app code routes through `nohrs-services` instead.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::fs;

    use tempfile::{TempDir, tempdir};

    use super::*;

    #[derive(Default)]
    struct RecordingBackend {
        trashed: Vec<PathBuf>,
        deleted: Vec<PathBuf>,
        error: Option<String>,
    }

    impl Backend for RecordingBackend {
        fn trash(&mut self, path: &Path) -> Result<()> {
            match &self.error {
                Some(message) => Err(nohrs_core::errors::Error::Other(message.clone())),
                None => {
                    self.trashed.push(path.to_path_buf());
                    Ok(())
                }
            }
        }

        fn delete(&mut self, path: &Path) -> Result<()> {
            match &self.error {
                Some(message) => Err(nohrs_core::errors::Error::Other(message.clone())),
                None => {
                    self.deleted.push(path.to_path_buf());
                    Ok(())
                }
            }
        }
    }

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
        backend: RecordingBackend,
        confirm: ScriptedConfirm,
        stdout: String,
        stderr: String,
    }

    fn execute_with(args: &Args, mut backend: RecordingBackend, answers: &[bool]) -> Run {
        let mut confirm = ScriptedConfirm::new(answers);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let summary = Session::new(args, &mut backend, &mut confirm, &mut stdout, &mut stderr)
            .run()
            .unwrap();
        Run {
            summary,
            backend,
            confirm,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
        }
    }

    fn execute(args: &Args) -> Run {
        execute_with(args, RecordingBackend::default(), &[])
    }

    fn args_for<I: IntoIterator<Item = PathBuf>>(paths: I) -> Args {
        Args {
            paths: paths.into_iter().collect(),
            ..Args::default()
        }
    }

    fn file_in(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, "payload").unwrap();
        path
    }

    #[test]
    fn a_file_goes_to_the_trash_by_default() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");

        let run = execute(&args_for([file.clone()]));

        assert_eq!(run.backend.trashed, vec![file]);
        assert!(run.backend.deleted.is_empty());
        assert_eq!(run.summary.trashed, 1);
        assert_eq!(run.summary.exit_code(), 0);
        assert!(run.stderr.is_empty());
    }

    #[test]
    fn permanent_deletes_instead_of_trashing() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            permanent: true,
            ..args_for([file.clone()])
        };

        let run = execute(&args);

        assert_eq!(run.backend.deleted, vec![file]);
        assert!(run.backend.trashed.is_empty());
        assert_eq!(run.summary.deleted, 1);
    }

    #[test]
    fn permanent_really_removes_the_file_through_the_os_backend() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            permanent: true,
            ..args_for([file.clone()])
        };
        let mut backend = OsBackend;
        let mut confirm = ScriptedConfirm::new(&[]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let summary = Session::new(&args, &mut backend, &mut confirm, &mut stdout, &mut stderr)
            .run()
            .unwrap();

        assert_eq!(summary.deleted, 1);
        assert!(!file.exists());
    }

    #[test]
    fn a_relative_operand_is_resolved_before_removal() {
        // Cargo runs unit tests with the package root as the working directory,
        // so this crate's own manifest is a relative operand that exists. The
        // recording backend never touches it.
        let run = execute(&args_for([PathBuf::from("Cargo.toml")]));

        let trashed = match run.backend.trashed.first() {
            Some(trashed) => trashed,
            None => panic!("nothing was trashed: {}", run.stderr),
        };
        assert!(
            trashed.is_absolute(),
            "expected an absolute path, got {}",
            trashed.display()
        );
        assert!(trashed.ends_with("Cargo.toml"));
    }

    #[test]
    fn a_directory_needs_recursive() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("project");
        fs::create_dir(&nested).unwrap();

        let run = execute(&args_for([nested.clone()]));

        assert!(run.backend.trashed.is_empty());
        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.summary.exit_code(), 1);
        assert!(
            run.stderr.contains("is a directory"),
            "unexpected stderr: {}",
            run.stderr
        );

        let args = Args {
            recursive: true,
            ..args_for([nested.clone()])
        };
        let run = execute(&args);
        assert_eq!(run.backend.trashed, vec![nested]);
    }

    #[test]
    fn dir_removes_only_empty_directories() {
        let dir = tempdir().unwrap();
        let empty = dir.path().join("empty");
        let full = dir.path().join("full");
        fs::create_dir(&empty).unwrap();
        fs::create_dir(&full).unwrap();
        fs::write(full.join("inner.txt"), "x").unwrap();

        let args = Args {
            directory: true,
            ..args_for([empty.clone(), full])
        };
        let run = execute(&args);

        assert_eq!(run.backend.trashed, vec![empty]);
        assert_eq!(run.summary.failed, 1);
        assert!(
            run.stderr.contains("directory not empty"),
            "unexpected stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn a_symlink_is_removed_without_recursive() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(not(unix))]
        fs::write(&link, "x").unwrap();

        let run = execute(&args_for([link.clone()]));

        assert_eq!(run.backend.trashed, vec![link]);
        assert!(target.is_dir(), "the link target is left alone");
    }

    #[test]
    fn a_missing_operand_fails_unless_forced() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("ghost.txt");

        let run = execute(&args_for([missing.clone()]));
        assert_eq!(run.summary.failed, 1);
        assert!(
            run.stderr.contains("no such file or directory"),
            "unexpected stderr: {}",
            run.stderr
        );

        let args = Args {
            force: true,
            ..args_for([missing])
        };
        let run = execute(&args);
        assert_eq!(run.summary.skipped, 1);
        assert_eq!(run.summary.exit_code(), 0);
        assert!(run.stderr.is_empty());
    }

    #[test]
    fn force_does_not_imply_a_permanent_delete() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            force: true,
            recursive: true,
            ..args_for([file.clone()])
        };

        let run = execute(&args);

        assert_eq!(run.backend.trashed, vec![file]);
        assert!(run.backend.deleted.is_empty());
    }

    #[test]
    fn dot_operands_are_refused() {
        for operand in [".", "..", "./", "../", "foo/.", "foo/.."] {
            let run = execute(&args_for([PathBuf::from(operand)]));
            assert_eq!(run.summary.failed, 1, "{operand} was not refused");
            assert!(
                run.stderr.contains("refusing to remove '.' or '..'"),
                "unexpected stderr for {operand}: {}",
                run.stderr
            );
        }
    }

    #[test]
    fn the_root_directory_is_refused() {
        let run = execute(&args_for([PathBuf::from(std::path::MAIN_SEPARATOR_STR)]));

        assert_eq!(run.summary.failed, 1);
        assert!(
            run.stderr.contains("refusing to remove the root directory"),
            "unexpected stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn interactive_skips_a_declined_operand() {
        let dir = tempdir().unwrap();
        let kept = file_in(&dir, "kept.txt");
        let removed = file_in(&dir, "removed.txt");
        let args = Args {
            interactive: true,
            ..args_for([kept, removed.clone()])
        };

        let run = execute_with(&args, RecordingBackend::default(), &[false, true]);

        assert_eq!(run.backend.trashed, vec![removed]);
        assert_eq!(run.summary.skipped, 1);
        assert_eq!(run.summary.trashed, 1);
        assert_eq!(run.summary.exit_code(), 0);
        assert!(
            run.confirm.questions[0].contains("to the trash?"),
            "unexpected prompt: {}",
            run.confirm.questions[0]
        );
    }

    #[test]
    fn interactive_names_the_irreversible_case() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            interactive: true,
            permanent: true,
            ..args_for([file])
        };

        let run = execute_with(&args, RecordingBackend::default(), &[true]);

        assert_eq!(run.summary.deleted, 1);
        assert!(
            run.confirm.questions[0].contains("permanently delete"),
            "unexpected prompt: {}",
            run.confirm.questions[0]
        );
    }

    #[test]
    fn force_suppresses_the_prompt() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            interactive: true,
            force: true,
            ..args_for([file.clone()])
        };

        let run = execute_with(&args, RecordingBackend::default(), &[false]);

        assert_eq!(run.backend.trashed, vec![file]);
        assert!(run.confirm.questions.is_empty());
    }

    #[test]
    fn verbose_reports_each_removal() {
        let dir = tempdir().unwrap();
        let file = file_in(&dir, "notes.txt");
        let args = Args {
            verbose: true,
            ..args_for([file.clone()])
        };

        let run = execute(&args);

        assert_eq!(run.stdout, format!("trashed {}\n", file.display()));

        let args = Args {
            verbose: true,
            permanent: true,
            ..args_for([file.clone()])
        };
        let run = execute(&args);
        assert_eq!(run.stdout, format!("deleted {}\n", file.display()));
    }

    #[test]
    fn a_backend_failure_is_reported_and_the_run_continues() {
        let dir = tempdir().unwrap();
        let first = file_in(&dir, "a.txt");
        let second = file_in(&dir, "b.txt");
        let backend = RecordingBackend {
            error: Some("trash is full".to_string()),
            ..RecordingBackend::default()
        };

        let run = execute_with(&args_for([first, second]), backend, &[]);

        assert_eq!(run.summary.failed, 2);
        assert_eq!(run.summary.exit_code(), 1);
        assert_eq!(run.stderr.lines().count(), 2);
        assert!(
            run.stderr.contains("trash is full"),
            "unexpected stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn no_operands_is_an_error_unless_forced() {
        let run = execute(&Args::default());
        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.stderr, "noh rm: missing operand\n");

        let run = execute(&Args {
            force: true,
            ..Args::default()
        });
        assert_eq!(run.summary, Summary::default());
        assert_eq!(run.summary.exit_code(), 0);
        assert!(run.stderr.is_empty());
    }
}
