//! `noh index` — what the search index knows, and building it.
//!
//! The index is what makes a search answer without reading every file, and it
//! is otherwise invisible: nothing in a terminal says whether one exists, how
//! much of the disk it covers, or how old it is. `status` says all three, and
//! `build` is how a machine with no GUI running gets one at all.
//!
//! Reading never takes tantivy's writer lock (see
//! [`nohrs_services::search::indexer::IndexReader`]), so `status` is safe to run
//! beside the app. `build` does take it, and says so plainly when the app — or
//! another build — is holding it.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nohrs_core::errors::{Error, Result};
use nohrs_services::search::indexer::{IndexManager, IndexReader, Refresh};

/// The `noh index` subcommands.
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Report where the index is, what it covers, and how much it holds.
    Status,
    /// Bring the index up to date. Takes the index writer for the duration.
    Build(BuildArgs),
}

/// Flags for `noh index build`.
#[derive(clap::Args, Debug, Default, Clone, Copy)]
pub struct BuildArgs {
    /// Re-read every file, instead of only those whose modification time has
    /// changed since the last pass.
    #[arg(long)]
    pub full: bool,
}

impl BuildArgs {
    fn refresh(self) -> Refresh {
        if self.full {
            Refresh::Everything
        } else {
            Refresh::Changed
        }
    }
}

/// What `status` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Where the index is (or would be).
    pub index_path: PathBuf,
    /// The tree the index covers.
    pub content_root: PathBuf,
    /// How many documents it holds, or `None` when nothing has built one.
    pub documents: Option<u64>,
}

/// What `build` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Built {
    /// Documents written, whether new or replacing an older one.
    pub indexed: usize,
    /// Files left alone because the index already had them at that time.
    pub unchanged: usize,
    /// Documents dropped because the file is no longer there.
    pub removed: usize,
    /// How long the pass took.
    pub took: Duration,
}

/// The index itself, behind a trait so the command's reporting can be tested
/// without an index on disk — building a real one walks a home directory.
pub trait Backend {
    /// Where the index is and what it holds.
    fn status(&self) -> Result<Status>;
    /// Bring it up to date, returning what the pass did.
    fn build(&self, refresh: Refresh) -> Result<Built>;
}

/// The [`Backend`] used in production, delegating to `nohrs-services`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServicesBackend;

impl ServicesBackend {
    fn location() -> Result<(PathBuf, PathBuf)> {
        IndexManager::default_location().map_err(|error| Error::Other(format!("{error:#}")))
    }
}

impl Backend for ServicesBackend {
    fn status(&self) -> Result<Status> {
        let (index_path, content_root) = Self::location()?;
        let reader = IndexReader::open(index_path.clone(), content_root.clone())
            .map_err(|error| Error::Other(format!("{error:#}")))?;
        let documents = reader.map(|reader| reader.document_count());
        Ok(Status {
            index_path,
            content_root,
            documents,
        })
    }

    fn build(&self, refresh: Refresh) -> Result<Built> {
        let started = Instant::now();
        let manager = IndexManager::new().map_err(|error| Error::Other(format!("{error:#}")))?;
        // No progress channel: a one-shot command has nowhere to put a progress
        // bar that a pipe would not have to read around.
        let report = manager
            .index_home(refresh, None)
            .map_err(|error| Error::Other(format!("{error:#}")))?;
        Ok(Built {
            indexed: report.indexed,
            unchanged: report.unchanged,
            removed: report.removed,
            took: started.elapsed(),
        })
    }
}

/// What a run of [`Session::run`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Whether the command failed.
    pub failed: bool,
}

impl Summary {
    /// The process exit code for this run: `1` on failure, else `0`.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed)
    }
}

/// One `noh index` invocation.
pub struct Session<'a> {
    backend: &'a dyn Backend,
    output: &'a mut dyn Write,
    errors: &'a mut dyn Write,
}

impl<'a> Session<'a> {
    /// Assemble a session around `backend`.
    pub fn new(
        backend: &'a dyn Backend,
        output: &'a mut dyn Write,
        errors: &'a mut dyn Write,
    ) -> Self {
        Self {
            backend,
            output,
            errors,
        }
    }

    /// Run one subcommand.
    pub fn run(self, command: &Command) -> io::Result<Summary> {
        match command {
            Command::Status => self.status(),
            Command::Build(args) => self.build(*args),
        }
    }

    fn status(self) -> io::Result<Summary> {
        let status = match self.backend.status() {
            Ok(status) => status,
            Err(error) => return self.abort(&error),
        };
        writeln!(self.output, "index      {}", status.index_path.display())?;
        writeln!(self.output, "covers     {}", status.content_root.display())?;
        match status.documents {
            // An index that exists but holds nothing answers every search with
            // "no results", so it is worth as much as no index at all and is
            // reported the same way: with what to do about it.
            Some(0) | None => writeln!(
                self.output,
                "documents  none yet — run `noh index build`, or open the app"
            )?,
            Some(documents) => writeln!(self.output, "documents  {documents}")?,
        }
        Ok(Summary::default())
    }

    fn build(self, args: BuildArgs) -> io::Result<Summary> {
        let status = self.backend.status();
        if let Ok(status) = &status {
            writeln!(self.output, "indexing {}", status.content_root.display())?;
            // Said before the work rather than after it: a first pass over a
            // home directory is minutes of silence otherwise.
            self.output.flush()?;
        }
        match self.backend.build(args.refresh()) {
            Ok(built) => {
                // The counts are what say whether the pass had anything to do,
                // which is the difference between "nothing has changed" and
                // "the index is not being updated".
                writeln!(
                    self.output,
                    "indexed {}, unchanged {}, removed {} in {:.1}s",
                    built.indexed,
                    built.unchanged,
                    built.removed,
                    built.took.as_secs_f64()
                )?;
                Ok(Summary::default())
            }
            Err(error) => self.abort(&error),
        }
    }

    fn abort(self, error: &Error) -> io::Result<Summary> {
        writeln!(self.errors, "noh index: {}", crate::message(error))?;
        Ok(Summary { failed: true })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct FakeBackend {
        status: Result<Status>,
        build: Result<Built>,
        asked_for: std::cell::Cell<Option<Refresh>>,
    }

    impl FakeBackend {
        fn holding(documents: Option<u64>) -> Self {
            Self {
                status: Ok(Status {
                    index_path: PathBuf::from("/home/me/.nohrs/index"),
                    content_root: PathBuf::from("/home/me/Documents"),
                    documents,
                }),
                build: Ok(Built {
                    indexed: documents.unwrap_or_default() as usize,
                    unchanged: 0,
                    removed: 0,
                    took: Duration::from_millis(1500),
                }),
                asked_for: std::cell::Cell::new(None),
            }
        }
    }

    impl Backend for FakeBackend {
        fn status(&self) -> Result<Status> {
            match &self.status {
                Ok(status) => Ok(status.clone()),
                Err(error) => Err(Error::Other(crate::message(error))),
            }
        }

        fn build(&self, refresh: Refresh) -> Result<Built> {
            self.asked_for.set(Some(refresh));
            match &self.build {
                Ok(built) => Ok(*built),
                Err(error) => Err(Error::Other(crate::message(error))),
            }
        }
    }

    fn run(backend: &dyn Backend, command: &Command) -> (String, String, Summary) {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let summary = Session::new(backend, &mut output, &mut errors)
            .run(command)
            .unwrap();
        (
            String::from_utf8(output).unwrap(),
            String::from_utf8(errors).unwrap(),
            summary,
        )
    }

    #[test]
    fn status_reports_where_the_index_is_and_what_it_holds() {
        let backend = FakeBackend::holding(Some(12_431));

        let (output, errors, summary) = run(&backend, &Command::Status);

        assert_eq!(
            output,
            "index      /home/me/.nohrs/index\n\
             covers     /home/me/Documents\n\
             documents  12431\n"
        );
        assert!(errors.is_empty());
        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn an_index_that_holds_nothing_reads_the_same_as_no_index_at_all() {
        for documents in [None, Some(0)] {
            let backend = FakeBackend::holding(documents);

            let (output, _, summary) = run(&backend, &Command::Status);

            assert!(
                output.contains("documents  none yet — run `noh index build`"),
                "unexpected output for {documents:?}: {output}"
            );
            // Nothing failed: an index that has not been built is a state to
            // report, not an error.
            assert_eq!(summary.exit_code(), 0);
        }
    }

    #[test]
    fn build_says_what_it_is_about_to_do_and_then_what_it_did() {
        let backend = FakeBackend::holding(Some(7));

        let (output, errors, summary) = run(&backend, &Command::Build(BuildArgs::default()));

        assert_eq!(
            output,
            "indexing /home/me/Documents\nindexed 7, unchanged 0, removed 0 in 1.5s\n"
        );
        assert!(errors.is_empty());
        assert_eq!(summary.exit_code(), 0);
    }

    #[test]
    fn build_reads_only_what_changed_unless_full_is_given() {
        let backend = FakeBackend::holding(Some(7));
        run(&backend, &Command::Build(BuildArgs::default()));
        assert_eq!(backend.asked_for.get(), Some(Refresh::Changed));

        let backend = FakeBackend::holding(Some(7));
        run(&backend, &Command::Build(BuildArgs { full: true }));
        assert_eq!(backend.asked_for.get(), Some(Refresh::Everything));
    }

    #[test]
    fn a_writer_held_elsewhere_is_reported_rather_than_retried() {
        let backend = FakeBackend {
            build: Err(Error::Other(
                "the index at /home/me/.nohrs/index is being written by another nohrs process"
                    .to_string(),
            )),
            ..FakeBackend::holding(Some(7))
        };

        let (_, errors, summary) = run(&backend, &Command::Build(BuildArgs::default()));

        assert_eq!(
            errors,
            "noh index: the index at /home/me/.nohrs/index is being written by another nohrs process\n"
        );
        assert_eq!(summary.exit_code(), 1);
    }

    #[test]
    fn a_status_that_cannot_be_read_is_a_failure() {
        let backend = FakeBackend {
            status: Err(Error::Other(
                "the index is from an older version".to_string(),
            )),
            ..FakeBackend::holding(None)
        };

        let (output, errors, summary) = run(&backend, &Command::Status);

        assert!(output.is_empty());
        assert_eq!(errors, "noh index: the index is from an older version\n");
        assert_eq!(summary.exit_code(), 1);
    }
}
