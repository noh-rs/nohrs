use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;

/// Calculate the maximum line width in characters for horizontal scroll sizing
fn calculate_max_line_width(lines: &[String]) -> usize {
    lines.iter().map(|l| l.chars().count()).max().unwrap_or(80)
}

pub fn render(page: &mut ExplorerPage, window: &mut Window) -> impl IntoElement {
    let title = page
        .preview_path
        .as_ref()
        .map(|p| path_name(p))
        .unwrap_or_else(|| "Preview".to_string());

    let line_count = page.preview_lines.len().max(1);
    let max_digits = line_count.to_string().len();

    let query = page.search_query.clone();

    let content = if let Some(editor) = &page.preview_editor {
        div().flex_1().child(editor.clone()).into_any_element()
    } else {
        // Virtual Scrolling Constants
        let row_height: f32 = 20.0;
        let row_height_px = px(row_height);

        // Calculate content dimensions
        let total_lines = page.preview_lines.len();
        let total_content_height = total_lines as f32 * row_height;

        // Use a conservative estimate for viewport since we don't have exact panel height
        // The key insight: we'll use a FIXED scrollbar size (percentage of track)
        // and calculate position purely based on scroll ratio

        // Scroll handle returns Point<Pixels> - y is negative when scrolled down
        let raw_scroll_y = page.preview_scroll_handle.offset().y;
        let scroll_y = (-f32::from(raw_scroll_y)).max(0.0);

        // Window height for virtual scroll estimation (not for scrollbar)
        let window_height = f32::from(window.viewport_size().height);

        // Virtual scroll calculations
        let visible_lines = ((window_height / row_height) as usize).max(20);
        let start_line = (scroll_y / row_height) as usize;
        let buffer = 20;
        let render_start = start_line.saturating_sub(buffer);
        let render_end = (start_line + visible_lines + buffer).min(total_lines);
        let padding_top = render_start as f32 * row_height;
        let remaining = total_lines.saturating_sub(render_end);
        let padding_bottom = remaining as f32 * row_height;

        // Calculate max line width for horizontal scrolling
        let max_chars = calculate_max_line_width(&page.preview_lines);
        // Approximate: ~8px per character + line number width + gaps
        let line_number_width = (max_digits as f32 * 10.0) + 32.0; // padding + gap
        let content_width = (max_chars as f32 * 8.0) + line_number_width + 64.0; // extra padding

        // SCROLLBAR: Use percentage-based calculation
        // The key is to NOT depend on exact panel height for scrollbar sizing
        // Instead, use the content height ratio
        let show_scrollbar = total_content_height > window_height * 0.5; // rough estimate

        // Scrollbar height: fixed ratio of the visible portion
        // If content is 2x viewport, scrollbar is 50% of track
        // If content is 4x viewport, scrollbar is 25% of track, min 30px
        let scrollbar_ratio = if total_content_height > 0.0 {
            (window_height / total_content_height).clamp(0.1, 1.0)
        } else {
            1.0
        };

        // For position: what percentage through the content are we?
        let max_scroll = (total_content_height - window_height * 0.3).max(1.0);
        let scroll_progress = (scroll_y / max_scroll).clamp(0.0, 1.0);

        div()
            .flex_1()
            .overflow_hidden()
            .relative()
            .child(
                // Scrollable content
                div()
                    .id("preview-scroll")
                    .absolute()
                    .inset(px(0.0))
                    .overflow_scroll()
                    .track_scroll(&page.preview_scroll_handle)
                    .child(
                        // Content container with explicit width for horizontal scroll
                        div()
                            .min_w(px(content_width))
                            .pt(px(padding_top + 16.0)) // Add padding
                            .pb(px(padding_bottom + 16.0))
                            .px(px(16.0))
                            .text_sm()
                            .font_family("Mono")
                            .line_height(row_height_px)
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

                                        // Each line row
                                        div()
                                            .h(row_height_px)
                                            .flex()
                                            .items_center()
                                            .gap_4()
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .text_color(rgb(theme::MUTED))
                                                    .child(num_str),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .whitespace_nowrap()
                                                    .child(styled),
                                            )
                                    }),
                            ),
                    ),
            )
            .when(show_scrollbar, |this| {
                // Use a fixed track height estimate for stable scrollbar
                let track_height: f32 = 400.0; // Fixed track height matching logic
                let thumb_height = (track_height * scrollbar_ratio).max(30.0);
                let thumb_top = scroll_progress * (track_height - thumb_height);

                this.child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .right(px(2.0))
                        .bottom(px(0.0))
                        .w(px(6.0))
                        .child(
                            // Scrollbar thumb
                            div()
                                .absolute()
                                .top(px(thumb_top))
                                .w_full()
                                .h(px(thumb_height))
                                .rounded(px(3.0))
                                .bg(rgb(theme::GRAY_400))
                                .opacity(0.6),
                        ),
                )
            })
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .child(
            // Header
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
        .child(content)
}

fn path_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

pub mod editor;
