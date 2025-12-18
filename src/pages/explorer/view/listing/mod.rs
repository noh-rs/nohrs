use super::super::types::ViewMode;
use crate::pages::explorer::ExplorerPage;
use gpui::*;

pub mod grid;
pub mod list;
pub mod row;
pub mod search_bar;

pub fn render(
    page: &mut ExplorerPage,
    window: &mut Window,
    cx: &mut Context<ExplorerPage>,
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

pub fn truncate_middle(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();

    if char_count <= max_len {
        return text.to_string();
    }

    if let Some(dot_pos) = text.rfind('.') {
        let name_part = &text[..dot_pos];
        let ext_part = &text[dot_pos..];
        let name_chars: Vec<char> = name_part.chars().collect();
        let ext_chars = ext_part.chars().count();

        if name_chars.len() > max_len - ext_chars - 3 {
            let keep_start = (max_len - ext_chars - 3) / 2;
            let keep_end = (max_len - ext_chars - 3) - keep_start;

            let start_part: String = name_chars[..keep_start].iter().collect();
            let end_part: String = name_chars[name_chars.len() - keep_end..].iter().collect();

            format!("{}...{}{}", start_part, end_part, ext_part)
        } else {
            text.to_string()
        }
    } else {
        let chars: Vec<char> = text.chars().collect();
        let keep_start = (max_len - 3) / 2;
        let keep_end = (max_len - 3) - keep_start;

        let start_part: String = chars[..keep_start].iter().collect();
        let end_part: String = chars[chars.len() - keep_end..].iter().collect();

        format!("{}...{}", start_part, end_part)
    }
}
