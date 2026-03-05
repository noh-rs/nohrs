use crate::pages::explorer::ExplorerPage;
use crate::services::fs::listing::FileEntryDto;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;

pub(super) fn collect_snippets(
    page: &ExplorerPage,
    item: &FileEntryDto,
    is_expanded: bool,
    has_content_matches: bool,
) -> Vec<(usize, String)> {
    if is_expanded && has_content_matches {
        page.search_results
            .as_ref()
            .and_then(|results| {
                results.iter().find(|r| r.path == item.path).map(|r| {
                    r.matches
                        .iter()
                        .take(10)
                        .map(|m| (m.line_number, m.line_content.clone()))
                        .collect()
                })
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub(super) fn render_snippets(
    snippets: Vec<(usize, String)>,
    path: &str,
    query: &str,
    cx: &mut Context<ExplorerPage>,
) -> Vec<AnyElement> {
    snippets
        .into_iter()
        .map(|(line_num, content)| {
            let highlights =
                crate::pages::explorer::view::find_query_highlights(&content, query);
            let styled = StyledText::new(content).with_highlights(highlights);
            let path = path.to_string();
            div()
                .id(SharedString::from(format!(
                    "snippet-{}-{}",
                    path, line_num
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
                .into_any_element()
        })
        .collect()
}
