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
            .child(search_bar::render(page, window, cx))
            .child(file_list)
            .into_any_element()
    } else {
        file_list
    }
}

// 純粋ロジックは core から再エクスポート
pub use crate::core::text::truncate_middle;
