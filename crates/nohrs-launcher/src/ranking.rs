//! Turning index entries into ranked launcher results.
//!
//! The pipeline is the one in docs/architecture.md §3.2: candidates come from
//! the name index, `nucleo` scores them, and a small set of boosts re-ranks the
//! survivors before they become [`LauncherItem`]s. Matching runs on a background
//! executor, so nothing here touches GPUI state.

use std::path::Path;

use gpui::SharedString;
use nohrs_services::search::file_index::IndexedEntry;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Score adjustments layered on nucleo's own score (docs/launcher.md §6).
///
/// nucleo scores a short, well-matched name in the low hundreds, so these are
/// sized to break ties between comparable matches rather than to overrule a
/// clearly better one.
const DEPTH_PENALTY_PER_LEVEL: u32 = 4;
/// Ceiling on the depth penalty: past a few levels, deeper is not meaningfully
/// worse, and without a cap a deep exact match would lose to a shallow poor one.
const MAX_DEPTH_PENALTY: u32 = 40;
/// Awarded when the name is exactly the query — the strongest signal there is.
const EXACT_NAME_BONUS: u32 = 96;
/// Awarded when the name starts with the query, the next-strongest signal.
const PREFIX_BONUS: u32 = 32;

/// What a result row represents.
///
/// Search-only for now; commands and plugin results (docs/launcher.md §5) become
/// further variants without changing the rest of the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A file on disk.
    File,
    /// A directory on disk.
    Folder,
}

impl ItemKind {
    /// The right-aligned badge label for the result row.
    pub fn badge(self) -> &'static str {
        match self {
            ItemKind::File => "File",
            ItemKind::Folder => "Folder",
        }
    }

    /// Asset path of the row's leading icon.
    pub fn icon_path(self) -> &'static str {
        match self {
            ItemKind::File => "icons/file.svg",
            ItemKind::Folder => "icons/folder.svg",
        }
    }
}

/// One row in the result list, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherItem {
    /// Full path of the entry the row opens.
    pub path: std::path::PathBuf,
    /// Primary text: the file or directory name.
    pub title: SharedString,
    /// Secondary text: the containing directory, abbreviated to `~` under home.
    pub subtitle: SharedString,
    /// Which kind of entry this is.
    pub kind: ItemKind,
    /// Final score after boosts; higher sorts first.
    pub score: u32,
    /// Character positions in `title` that the query matched, sorted and
    /// deduplicated, for highlighting. Empty when the query only matched the
    /// full path rather than the name itself.
    pub title_matches: Vec<u32>,
}

/// Scores `entries` against `query` and returns the best `limit` as rows.
///
/// `home` is both the reference point for the depth boost and the prefix
/// abbreviated to `~` in subtitles; pass the directory the index was built from.
/// An empty or whitespace-only query yields no results — the launcher shows
/// nothing until something is typed (docs/launcher.md §3).
pub fn rank(
    entries: &[IndexedEntry],
    query: &str,
    limit: usize,
    home: Option<&Path>,
) -> Vec<LauncherItem> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    // `match_paths` tunes nucleo's bonuses for path-shaped haystacks, which is
    // right for both haystacks we use: bare names and full paths.
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    // A query containing a separator is about where a file lives, not just what
    // it is called, so it is matched against the whole path instead.
    let match_full_path = query.contains('/') || query.contains(std::path::MAIN_SEPARATOR);

    let mut haystack_buffer = Vec::new();
    let mut scored: Vec<(u32, usize)> = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        let haystack = if match_full_path {
            match entry.path().to_str() {
                Some(path) => path,
                // Indexed names are valid UTF-8, but a parent component may not
                // be; such an entry simply cannot take part in a path query.
                None => continue,
            }
        } else {
            entry.name()
        };

        let Some(base) = pattern.score(Utf32Str::new(haystack, &mut haystack_buffer), &mut matcher)
        else {
            continue;
        };
        scored.push((boost(base, entry, query, home), position));
    }

    // Rank by score, then prefer the shorter name (the more likely target),
    // then by path so equally-ranked results keep a stable order between
    // keystrokes.
    let better = |left: &(u32, usize), right: &(u32, usize)| {
        let (left_score, left_position) = *left;
        let (right_score, right_position) = *right;
        let left_entry = entries.get(left_position);
        let right_entry = entries.get(right_position);
        right_score
            .cmp(&left_score)
            .then_with(|| {
                let left_length = left_entry.map(|entry| entry.name().len()).unwrap_or(0);
                let right_length = right_entry.map(|entry| entry.name().len()).unwrap_or(0);
                left_length.cmp(&right_length)
            })
            .then_with(|| {
                let left_path = left_entry.map(IndexedEntry::path);
                let right_path = right_entry.map(IndexedEntry::path);
                left_path.cmp(&right_path)
            })
    };

    // Partition off the best `limit` before sorting. A broad query over a
    // home-sized index matches six figures of entries, and ordering all of them
    // to show fifty is most of the keystroke budget spent on rows nobody sees.
    if scored.len() > limit {
        scored.select_nth_unstable_by(limit, better);
        scored.truncate(limit);
    }
    scored.sort_unstable_by(better);

    // Match indices cost more than scoring, so they are only computed for the
    // rows that survived the cut.
    let mut indices = Vec::new();
    scored
        .into_iter()
        .filter_map(|(score, position)| {
            let entry = entries.get(position)?;
            let name = entry.name();
            indices.clear();
            // Highlighting always refers to the name, whichever haystack scored
            // the entry, so a path query that does not fit the name alone simply
            // highlights nothing rather than marking the wrong characters.
            if pattern
                .indices(
                    Utf32Str::new(name, &mut haystack_buffer),
                    &mut matcher,
                    &mut indices,
                )
                .is_some()
            {
                indices.sort_unstable();
                indices.dedup();
            } else {
                indices.clear();
            }

            Some(LauncherItem {
                path: entry.path().to_path_buf(),
                title: SharedString::from(name.to_string()),
                subtitle: SharedString::from(display_parent(entry.path(), home)),
                kind: if entry.is_dir() {
                    ItemKind::Folder
                } else {
                    ItemKind::File
                },
                score,
                title_matches: indices.clone(),
            })
        })
        .collect()
}

fn boost(base: u32, entry: &IndexedEntry, query: &str, home: Option<&Path>) -> u32 {
    let name = entry.name();
    let mut score = base;

    if name.eq_ignore_ascii_case(query) {
        score = score.saturating_add(EXACT_NAME_BONUS);
    } else if name.len() >= query.len()
        && name
            .get(..query.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query))
    {
        score = score.saturating_add(PREFIX_BONUS);
    }

    let depth = home
        .map(|home| entry.depth_below(home))
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let penalty = depth
        .saturating_mul(DEPTH_PENALTY_PER_LEVEL)
        .min(MAX_DEPTH_PENALTY);
    score.saturating_sub(penalty)
}

/// Renders the containing directory for display, abbreviating the home prefix to
/// `~` the way a shell prompt does.
fn display_parent(path: &Path, home: Option<&Path>) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    match home.and_then(|home| parent.strip_prefix(home).ok()) {
        Some(relative) if relative.as_os_str().is_empty() => "~".to_string(),
        Some(relative) => format!("~/{}", relative.display()),
        None => parent.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn entry(path: &str, is_dir: bool) -> IndexedEntry {
        IndexedEntry::new(PathBuf::from(path), is_dir).expect("test paths are displayable")
    }

    fn titles(items: &[LauncherItem]) -> Vec<&str> {
        items.iter().map(|item| item.title.as_ref()).collect()
    }

    #[test]
    fn empty_query_yields_nothing() {
        let entries = vec![entry("/home/u/notes.md", false)];
        assert!(rank(&entries, "", 10, None).is_empty());
        assert!(rank(&entries, "   ", 10, None).is_empty());
        // A zero limit is honoured rather than treated as unlimited.
        assert!(rank(&entries, "notes", 0, None).is_empty());
    }

    #[test]
    fn fuzzy_matching_finds_subsequences_and_drops_non_matches() {
        let entries = vec![
            entry("/home/u/launcher_view.rs", false),
            entry("/home/u/readme.md", false),
        ];
        let items = rank(&entries, "lvr", 10, None);
        assert_eq!(titles(&items), vec!["launcher_view.rs"]);
        assert!(rank(&entries, "zzzz", 10, None).is_empty());
    }

    #[test]
    fn exact_and_prefix_names_outrank_incidental_matches() {
        let home = PathBuf::from("/home/u");
        let entries = vec![
            entry("/home/u/deep/nested/notes-archive-2019.md", false),
            entry("/home/u/notes.md", false),
        ];
        let items = rank(&entries, "notes.md", 10, Some(&home));
        assert_eq!(titles(&items).first(), Some(&"notes.md"));
        assert!(items[0].score > items[1].score);
    }

    #[test]
    fn shallower_paths_win_ties() {
        let home = PathBuf::from("/home/u");
        let entries = vec![
            entry("/home/u/a/b/c/d/config.toml", false),
            entry("/home/u/config.toml", false),
        ];
        let items = rank(&entries, "config", 10, Some(&home));
        assert_eq!(items[0].path, PathBuf::from("/home/u/config.toml"));
    }

    #[test]
    fn a_query_with_a_separator_matches_the_whole_path() {
        let home = PathBuf::from("/home/u");
        let entries = vec![
            entry("/home/u/project/src/main.rs", false),
            entry("/home/u/other/main.rs", false),
        ];
        let items = rank(&entries, "src/main", 10, Some(&home));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, PathBuf::from("/home/u/project/src/main.rs"));
    }

    #[test]
    fn match_indices_point_into_the_title() {
        let entries = vec![entry("/home/u/main.rs", false)];
        let items = rank(&entries, "man", 10, None);
        // The "m", "a" and "n" of "m-a-i-n.rs": the `n` is the fourth character,
        // so the indices are positions in the title, not in the query.
        assert_eq!(items[0].title_matches, vec![0, 1, 3]);
        assert!(
            items[0]
                .title_matches
                .iter()
                .all(|index| (*index as usize) < items[0].title.chars().count())
        );
    }

    #[test]
    fn a_path_query_leaves_the_title_unhighlighted_rather_than_mismarked() {
        let home = PathBuf::from("/home/u");
        let entries = vec![entry("/home/u/project/src/main.rs", false)];
        let items = rank(&entries, "project/main", 10, Some(&home));
        assert_eq!(items.len(), 1);
        // "project" is nowhere in "main.rs", so nothing is highlighted.
        assert!(items[0].title_matches.is_empty());
    }

    #[test]
    fn subtitles_abbreviate_home_and_kinds_follow_the_entry() {
        let home = PathBuf::from("/home/u");
        let entries = vec![
            entry("/home/u/notes.md", false),
            entry("/home/u/docs/notes", true),
            entry("/etc/notes.conf", false),
        ];
        let items = rank(&entries, "notes", 10, Some(&home));

        let by_path = |path: &str| {
            items
                .iter()
                .find(|item| item.path == Path::new(path))
                .expect("entry should have matched")
        };
        assert_eq!(by_path("/home/u/notes.md").subtitle.as_ref(), "~");
        assert_eq!(by_path("/home/u/docs/notes").subtitle.as_ref(), "~/docs");
        assert_eq!(by_path("/home/u/docs/notes").kind, ItemKind::Folder);
        // Outside home the path is shown in full.
        assert_eq!(by_path("/etc/notes.conf").subtitle.as_ref(), "/etc");
        assert_eq!(by_path("/etc/notes.conf").kind, ItemKind::File);
    }

    #[test]
    fn results_are_capped_and_ordered_stably() {
        let entries: Vec<IndexedEntry> = (0..50)
            .map(|number| entry(&format!("/home/u/note-{number:02}.md"), false))
            .collect();
        let items = rank(&entries, "note", 5, None);
        assert_eq!(items.len(), 5);
        // Equal scores fall back to path order, so the result does not shuffle
        // between identical queries.
        assert_eq!(rank(&entries, "note", 5, None), items);
    }
}
