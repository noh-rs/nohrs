use super::truncate_middle;

use crate::pages::explorer::ExplorerPage;
use crate::services::fs::listing::FileEntryDto;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{Icon, IconName, ListItem};

mod snippets;

pub fn render(
    page: &ExplorerPage,
    item: &FileEntryDto,
    ix: usize,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    use crate::ui::components::file_list::{format_date, get_file_type, human_bytes};

    let icon_name = match item.kind.as_str() {
        "dir" => IconName::Folder,
        _ => IconName::File,
    };
    let icon_color = match item.kind.as_str() {
        "dir" => rgb(theme::ACCENT),
        _ => rgb(theme::GRAY_600),
    };
    let bg_color = if ix % 2 == 0 {
        theme::BG
    } else {
        theme::GRAY_50
    };

    let file_type = get_file_type(&item.name, &item.kind);
    let max_chars = (page.col_name_width / 8.0) as usize;
    let display_name = truncate_middle(&item.name, max_chars.max(20));
    let total_width = page.total_table_width();
    let item_for_preview = item.clone();
    let item_for_activate = item.clone();

    let query_lower = page.search_query.to_lowercase();
    let has_filename_match =
        !page.search_query.is_empty() && item.name.to_lowercase().contains(&query_lower);
    // O(1) HashMap ルックアップ (4.2.6)
    let has_content_matches = page
        .get_search_result(&item.path)
        .map(|r| !r.matches.is_empty())
        .unwrap_or(false);

    let is_expanded = page.expanded_search_files.contains(&item.path);
    let match_snippets = snippets::collect_snippets(page, item, is_expanded, has_content_matches);
    let query = page.search_query.clone();
    let path_for_toggle = item.path.clone();

    let styled_name = if has_filename_match && !page.search_query.is_empty() {
        let highlights = crate::pages::explorer::view::find_query_highlights(&display_name, &query);
        StyledText::new(display_name.clone()).with_highlights(highlights)
    } else {
        StyledText::new(display_name.clone())
    };

    let name_cell = render_name_cell(
        page, has_content_matches, is_expanded, path_for_toggle,
        icon_name, icon_color, styled_name, cx,
    );

    div()
        .flex()
        .flex_col()
        .w(px(total_width))
        .child(
            ListItem::new(("file-row", ix))
                .w(px(total_width))
                .h(px(32.0))
                .px(px(24.0))
                .bg(rgb(bg_color))
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        if let gpui::ClickEvent::Mouse(mouse) = event {
                            if mouse.up.button == gpui::MouseButton::Left {
                                this.record_click(ix, mouse.up.click_count);
                                this.selected_index = Some(ix);
                                if item_for_preview.kind == "file" {
                                    this.open_preview(item_for_preview.path.clone(), window, cx);
                                }
                                if mouse.up.click_count >= 2 {
                                    this.activate_entry(item_for_activate.clone(), window, cx);
                                }
                            }
                        }
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .h_full()
                        .child(name_cell)
                        .child(
                            div()
                                .w(px(page.col_type_width))
                                .flex_shrink_0()
                                .text_sm()
                                .text_color(rgb(theme::FG_SECONDARY))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(file_type),
                        )
                        .child(
                            div()
                                .w(px(page.col_size_width))
                                .flex_shrink_0()
                                .text_sm()
                                .text_color(rgb(theme::FG_SECONDARY))
                                .child(match item.kind.as_str() {
                                    "file" => human_bytes(item.size),
                                    "dir" => "-".to_string(),
                                    other => other.to_string(),
                                }),
                        )
                        .child(
                            div()
                                .w(px(page.col_modified_width))
                                .flex_shrink_0()
                                .text_sm()
                                .text_color(rgb(theme::FG_SECONDARY))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(format_date(&item.modified)),
                        ),
                ),
        )
        .children(snippets::render_snippets(match_snippets, &item.path, &query, cx))
}

fn render_name_cell(
    page: &ExplorerPage,
    has_content_matches: bool,
    is_expanded: bool,
    path_for_toggle: String,
    icon_name: IconName,
    icon_color: Rgba,
    styled_name: StyledText,
    cx: &mut Context<ExplorerPage>,
) -> Div {
    let expand_icon = if is_expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };

    div()
        .flex()
        .items_center()
        .gap_1()
        .w(px(page.col_name_width))
        .flex_shrink_0()
        .when(has_content_matches, |this| {
            this.child(
                div()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)).rounded(px(4.0)))
                    .p(px(2.0))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener({
                            let path = path_for_toggle;
                            move |this, _, _, cx| {
                                if this.expanded_search_files.contains(&path) {
                                    this.expanded_search_files.remove(&path);
                                } else {
                                    this.expanded_search_files.insert(path.clone());
                                }
                                this.update_item_sizes();
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        Icon::new(expand_icon)
                            .size_3()
                            .text_color(rgb(theme::GRAY_600)),
                    ),
            )
        })
        .when(!has_content_matches, |this| this.child(div().w(px(20.0))))
        .child(Icon::new(icon_name).size_4().text_color(icon_color))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme::FG))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(styled_name),
        )
}
