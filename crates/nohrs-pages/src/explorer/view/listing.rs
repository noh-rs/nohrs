use super::super::types::ViewMode;
use crate::explorer::ExplorerPane;
use gpui::*;

/// Grid-mode rendering of the listing.
pub mod grid;
/// List-mode rendering of the listing.
pub mod list;
/// Rendering of a single listing row.
pub mod row;
/// The full-text search bar shown above the listing.
pub mod search_bar;

/// Renders the file listing in the active view mode, with the search bar when
/// search is visible.
pub fn render(
    page: &mut ExplorerPane,
    window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> AnyElement {
    page.ensure_list_initialized(window, cx);

    let file_list = match page.view_mode {
        ViewMode::List => list::render(page, cx),
        ViewMode::Grid => grid::render(page, window, cx),
    };

    if page.search_visible {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(search_bar::render(page, cx))
            .child(file_list)
            .into_any_element()
    } else {
        file_list
    }
}

/// Truncates `text` to at most `max_len` characters by eliding the middle,
/// preserving the file extension where possible.
pub fn truncate_middle(text: &str, max_len: usize) -> String {
    // Width reserved for the "..." elision marker.
    const ELLIPSIS: usize = 3;

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_len {
        return text.to_string();
    }

    // Too little room for the "..." marker plus any content: just take the
    // leading characters so the result never exceeds `max_len` (the contract
    // above). Callers pass `max_len >= 20`, so this only guards pathological
    // inputs, but keeping the function self-consistent avoids surprises.
    if max_len <= ELLIPSIS {
        return chars[..max_len].iter().collect();
    }

    // Prefer eliding the middle of the *name* so the extension stays visible.
    // `checked_sub` guards the case where the extension alone (plus the marker)
    // already exceeds `max_len`: in debug builds the previous `max_len - ext - 3`
    // panicked on overflow, in release it would have wrapped to a huge budget.
    if let Some(dot_pos) = text.rfind('.') {
        let name_chars: Vec<char> = text[..dot_pos].chars().collect();
        let ext_part = &text[dot_pos..];
        let ext_chars = ext_part.chars().count();

        if let Some(budget) = max_len.checked_sub(ext_chars + ELLIPSIS) {
            if name_chars.len() > budget {
                let keep_start = budget / 2;
                let keep_end = budget - keep_start;

                let start_part: String = name_chars[..keep_start].iter().collect();
                let end_part: String = name_chars[name_chars.len() - keep_end..].iter().collect();

                return format!("{}...{}{}", start_part, end_part, ext_part);
            }
            return text.to_string();
        }
    }

    // No usable extension, or one too long to preserve: hard-truncate the whole
    // string, keeping the head and tail around the marker.
    let budget = max_len.saturating_sub(ELLIPSIS);
    let keep_start = budget / 2;
    let keep_end = budget - keep_start;

    let start_part: String = chars[..keep_start].iter().collect();
    let end_part: String = chars[chars.len() - keep_end..].iter().collect();

    format!("{}...{}", start_part, end_part)
}

#[cfg(test)]
mod truncate_middle_tests {
    use super::truncate_middle;

    #[test]
    fn keeps_short_names_unchanged() {
        assert_eq!(truncate_middle("Cargo.toml", 20), "Cargo.toml");
    }

    #[test]
    fn elides_middle_preserving_extension() {
        let out = truncate_middle("a-very-long-file-name.rs", 16);
        assert!(out.contains("..."));
        assert!(out.ends_with(".rs"));
        assert!(out.chars().count() <= 16);
    }

    #[test]
    fn does_not_panic_when_extension_exceeds_budget() {
        // Regression: an extension longer than `max_len - 3` previously
        // underflowed `max_len - ext - 3` and panicked in debug builds. Such
        // names reach this code path via search results.
        let _ = truncate_middle("x.0123456789012345678901234567890", 20);
        let _ = truncate_middle("no-dot-but-extremely-long-name-here", 8);
        let _ = truncate_middle(".gitignore", 5);
    }

    #[test]
    fn never_exceeds_max_len_even_below_ellipsis_width() {
        // The result must honor the "at most `max_len` characters" contract for
        // every budget, including ones too small to fit the "..." marker.
        for max_len in 0..=6 {
            let out = truncate_middle("really-long-filename.rs", max_len);
            assert!(
                out.chars().count() <= max_len,
                "max_len={max_len} produced {out:?}"
            );
        }
    }
}
