use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme;
use gpui::*;

pub fn render(page: &mut ExplorerPage, window: &mut Window) -> impl IntoElement {
    let title = page
        .preview_path
        .as_ref()
        .map(|p| path_name(p))
        .unwrap_or_else(|| "Preview".to_string());

    let line_count = page.preview_lines.len().max(1);
    let max_digits = line_count.to_string().len();

    let query = page.search_query.clone();

    // Virtual Scrolling Constants
    let row_height_px = px(20.0);
    // Use window height as approximation
    let viewport_height = window.viewport_size().height;

    // Scroll handle returns Point<Pixels>
    let scroll_y = page.preview_scroll_handle.offset().y.abs();

    let visible_lines = (viewport_height / row_height_px) as usize + 1;
    let start_line = (scroll_y / row_height_px) as usize;
    let buffer = 20; // Extra lines

    let render_start = start_line.saturating_sub(buffer);
    let render_end = (start_line + visible_lines + buffer).min(page.preview_lines.len());

    let padding_top = render_start as f32 * 20.0;
    let padding_bottom = (page.preview_lines.len().saturating_sub(render_end)) as f32 * 20.0;

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .child(
            div()
                .px(px(16.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme::FG))
                        .child(title),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .px(px(16.0))
                .py(px(16.0))
                .child(
                    div()
                        .id("preview-scroll")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&page.preview_scroll_handle)
                        .flex()
                        .flex_col()
                        .text_sm()
                        .font_family("Mono")
                        .line_height(row_height_px)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .pt(px(padding_top))
                                .pb(px(padding_bottom))
                                .children(
                                    page.preview_lines
                                        .iter()
                                        .enumerate()
                                        .skip(render_start)
                                        .take(render_end - render_start)
                                        .map(|(ix, line_content)| {
                                            let line_num = ix + 1;
                                            let num_str =
                                                format!("{:>width$}", line_num, width = max_digits);

                                            let syntax = page
                                                .preview_line_highlights
                                                .get(&ix)
                                                .cloned()
                                                .unwrap_or_default();

                                            let query_highlights = if !query.is_empty() {
                                                super::find_query_highlights(line_content, &query)
                                            } else {
                                                Vec::new()
                                            };

                                            let mut combined: Vec<(
                                                std::ops::Range<usize>,
                                                gpui::HighlightStyle,
                                            )> = syntax;
                                            combined.extend(query_highlights);

                                            let mut points: Vec<(usize, bool, usize)> = Vec::new();
                                            for (i, (range, _)) in combined.iter().enumerate() {
                                                points.push((range.start, true, i));
                                                points.push((range.end, false, i));
                                            }
                                            points.sort_by(|a, b| {
                                                a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
                                            });

                                            let mut flattened: Vec<(
                                                std::ops::Range<usize>,
                                                gpui::HighlightStyle,
                                            )> = Vec::new();
                                            let mut active: Vec<usize> = Vec::new();
                                            let mut prev = 0;

                                            for (pos, is_start, idx) in points {
                                                if pos > prev {
                                                    if line_content.is_char_boundary(prev)
                                                        && line_content.is_char_boundary(pos)
                                                    {
                                                        if let Some(&top) = active.last() {
                                                            flattened.push((
                                                                prev..pos,
                                                                combined[top].1.clone(),
                                                            ));
                                                        }
                                                    }
                                                }
                                                if is_start {
                                                    active.push(idx);
                                                } else {
                                                    if let Some(p) =
                                                        active.iter().rposition(|&x| x == idx)
                                                    {
                                                        active.remove(p);
                                                    }
                                                }
                                                prev = pos;
                                            }

                                            let styled = StyledText::new(line_content.clone())
                                                .with_highlights(flattened);

                                            div()
                                                .flex()
                                                .items_start()
                                                .gap_4()
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .text_color(rgb(theme::MUTED))
                                                        .child(num_str),
                                                )
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .whitespace_normal()
                                                        .child(styled),
                                                )
                                        }),
                                ),
                        ),
                ),
        )
}

fn path_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}
