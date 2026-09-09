use super::truncate_middle;

use crate::explorer::ExplorerPane;
use gpui::prelude::*;
use gpui::*;
use gpui_component::list::ListItem;
use gpui_component::{Icon, IconName};
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_ui::theme::theme;

/// Renders a single listing row for the given entry at row index `ix`.
pub fn render(
    page: &ExplorerPane,
    item: &FileEntryDto,
    ix: usize,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    use nohrs_ui::components::file_list::{format_date, get_file_type, human_bytes};

    let icon_name = match item.kind.as_str() {
        "dir" => IconName::Folder,
        _ => IconName::File,
    };
    let icon_color = match item.kind.as_str() {
        "dir" => rgb(theme::ACCENT),
        _ => rgb(theme::GRAY_600),
    };

    let bg_color = if page.is_selected(ix) {
        theme::ACCENT_LIGHT
    } else if ix % 2 == 0 {
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

    // Check if query matches filename (for highlighting)
    let query_lower = page.search_query.to_lowercase();
    let has_filename_match =
        !page.search_query.is_empty() && item.name.to_lowercase().contains(&query_lower);

    // Check if there are content matches (for expand arrow)
    let has_content_matches = page
        .search_results
        .as_ref()
        .map(|results| {
            results
                .iter()
                .any(|r| r.path == item.path && !r.matches.is_empty())
        })
        .unwrap_or(false);

    let is_expanded = page.expanded_search_files.contains(&item.path);

    let match_snippets: Vec<(usize, String)> = if is_expanded && has_content_matches {
        page.search_results
            .as_ref()
            .and_then(|results| {
                results.iter().find(|r| r.path == item.path).map(|r| {
                    r.matches
                        .iter()
                        .take(10) // Limit to 10 snippets
                        .map(|m| (m.line_number, m.line_content.clone()))
                        .collect()
                })
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let query = page.search_query.clone();
    let path_for_toggle = item.path.clone();
    let expand_icon = if is_expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };

    // Create styled filename with highlighted matches
    let styled_name = if has_filename_match && !page.search_query.is_empty() {
        let highlights = crate::explorer::view::find_query_highlights(&display_name, &query);
        StyledText::new(display_name.clone()).with_highlights(highlights)
    } else {
        StyledText::new(display_name.clone())
    };

    // ... rendering ...
    // Note: page.total_table_width() method is needed.
    // I need to make `total_table_width` pub on ExplorerPane. I did make fields pub, but method?
    // I should check if `total_table_width` is a method. Yes (line 347 in original).
    // I need to ensure that method is `pub` or copy logic.
    // Copying logic is safer: `page.col_name_width + ...`.
    // I'll copy the logic.

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
                                let modifiers = mouse.up.modifiers;
                                if modifiers.shift {
                                    this.select_range_to(ix);
                                } else if modifiers.platform || modifiers.control {
                                    this.toggle_select(ix);
                                } else {
                                    this.select_single(ix);
                                }
                                if item_for_preview.kind == "file" {
                                    this.open_preview(item_for_preview.path.clone(), window, cx);
                                }
                                if mouse.up.click_count >= 2 {
                                    this.activate_entry(item_for_activate.clone(), window, cx);
                                }
                                cx.notify();
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
                        .child(
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
                                                    let path = path_for_toggle.clone();
                                                    move |this, _, _, cx| {
                                                        if this
                                                            .expanded_search_files
                                                            .contains(&path)
                                                        {
                                                            this.expanded_search_files
                                                                .remove(&path);
                                                        } else {
                                                            this.expanded_search_files
                                                                .insert(path.clone());
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
                                ),
                        )
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
        .children(
            match_snippets
                .into_iter()
                .map(|(line_num, content)| {
                    let highlights = crate::explorer::view::find_query_highlights(&content, &query);
                    let styled = StyledText::new(content.clone()).with_highlights(highlights);
                    let path = item.path.clone();
                    div()
                        .id(SharedString::from(format!(
                            "snippet-{}-{}",
                            path.clone(),
                            line_num
                        )))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.scroll_to_line(line_num, window, cx);
                            cx.notify();
                        }))
                        .h(px(24.0))
                        .pl(px(48.0))
                        .pr(px(24.0))
                        .bg(rgb(theme::GRAY_50))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::MUTED))
                                .w(px(32.0))
                                .child(format!("{}", line_num)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::FG_SECONDARY))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(styled),
                        )
                })
                .collect::<Vec<_>>(),
        )
}
