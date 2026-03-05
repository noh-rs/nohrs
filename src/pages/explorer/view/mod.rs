use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme;
use gpui::*;

pub mod header;
pub mod listing;
pub mod preview;
pub mod sidebar;

pub fn render(
    page: &mut ExplorerPage,
    window: &mut Window,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    page.ensure_loaded();
    if !page.focus_requested {
        page.focus_requested = true;
        cx.focus_self(window);
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .relative()
        .track_focus(&page.focus_handle)
        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            let key_lc = event.keystroke.key.to_lowercase();
            let is_f = key_lc == "f" || event.keystroke.key == "KeyF";
            if is_f && (event.keystroke.modifiers.platform || event.keystroke.modifiers.control) {
                this.toggle_search(window, cx);
                cx.stop_propagation();
            } else if key_lc == "escape" && this.search_visible {
                this.toggle_search(window, cx);
                cx.stop_propagation();
            }
        }))
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                if this.resizing_column.is_some() {
                    this.update_column_resize(event.position);
                    cx.notify();
                }
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                if this.resizing_column.is_some() {
                    this.stop_column_resize();
                    cx.notify();
                }
            }),
        )
        .child(header::render(page, window, cx))
        .child(
            div().flex().flex_row().flex_grow().min_h(px(0.0)).child(
                gpui_component::resizable::h_resizable("file-explorer", page.resizable.clone())
                    .child(
                        gpui_component::resizable::resizable_panel()
                            .size(px(180.0))
                            .size_range(px(180.0)..px(360.0))
                            .child(
                                div()
                                    .size_full()
                                    .overflow_hidden()
                                    .border_r_1()
                                    .border_color(rgb(theme::BORDER))
                                    .child(sidebar::render(page, window, cx)),
                            ),
                    )
                    .child(
                        gpui_component::resizable::resizable_panel().child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .min_h(px(0.0))
                                .overflow_hidden()
                                .child(listing::render(page, window, cx)),
                        ),
                    )
                    .child(
                        gpui_component::resizable::resizable_panel()
                            .size(px(240.0))
                            .size_range(px(240.0)..px(2000.0))
                            .child(
                                div()
                                    .size_full()
                                    .overflow_hidden()
                                    .border_l_1()
                                    .border_color(rgb(theme::BORDER))
                                    .child(preview::render(page, window)),
                            ),
                    )
                    .into_any_element(),
            ),
        )
}

/// マッチ範囲に HighlightStyle を付与する薄いラッパー
/// ロジック本体は crate::core::text::find_query_match_ranges
pub fn find_query_highlights(
    text: &str,
    query: &str,
) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> {
    use crate::core::text::find_query_match_ranges;

    let highlight_style = gpui::HighlightStyle {
        background_color: Some(gpui::Hsla::from(gpui::Rgba {
            r: 1.0,
            g: 0.9,
            b: 0.0,
            a: 0.5,
        })),
        color: Some(gpui::Hsla::from(gpui::Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        })),
        ..Default::default()
    };

    find_query_match_ranges(text, query)
        .into_iter()
        .map(|range| (range, highlight_style))
        .collect()
}
