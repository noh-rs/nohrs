use crate::explorer::ExplorerPane;
use gpui::prelude::FluentBuilder;
use gpui::*;
use nohrs_ui::theme::theme;

/// The explorer header with navigation controls and the path bar.
pub mod header;
/// The main file listing, in list or grid mode, with the search bar.
pub mod listing;
/// The file preview pane.
pub mod preview;
/// The explorer sidebar with quick-access locations.
pub mod sidebar;

/// Renders the explorer page: header, sidebar, listing, and preview panes.
pub fn render(
    page: &mut ExplorerPane,
    window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    page.ensure_loaded();
    page.update_editor_search(window, cx);
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
            let modifiers = event.keystroke.modifiers;
            let with_modifier = modifiers.platform || modifiers.control;
            let is_f = key_lc == "f" || event.keystroke.key == "KeyF";
            let close_with_escape = key_lc == "escape" && this.search_visible;
            if (is_f && with_modifier) || close_with_escape {
                this.toggle_search(window, cx);
                cx.stop_propagation();
                return;
            }
            // Selection model (§5). Escape while searching is handled above, so
            // here it only clears the selection.
            match key_lc.as_str() {
                "a" if with_modifier => {
                    this.select_all();
                    cx.stop_propagation();
                    cx.notify();
                }
                // Search-close Escape is handled by the branch above (which
                // returns early), so here Escape only clears a selection — and
                // only consumes the event when there was one to clear, leaving
                // an empty-selection Escape free to bubble.
                "escape" if !this.selection.is_empty() => {
                    this.clear_selection();
                    cx.stop_propagation();
                    cx.notify();
                }
                "up" => {
                    this.move_active(-1, modifiers.shift);
                    cx.stop_propagation();
                    cx.notify();
                }
                "down" => {
                    this.move_active(1, modifiers.shift);
                    cx.stop_propagation();
                    cx.notify();
                }
                _ => {}
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
                gpui_component::resizable::h_resizable("file-explorer")
                    .with_state(&page.resizable)
                    .child(
                        // Keep the panel in the resizable's child list even when
                        // hidden (toggle via `.visible`), so the persisted panel
                        // sizes/indices stay stable; dropping the child outright
                        // would make the listing inherit the sidebar's slot.
                        gpui_component::resizable::resizable_panel()
                            .size(px(180.0))
                            .size_range(px(180.0)..px(360.0))
                            .visible(page.sidebar_visible)
                            .when(page.sidebar_visible, |panel| {
                                panel.child(
                                    div()
                                        .size_full()
                                        .overflow_hidden()
                                        .border_r_1()
                                        .border_color(rgb(theme::BORDER))
                                        .child(sidebar::render(page, window, cx)),
                                )
                            }),
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

/// Returns highlight ranges for every case-insensitive occurrence of `query`
/// within `text`, for emphasizing search matches.
pub fn find_query_highlights(
    text: &str,
    query: &str,
) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> {
    let mut highlights = Vec::new();
    if query.is_empty() {
        return highlights;
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let text_chars: Vec<(usize, char)> = text.char_indices().collect();

    let mut i = 0;
    while i < text_chars.len() {
        let mut match_found = true;
        let mut q_idx = 0;

        let mut current_t_offset = 0;

        while q_idx < query_lower.len() {
            if i + current_t_offset >= text_chars.len() {
                match_found = false;
                break;
            }

            let (_, t_char) = text_chars[i + current_t_offset];
            let t_lower = t_char.to_lowercase();

            for tc in t_lower {
                if q_idx >= query_lower.len() || query_lower[q_idx] != tc {
                    match_found = false;
                    break;
                }
                q_idx += 1;
            }

            if !match_found {
                break;
            }
            current_t_offset += 1;
        }

        if match_found && q_idx == query_lower.len() {
            let start_byte = text_chars[i].0;
            let end_byte = if i + current_t_offset < text_chars.len() {
                text_chars[i + current_t_offset].0
            } else {
                text.len()
            };

            highlights.push((
                start_byte..end_byte,
                gpui::HighlightStyle {
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
                },
            ));

            i += current_t_offset;
        } else {
            i += 1;
        }
    }

    highlights.retain(|(range, _)| {
        let start_ok = text.is_char_boundary(range.start);
        let end_ok = text.is_char_boundary(range.end);
        if !start_ok || !end_ok {
            if std::env::var("NOHR_DEBUG").is_ok() {
                tracing::error!("[CRITICAL] find_query_highlights: Removing invalid highlight: {:?} (start_ok={}, end_ok={}) in text len {}", range, start_ok, end_ok, text.len());
            }
            return false;
        }
        true
    });

    highlights
}
