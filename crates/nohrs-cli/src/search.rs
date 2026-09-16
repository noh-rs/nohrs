//! `noh search` — find files by what is in them, or by what they are called.
//!
//! The GUI has two searches (`docs/search.md` §8): the launcher's global one and
//! the explorer's, scoped to the pane's current directory. This is the latter,
//! with the operands naming the scope instead of the pane, and it goes through
//! [`nohrs_services::search::scoped`] rather than shelling out to `grep`, so the
//! terminal and the app answer the same way about the same tree.
//!
//! A query is matched against both the names walked and the text inside the
//! files, which is what the index-backed search does (its documents carry a
//! `filename` field beside the `content` one) and therefore what someone who
//! has used the app expects from the terminal. `--name` and `--content` narrow
//! it to one of the two.
//!
//! The exit codes are `noh`'s own and not `grep(1)`'s: `1` means the command
//! failed, not that the search came up empty. Finding nothing is a perfectly
//! good answer, and it is reported in words on stderr so that a person is not
//! left wondering, while stdout stays exactly what a pipe should see.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use nohrs_core::errors::{Error, Result};
use nohrs_services::search::scoped::{self, Answered, Engine, Options, Outcome, Subject};

/// Operands and flags for the `search` subcommand.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct Args {
    /// What to look for: a regular expression, unless `--fixed-strings` is
    /// given.
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Where to look. Defaults to the current directory.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Match only the names of files and directories, not what is in them.
    #[arg(short = 'n', long, conflicts_with = "content")]
    pub name: bool,

    /// Match only what is in the files, not their names.
    #[arg(short = 'c', long)]
    pub content: bool,

    /// Match regardless of case.
    #[arg(short, long)]
    pub ignore_case: bool,

    /// Take the query literally rather than as a regular expression.
    #[arg(short = 'F', long)]
    pub fixed_strings: bool,

    /// Print the path of each matching file once, without the matching lines.
    #[arg(short = 'l', long)]
    pub files_with_matches: bool,

    /// Stop after this many matches.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Descend at most this many levels below each starting point.
    #[arg(long, value_name = "N")]
    pub max_depth: Option<usize>,

    /// Search hidden files and directories too.
    #[arg(long)]
    pub hidden: bool,

    /// Do not let `.gitignore` / `.ignore` files exclude anything.
    #[arg(long)]
    pub no_ignore: bool,

    /// Which engine answers the search.
    #[arg(long, value_name = "ENGINE", default_value_t = EngineChoice::Auto, value_enum)]
    pub engine: EngineChoice,

    /// Print the matches as JSON, one object per line.
    #[arg(long)]
    pub json: bool,
}

/// The `--engine` spellings, kept apart from
/// [`nohrs_services::search::scoped::Engine`] so that the services crate owes
/// nothing to clap.
#[derive(clap::ValueEnum, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EngineChoice {
    /// Ask the index when it can answer, and read the files when it cannot.
    #[default]
    Auto,
    /// Insist on the index, and fail with the reason when it cannot answer.
    Index,
    /// Read the files, whatever the index may know.
    Walk,
}

impl std::fmt::Display for EngineChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let spelling = match self {
            Self::Auto => "auto",
            Self::Index => "index",
            Self::Walk => "walk",
        };
        formatter.write_str(spelling)
    }
}

impl From<EngineChoice> for Engine {
    fn from(choice: EngineChoice) -> Self {
        match choice {
            EngineChoice::Auto => Engine::Auto,
            EngineChoice::Index => Engine::Index,
            EngineChoice::Walk => Engine::Walk,
        }
    }
}

impl Args {
    /// The scope and query interpretation these flags ask for.
    pub fn options(&self) -> Options {
        Options {
            engine: self.engine.into(),
            subject: match (self.name, self.content) {
                (true, false) => Subject::Names,
                (false, true) => Subject::Contents,
                // Both flags at once is refused by clap, so what is left is
                // neither of them: search names and contents alike.
                _ => Subject::Both,
            },
            max_depth: self.max_depth,
            include_hidden: self.hidden,
            respect_ignore_files: !self.no_ignore,
            case_insensitive: self.ignore_case,
            literal: self.fixed_strings,
            limit: self.limit,
        }
    }

    /// Where to search: the operands, or the working directory when there are
    /// none.
    fn roots(&self) -> Vec<PathBuf> {
        if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths.clone()
        }
    }

    /// Whether the output is a list of paths rather than of matching lines.
    fn paths_only(&self) -> bool {
        self.files_with_matches
    }
}

/// What a run of [`Session::run`] found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Matches found, counting every matching line rather than every file.
    pub matched: usize,
    /// Lines written to stdout. Lower than `matched` when several matches share
    /// a path and only the path is printed.
    pub printed: usize,
    /// Operands that could not be searched at all.
    pub failed: usize,
    /// Whether `--limit` ended a search before its tree was exhausted.
    pub truncated: bool,
}

impl Summary {
    /// The process exit code for this run: `1` if an operand could not be
    /// searched, else `0`. Matching nothing is an answer, not a failure.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed > 0)
    }
}

/// The engine a [`Session`] queries, behind a trait so that what the command
/// *reports* can be tested without a directory tree to walk.
pub trait Backend {
    /// Find what `query` matches under `root`.
    fn search(&self, root: &Path, query: &str, options: &Options) -> Result<Outcome>;
}

/// The [`Backend`] used in production, delegating to `nohrs-services`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServicesBackend;

impl Backend for ServicesBackend {
    fn search(&self, root: &Path, query: &str, options: &Options) -> Result<Outcome> {
        // `{:#}` keeps the context the service attached (which pattern, and why
        // the regex engine rejected it) instead of only the outermost sentence.
        scoped::search(root, query, options).map_err(|error| Error::Other(format!("{error:#}")))
    }
}

/// One `noh search` invocation: the engine plus the streams it reports on.
pub struct Session<'a> {
    backend: &'a dyn Backend,
    output: &'a mut dyn Write,
    errors: &'a mut dyn Write,
    summary: Summary,
    printed_paths: HashSet<PathBuf>,
    noted_engine: bool,
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
            summary: Summary::default(),
            printed_paths: HashSet::new(),
            noted_engine: false,
        }
    }

    /// Search each operand in turn and report what matched.
    pub fn run(mut self, args: &Args) -> io::Result<Summary> {
        if args.query.is_empty() {
            // An empty pattern matches every line of every file, which is never
            // what someone meant to ask for and would take a long time to say.
            writeln!(
                self.errors,
                "noh search: the query is empty; give something to look for"
            )?;
            self.summary.failed += 1;
            return Ok(self.summary);
        }

        let mut options = args.options();
        for root in args.roots() {
            if let Some(limit) = args.limit {
                // The limit is a budget for the whole run, so each operand gets
                // what the ones before it left.
                let remaining = limit.saturating_sub(self.summary.matched);
                if remaining == 0 {
                    self.summary.truncated = true;
                    break;
                }
                options.limit = Some(remaining);
            }
            match self.backend.search(&root, &args.query, &options) {
                Ok(outcome) => {
                    self.summary.truncated |= outcome.truncated;
                    self.note_engine(&outcome)?;
                    self.report(args, &outcome)?;
                }
                Err(error) => {
                    writeln!(
                        self.errors,
                        "noh search: {}: {}",
                        root.display(),
                        crate::message(&error)
                    )?;
                    self.summary.failed += 1;
                }
            }
        }

        if self.summary.truncated {
            writeln!(
                self.errors,
                "noh search: stopped at {} matches; raise --limit for more",
                self.summary.matched
            )?;
        } else if self.summary.matched == 0 && self.summary.failed == 0 {
            // On stderr, not stdout: a pipe and a `--json` reader both want the
            // empty stream that "nothing matched" really is, while a person at a
            // terminal wants to be told the search ran and came back empty.
            writeln!(self.errors, "noh search: no matches")?;
        }
        Ok(self.summary)
    }

    /// Say once that the index did not answer, and why.
    ///
    /// Silently reading every file when the user believes an index is doing the
    /// work is how a search comes to look mysteriously slow, or mysteriously
    /// thorough; either way it is a thing they can act on (build the index,
    /// drop a flag), so it is said rather than logged.
    fn note_engine(&mut self, outcome: &Outcome) -> io::Result<()> {
        let Answered::Walk(Some(reason)) = &outcome.answered_by else {
            return Ok(());
        };
        if self.noted_engine {
            return Ok(());
        }
        self.noted_engine = true;
        writeln!(self.errors, "noh search: read the files: {reason}")
    }

    fn report(&mut self, args: &Args, outcome: &Outcome) -> io::Result<()> {
        for result in &outcome.results {
            self.summary.matched += 1;
            let path = shown(&result.path);
            if args.paths_only() {
                if !self.printed_paths.insert(result.path.clone()) {
                    continue;
                }
                if args.json {
                    writeln!(self.output, "{}", path_json(path))?;
                } else {
                    writeln!(self.output, "{}", path.display())?;
                }
            } else if result.is_name_match() {
                // A name match has no line to quote, so it is the bare path —
                // and it comes immediately before that file's own lines, which
                // is what tells the two apart in a mixed listing.
                if args.json {
                    writeln!(
                        self.output,
                        "{}",
                        serde_json::json!({ "kind": "name", "path": path.to_string_lossy() })
                    )?;
                } else {
                    writeln!(self.output, "{}", path.display())?;
                }
            } else if args.json {
                writeln!(
                    self.output,
                    "{}",
                    serde_json::json!({
                        "kind": "content",
                        "path": path.to_string_lossy(),
                        "line_number": result.line_number,
                        "line_content": result.line_content,
                    })
                )?;
            } else {
                writeln!(
                    self.output,
                    "{}:{}:{}",
                    path.display(),
                    result.line_number,
                    result.line_content
                )?;
            }
            self.summary.printed += 1;
        }
        Ok(())
    }
}

/// Render a path as its own JSON object.
///
/// The path is rendered lossily rather than serialized: `serde_json` refuses a
/// `Path` that is not valid UTF-8, and a path that came back from a walk of
/// someone's filesystem may well not be. A match is still worth printing.
fn path_json(path: &Path) -> String {
    serde_json::json!({ "path": path.to_string_lossy() }).to_string()
}

/// The path as the user should see it.
///
/// Walking the default root leaves `./` on the front of every path, which is
/// noise in a listing the reader is scanning for names.
fn shown(path: &Path) -> &Path {
    path.strip_prefix(".").unwrap_or(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::cell::RefCell;

    use nohrs_services::search::SearchResult;
    use nohrs_services::search::scoped::NoIndex;

    use super::*;

    /// A backend that answers with canned outcomes and records what it was
    /// asked, so the command's reporting can be tested on its own.
    #[derive(Default)]
    struct FakeBackend {
        outcomes: RefCell<Vec<Result<Outcome>>>,
        asked: RefCell<Vec<(PathBuf, String, Options)>>,
    }

    impl FakeBackend {
        fn answering(outcomes: Vec<Result<Outcome>>) -> Self {
            Self {
                outcomes: RefCell::new(outcomes),
                asked: RefCell::new(Vec::new()),
            }
        }

        fn finding(results: Vec<SearchResult>) -> Self {
            Self::answering(vec![Ok(Outcome {
                results,
                truncated: false,
                ..Outcome::default()
            })])
        }
    }

    impl Backend for FakeBackend {
        fn search(&self, root: &Path, query: &str, options: &Options) -> Result<Outcome> {
            self.asked
                .borrow_mut()
                .push((root.to_path_buf(), query.to_string(), options.clone()));
            let mut outcomes = self.outcomes.borrow_mut();
            if outcomes.is_empty() {
                return Ok(Outcome::default());
            }
            outcomes.remove(0)
        }
    }

    fn line(path: &str, line_number: usize, line_content: &str) -> SearchResult {
        SearchResult {
            path: PathBuf::from(path),
            line_number,
            line_content: line_content.to_string(),
        }
    }

    fn name(path: &str) -> SearchResult {
        line(path, 0, "")
    }

    struct Run {
        output: String,
        errors: String,
        summary: Summary,
    }

    fn run(backend: &dyn Backend, args: &Args) -> Run {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let summary = Session::new(backend, &mut output, &mut errors)
            .run(args)
            .unwrap();
        Run {
            output: String::from_utf8(output).unwrap(),
            errors: String::from_utf8(errors).unwrap(),
            summary,
        }
    }

    fn args(query: &str) -> Args {
        Args {
            query: query.to_string(),
            ..Args::default()
        }
    }

    #[test]
    fn a_content_match_is_printed_as_path_line_and_text() {
        let backend = FakeBackend::finding(vec![
            line("./src/main.rs", 12, "fn main() {}"),
            line("./README.md", 3, "main idea"),
        ]);

        let run = run(&backend, &args("main"));

        assert_eq!(
            run.output,
            "src/main.rs:12:fn main() {}\nREADME.md:3:main idea\n"
        );
        assert_eq!(run.summary.matched, 2);
        assert_eq!(run.summary.exit_code(), 0);
        assert!(run.errors.is_empty());
    }

    #[test]
    fn with_no_operand_the_working_directory_is_searched() {
        let backend = FakeBackend::default();

        run(&backend, &args("needle"));

        let asked = backend.asked.borrow();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].0, PathBuf::from("."));
        assert_eq!(asked[0].1, "needle");
    }

    #[test]
    fn finding_nothing_is_said_on_stderr_and_is_not_a_failure() {
        let backend = FakeBackend::default();

        let run = run(&backend, &args("needle"));

        // stdout stays empty so a pipe (and `--json`) sees the empty stream
        // that "nothing matched" is.
        assert!(run.output.is_empty());
        assert_eq!(run.errors, "noh search: no matches\n");
        assert_eq!(run.summary.exit_code(), 0);
    }

    #[test]
    fn an_empty_query_is_refused_rather_than_matching_everything() {
        let backend = FakeBackend::default();

        let run = run(&backend, &args(""));

        assert!(run.output.is_empty());
        assert!(
            run.errors.contains("the query is empty"),
            "unexpected stderr: {}",
            run.errors
        );
        assert!(backend.asked.borrow().is_empty());
        assert_eq!(run.summary.exit_code(), 1);
    }

    #[test]
    fn an_operand_that_cannot_be_searched_is_named_and_the_rest_go_on() {
        let backend = FakeBackend::answering(vec![
            Err(Error::Other("`a(` is not a valid pattern".to_string())),
            Ok(Outcome {
                results: vec![line("docs/notes.md", 1, "a(")],
                truncated: false,
                ..Outcome::default()
            }),
        ]);
        let args = Args {
            paths: vec![PathBuf::from("src"), PathBuf::from("docs")],
            ..args("a(")
        };

        let run = run(&backend, &args);

        assert_eq!(run.output, "docs/notes.md:1:a(\n");
        assert_eq!(run.errors, "noh search: src: `a(` is not a valid pattern\n");
        assert_eq!(run.summary.failed, 1);
        // The run did not do all it was asked, which is what the code reports.
        assert_eq!(run.summary.exit_code(), 1);
    }

    #[test]
    fn a_name_search_prints_paths_and_asks_for_names_only() {
        let backend = FakeBackend::finding(vec![name("./src/main.rs"), name("./src")]);
        let args = Args {
            name: true,
            ..args("main")
        };

        let run = run(&backend, &args);

        assert_eq!(run.output, "src/main.rs\nsrc\n");
        assert_eq!(backend.asked.borrow()[0].2.subject, Subject::Names);
    }

    #[test]
    fn files_with_matches_prints_each_path_once() {
        let backend = FakeBackend::finding(vec![
            line("./src/main.rs", 12, "needle"),
            line("./src/main.rs", 20, "needle again"),
            line("./src/lib.rs", 4, "needle"),
        ]);
        let args = Args {
            files_with_matches: true,
            ..args("needle")
        };

        let run = run(&backend, &args);

        assert_eq!(run.output, "src/main.rs\nsrc/lib.rs\n");
        // Every match still counts, even the ones that shared a path.
        assert_eq!(run.summary.matched, 3);
        assert_eq!(run.summary.printed, 2);
    }

    #[test]
    fn json_carries_the_whole_match_and_escapes_the_line() {
        let backend = FakeBackend::finding(vec![line("./notes.txt", 7, "a \"quoted\" line")]);
        let args = Args {
            json: true,
            ..args("quoted")
        };

        let run = run(&backend, &args);

        assert_eq!(
            run.output,
            "{\"kind\":\"content\",\"line_content\":\"a \\\"quoted\\\" line\",\"line_number\":7,\"path\":\"notes.txt\"}\n"
        );
    }

    #[test]
    fn json_says_which_kind_of_match_each_object_is() {
        let backend = FakeBackend::finding(vec![name("./notes.txt")]);
        let args = Args {
            json: true,
            ..args("notes")
        };

        let run = run(&backend, &args);

        assert_eq!(run.output, "{\"kind\":\"name\",\"path\":\"notes.txt\"}\n");
    }

    #[test]
    fn json_output_that_is_only_paths_carries_only_the_path() {
        let backend = FakeBackend::finding(vec![line("./notes.txt", 7, "needle")]);
        let args = Args {
            json: true,
            files_with_matches: true,
            ..args("needle")
        };

        let run = run(&backend, &args);

        // Nothing to tag: `-l` prints the file, whichever way it matched.
        assert_eq!(run.output, "{\"path\":\"notes.txt\"}\n");
    }

    #[test]
    fn the_limit_is_a_budget_for_the_whole_run() {
        let backend = FakeBackend::answering(vec![
            Ok(Outcome {
                results: vec![line("src/a.rs", 1, "needle"), line("src/b.rs", 2, "needle")],
                truncated: false,
                ..Outcome::default()
            }),
            Ok(Outcome {
                results: vec![line("docs/c.md", 3, "needle")],
                truncated: true,
                ..Outcome::default()
            }),
        ]);
        let args = Args {
            paths: vec![PathBuf::from("src"), PathBuf::from("docs")],
            limit: Some(3),
            ..args("needle")
        };

        let run = run(&backend, &args);

        let asked = backend.asked.borrow();
        assert_eq!(asked[0].2.limit, Some(3));
        // The first operand used two of the three.
        assert_eq!(asked[1].2.limit, Some(1));
        assert_eq!(run.summary.matched, 3);
        assert!(run.summary.truncated);
        assert!(
            run.errors.contains("stopped at 3 matches"),
            "unexpected stderr: {}",
            run.errors
        );
        // Truncation is not a failure: what was printed did match.
        assert_eq!(run.summary.exit_code(), 0);
    }

    #[test]
    fn a_spent_limit_stops_the_run_before_the_next_operand() {
        let backend = FakeBackend::answering(vec![Ok(Outcome {
            results: vec![line("src/a.rs", 1, "needle")],
            truncated: false,
            ..Outcome::default()
        })]);
        let args = Args {
            paths: vec![PathBuf::from("src"), PathBuf::from("docs")],
            limit: Some(1),
            ..args("needle")
        };

        let run = run(&backend, &args);

        assert_eq!(backend.asked.borrow().len(), 1, "docs was searched anyway");
        assert!(run.summary.truncated);
    }

    #[test]
    fn the_index_standing_aside_is_reported_once_and_not_per_operand() {
        let aside = || {
            Ok(Outcome {
                answered_by: Answered::Walk(Some(NoIndex::NotBuilt)),
                ..Outcome::default()
            })
        };
        let backend = FakeBackend::answering(vec![aside(), aside()]);
        let args = Args {
            paths: vec![PathBuf::from("src"), PathBuf::from("docs")],
            ..args("needle")
        };

        let run = run(&backend, &args);

        assert_eq!(
            run.errors,
            "noh search: read the files: no index has been built yet\nnoh search: no matches\n"
        );
    }

    #[test]
    fn a_walk_that_was_asked_for_is_not_explained() {
        let backend = FakeBackend::answering(vec![Ok(Outcome {
            results: vec![line("src/a.rs", 1, "needle")],
            ..Outcome::default()
        })]);
        let args = Args {
            engine: EngineChoice::Walk,
            ..args("needle")
        };

        assert!(run(&backend, &args).errors.is_empty());
    }

    #[test]
    fn the_flags_reach_the_engine() {
        let backend = FakeBackend::default();
        let args = Args {
            ignore_case: true,
            fixed_strings: true,
            hidden: true,
            no_ignore: true,
            max_depth: Some(2),
            ..args("needle")
        };

        run(&backend, &args);

        let asked = backend.asked.borrow();
        let options = &asked[0].2;
        assert_eq!(
            *options,
            Options {
                engine: Engine::Auto,
                subject: Subject::Both,
                max_depth: Some(2),
                include_hidden: true,
                respect_ignore_files: false,
                case_insensitive: true,
                literal: true,
                limit: None,
            }
        );
    }
}
