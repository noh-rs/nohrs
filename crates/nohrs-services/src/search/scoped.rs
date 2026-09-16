//! On-demand search under a caller-chosen root: walk the tree and match either
//! the text inside each file or the names along the way.
//!
//! This is the V1 backend of [`docs/search.md`](../../../../docs/search.md) §2,
//! factored out of [`super::ripgrep`] so a caller that knows what it wants to
//! search — the explorer's current directory, `noh search`'s operands — can say
//! so, instead of being handed the one whole-filesystem scan the root scope
//! needs.

use std::path::Path;

use anyhow::{Context, Result};
use grep::matcher::Matcher;
use grep::regex::{RegexMatcher, RegexMatcherBuilder};
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;

use super::SearchResult;

/// What the query is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subject {
    /// Both: the name of every entry walked, and the text inside every file.
    /// A file can match twice, once for each, and does.
    #[default]
    Both,
    /// Only the text inside each file. Directories never match.
    Contents,
    /// Only the name of each file and directory, not its contents.
    Names,
}

impl Subject {
    /// Whether entry names are matched.
    pub fn includes_names(self) -> bool {
        matches!(self, Self::Both | Self::Names)
    }

    /// Whether file contents are matched.
    pub fn includes_contents(self) -> bool {
        matches!(self, Self::Both | Self::Contents)
    }
}

/// How far a search reaches and how its query is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Whether to match file contents or the names of the entries walked.
    pub subject: Subject,
    /// How many directories below the root to descend; `None` for no limit.
    pub max_depth: Option<usize>,
    /// Whether dot-files and dot-directories are searched.
    pub include_hidden: bool,
    /// Whether `.gitignore` / `.ignore` files prune the walk.
    pub respect_ignore_files: bool,
    /// Whether the query matches regardless of case.
    pub case_insensitive: bool,
    /// Whether the query is a literal string rather than a regular expression.
    pub literal: bool,
    /// Stop once this many matches have been collected; `None` for no limit.
    pub limit: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            subject: Subject::default(),
            max_depth: None,
            include_hidden: false,
            respect_ignore_files: true,
            case_insensitive: false,
            literal: false,
            limit: None,
        }
    }
}

/// What one [`search`] found.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The matches, in the order the walk reached them.
    pub results: Vec<SearchResult>,
    /// Whether [`Options::limit`] ended the search before the tree was
    /// exhausted. It says the search stopped early, not that more matches
    /// certainly exist: a limit reached by the very last match also sets it.
    pub truncated: bool,
}

/// Walks `root` and returns what `query` matched there.
///
/// Fails only when `query` is not a pattern the matcher can build. A root that
/// cannot be read, or an individual file that cannot be searched, is reported
/// through `tracing` and skipped: one unreadable directory in a large tree is
/// normal and must not throw away the rest of the results.
#[tracing::instrument(
    target = "nohrs::op",
    name = "search.scan",
    level = "debug",
    skip_all,
    fields(root = %root.display(), subject = ?options.subject)
)]
pub fn search(root: &Path, query: &str, options: &Options) -> Result<Outcome> {
    let matcher = build_matcher(query, options)?;
    let mut outcome = Outcome::default();
    if options.limit == Some(0) {
        return Ok(outcome);
    }

    let respect_ignore_files = options.respect_ignore_files;
    let walker = WalkBuilder::new(root)
        .max_depth(options.max_depth)
        .hidden(!options.include_hidden)
        .ignore(respect_ignore_files)
        .git_ignore(respect_ignore_files)
        .git_global(respect_ignore_files)
        .git_exclude(respect_ignore_files)
        .parents(respect_ignore_files)
        .build();

    let mut searcher = SearcherBuilder::new()
        // A match in a binary file is a line the caller cannot show — it would
        // put control bytes on a terminal — so stop at the first NUL byte, the
        // way ripgrep does by default.
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            // An unreadable directory is normal (permissions, races) and must
            // not abort the walk; record it for diagnosis and move on.
            Err(error) => {
                tracing::debug!("skipping unreadable entry: {error}");
                continue;
            }
        };
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        // The root directory is not itself a candidate. A root that names a
        // single file is: searching one file is a legitimate scope.
        if entry.depth() == 0 && is_dir {
            continue;
        }

        let path = entry.path();
        if options.subject.includes_names() {
            // A name that is not UTF-8 cannot be matched against the query, and
            // is skipped rather than matched by accident.
            if let Some(name) = entry.file_name().to_str() {
                if matcher.is_match(name.as_bytes()).unwrap_or(false) {
                    outcome.results.push(SearchResult {
                        path: path.to_path_buf(),
                        line_number: 0,
                        line_content: String::new(),
                    });
                }
            }
        }

        let room = remaining(options.limit, outcome.results.len());
        if options.subject.includes_contents()
            && room != Some(0)
            && entry.file_type().is_some_and(|kind| kind.is_file())
        {
            let sink = Collector {
                path,
                results: &mut outcome.results,
                room,
            };
            if let Err(error) = searcher.search_path(&matcher, path, sink) {
                // Unreadable or undecodable files are expected in a walk of
                // someone's filesystem; ripgrep skips them too.
                tracing::debug!("cannot search {}: {error}", path.display());
            }
        }

        if let Some(limit) = options.limit {
            if outcome.results.len() >= limit {
                outcome.truncated = true;
                break;
            }
        }
    }

    Ok(outcome)
}

/// How many more matches the caller will accept.
fn remaining(limit: Option<usize>, collected: usize) -> Option<usize> {
    limit.map(|limit| limit.saturating_sub(collected))
}

fn build_matcher(query: &str, options: &Options) -> Result<RegexMatcher> {
    RegexMatcherBuilder::new()
        .case_insensitive(options.case_insensitive)
        .fixed_strings(options.literal)
        .build(query)
        .with_context(|| format!("`{query}` is not a valid pattern"))
}

/// Collects one file's matching lines, stopping once `room` runs out.
struct Collector<'a> {
    path: &'a Path,
    results: &'a mut Vec<SearchResult>,
    room: Option<usize>,
}

impl Sink for Collector<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch) -> Result<bool, Self::Error> {
        if self.room == Some(0) {
            return Ok(false);
        }
        self.results.push(SearchResult {
            path: self.path.to_path_buf(),
            line_number: mat.line_number().unwrap_or(0) as usize,
            line_content: String::from_utf8_lossy(trim_line_terminator(mat.bytes())).into_owned(),
        });
        self.room = self.room.map(|room| room.saturating_sub(1));
        Ok(self.room != Some(0))
    }
}

/// Drops the line terminator the searcher includes in a match.
///
/// Every caller renders the line next to something else — a path and a line
/// number here, a row in the explorer there — so the trailing newline is never
/// wanted, and a CRLF file would otherwise leave a stray carriage return in the
/// middle of the output.
fn trim_line_terminator(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn write(root: &Path, relative: &str, contents: &[u8]) -> PathBuf {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn paths(outcome: &Outcome) -> Vec<String> {
        outcome
            .results
            .iter()
            .map(|result| result.path.display().to_string())
            .collect()
    }

    fn run(root: &Path, query: &str, options: &Options) -> Outcome {
        search(root, query, options).unwrap()
    }

    #[test]
    fn a_content_search_reports_the_line_it_matched() {
        let dir = tempfile::tempdir().unwrap();
        let notes = write(dir.path(), "notes.txt", b"first line\nsecond line\nthird\n");

        let outcome = run(dir.path(), "second", &Options::default());

        assert_eq!(outcome.results.len(), 1);
        let result = &outcome.results[0];
        assert_eq!(result.path, notes);
        assert_eq!(result.line_number, 2);
        assert_eq!(result.line_content, "second line");
        assert!(!outcome.truncated);
    }

    #[test]
    fn a_crlf_file_does_not_leave_a_carriage_return_in_the_line() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"alpha\r\nbeta\r\n");

        let outcome = run(dir.path(), "alpha", &Options::default());

        assert_eq!(outcome.results[0].line_content, "alpha");
    }

    #[test]
    fn a_root_that_names_one_file_searches_just_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let notes = write(dir.path(), "notes.txt", b"needle\n");
        write(dir.path(), "other.txt", b"needle\n");

        let outcome = run(&notes, "needle", &Options::default());

        assert_eq!(paths(&outcome), vec![notes.display().to_string()]);
    }

    #[test]
    fn case_and_literal_flags_change_what_the_query_means() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"Needle\na.b\n");

        assert!(
            run(dir.path(), "needle", &Options::default())
                .results
                .is_empty()
        );
        let insensitive = Options {
            case_insensitive: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "needle", &insensitive).results.len(), 1);

        // `a.b` as a regex also matches `a-b`; as a literal it does not.
        write(dir.path(), "dashed.txt", b"a-b\n");
        assert_eq!(run(dir.path(), "a.b", &Options::default()).results.len(), 2);
        let literal = Options {
            literal: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "a.b", &literal).results.len(), 1);
    }

    #[test]
    fn an_invalid_pattern_is_an_error_rather_than_an_empty_result() {
        let dir = tempfile::tempdir().unwrap();
        let error = search(dir.path(), "a(", &Options::default()).unwrap_err();
        assert!(
            error.to_string().contains("`a(` is not a valid pattern"),
            "unexpected error: {error}"
        );

        // The same query as a literal is a perfectly good one.
        write(dir.path(), "notes.txt", b"a(\n");
        let literal = Options {
            literal: true,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "a(", &literal).results.len(), 1);
    }

    #[test]
    fn a_name_search_matches_directories_as_well_as_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "reports/report.txt", b"nothing to find here\n");

        let names = Options {
            subject: Subject::Names,
            ..Options::default()
        };
        let outcome = run(dir.path(), "report", &names);

        let mut found = paths(&outcome);
        found.sort();
        assert_eq!(
            found,
            vec![
                dir.path().join("reports").display().to_string(),
                dir.path().join("reports/report.txt").display().to_string(),
            ]
        );
        // A name match has no line to point at.
        assert!(outcome.results.iter().all(|result| result.line_number == 0));
    }

    #[test]
    fn the_default_subject_matches_a_file_by_name_and_by_content_alike() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "needle.txt", b"a line with needle in it\n");

        let outcome = run(dir.path(), "needle", &Options::default());

        assert_eq!(outcome.results.len(), 2);
        // The name match comes first, and carries no line to quote.
        assert!(outcome.results[0].is_name_match());
        assert_eq!(outcome.results[1].line_number, 1);
    }

    #[test]
    fn the_root_itself_is_never_a_name_match() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("haystack");
        std::fs::create_dir(&root).unwrap();
        write(&root, "haystack.txt", b"x\n");

        let names = Options {
            subject: Subject::Names,
            ..Options::default()
        };
        let outcome = run(&root, "haystack", &names);

        assert_eq!(
            paths(&outcome),
            vec![root.join("haystack.txt").display().to_string()]
        );
    }

    #[test]
    fn hidden_files_and_ignored_files_are_skipped_unless_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".hidden.txt", b"needle\n");
        write(dir.path(), ".ignore", b"ignored.txt\n");
        write(dir.path(), "ignored.txt", b"needle\n");
        write(dir.path(), "plain.txt", b"needle\n");

        assert_eq!(
            paths(&run(dir.path(), "needle", &Options::default())),
            vec![dir.path().join("plain.txt").display().to_string()]
        );

        let everything = Options {
            include_hidden: true,
            respect_ignore_files: false,
            ..Options::default()
        };
        assert_eq!(run(dir.path(), "needle", &everything).results.len(), 3);
    }

    #[test]
    fn max_depth_bounds_how_far_the_walk_descends() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "top.txt", b"needle\n");
        write(dir.path(), "one/two/deep.txt", b"needle\n");

        let shallow = Options {
            max_depth: Some(1),
            ..Options::default()
        };
        assert_eq!(
            paths(&run(dir.path(), "needle", &shallow)),
            vec![dir.path().join("top.txt").display().to_string()]
        );
        assert_eq!(
            run(dir.path(), "needle", &Options::default()).results.len(),
            2
        );
    }

    #[test]
    fn a_limit_stops_the_search_and_says_that_it_did() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\nneedle\nneedle\n");

        let limited = Options {
            limit: Some(2),
            ..Options::default()
        };
        let outcome = run(dir.path(), "needle", &limited);
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.truncated);

        let unlimited = run(dir.path(), "needle", &Options::default());
        assert_eq!(unlimited.results.len(), 3);
        assert!(!unlimited.truncated);
    }

    #[test]
    fn a_limit_of_zero_searches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "notes.txt", b"needle\n");

        let none = Options {
            limit: Some(0),
            ..Options::default()
        };
        assert!(run(dir.path(), "needle", &none).results.is_empty());
    }

    #[test]
    fn a_binary_file_is_not_searched_for_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "binary.bin", b"\0needle\nneedle\n");

        assert!(
            run(dir.path(), "needle", &Options::default())
                .results
                .is_empty()
        );
    }

    #[test]
    fn a_missing_root_is_an_empty_result_rather_than_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run(&dir.path().join("nowhere"), "needle", &Options::default());
        assert!(outcome.results.is_empty());
    }
}
