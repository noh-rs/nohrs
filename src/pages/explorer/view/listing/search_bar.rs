use crate::pages::explorer::types::SearchType;
use crate::pages::explorer::ExplorerPage;
use crate::services::search::SearchScope;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;
use gpui_component::input::TextInput;
use gpui_component::{Icon, IconName, ListItem};

pub fn render(page: &mut ExplorerPage, cx: &mut Context<ExplorerPage>) -> impl IntoElement {
    let current_text = page.search_input.read(cx).text().to_string();
    if current_text != page.search_query {
        page.search_query = current_text;
        // If query changed, revert to file list filter until user explicitly triggers search?
        // Or we could auto-search? Auto-search is expensive for full content.
        // So default to file filter.
        page.search_results = None;
        page.apply_filter();
    }

    let is_empty = page.search_query.is_empty();
    let match_count = if let Some(results) = &page.search_results {
        results.iter().map(|r| r.matches.len()).sum()
    } else {
        page.filtered_entries.len()
    };
    let is_full_search = page.search_results.is_some();
    let status_text = if is_full_search {
        format!("{} matches in content", match_count)
    } else if !is_empty {
        format!("{} files filtered", match_count)
    } else {
        String::new()
    };

    let _search_scope = page.search_scope;
    let _search_type = page.search_type;
    let match_case = page.match_case;
    let match_whole_word = page.match_whole_word;
    let use_regex = page.use_regex;

    div()
        .w_full()
        .bg(rgb(theme::BG))
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|_this, _ev: &gpui::MouseDownEvent, _window, cx| {
                cx.stop_propagation();
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|_this, _ev: &gpui::MouseUpEvent, _window, cx| {
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(
            cx.listener(|_this, _ev: &gpui::MouseMoveEvent, _window, cx| {
                cx.stop_propagation();
            }),
        )
        .on_scroll_wheel(cx.listener(|_this, _ev, _window, cx| {
            cx.stop_propagation();
        }))
        .child(
            // Scope Selection
            div()
                .flex()
                .gap_4()
                .px(px(12.0))
                .pt(px(8.0))
                .text_xs()
                .text_color(rgb(theme::FG_SECONDARY))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child("Scope:")
                        .child(render_scope_button(page, SearchScope::Home, "Home", cx))
                        .child(render_scope_button(page, SearchScope::Root, "Root", cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child("Type:")
                        .child(render_type_button(page, SearchType::All, "All", cx))
                        .child(render_type_button(
                            page,
                            SearchType::Filename,
                            "Filename",
                            cx,
                        ))
                        .child(render_type_button(page, SearchType::Content, "Content", cx)),
                ),
        )
        .child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .px(px(12.0))
                .py(px(10.0))
                .child(
                    Icon::new(IconName::Search)
                        .size_4()
                        .text_color(rgb(theme::FG_SECONDARY))
                        .mt(px(8.0)), // Align with input
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child({
                            let si = page.search_input.clone();
                            div()
                                .on_key_down(cx.listener(
                                    |this, event: &gpui::KeyDownEvent, window, cx| {
                                        if event.keystroke.key == "enter" {
                                            this.trigger_search(window, cx);
                                        } else if event.keystroke.key == "escape" {
                                            this.toggle_search(window, cx);
                                        }
                                    },
                                ))
                                .child(TextInput::new(&si))
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(render_toggle_button(
                                    "Aa",
                                    match_case,
                                    |this, cx| this.toggle_match_case(cx),
                                    cx,
                                ))
                                .child(render_toggle_button(
                                    "ab",
                                    match_whole_word,
                                    |this, cx| this.toggle_match_whole_word(cx),
                                    cx,
                                ))
                                .child(render_toggle_button(
                                    ".*",
                                    use_regex,
                                    |this, cx| this.toggle_use_regex(cx),
                                    cx,
                                ))
                                .child(div().flex_1())
                                .child(
                                    // Run Search Button
                                    div()
                                        .cursor_pointer()
                                        .bg(rgb(theme::ACCENT))
                                        .text_color(rgb(theme::BG))
                                        .px(px(8.0))
                                        .py(px(2.0))
                                        .rounded(px(4.0))
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child("Search")
                                        .hover(|this| this.opacity(0.8))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|this, _, window, cx| {
                                                this.trigger_search(window, cx);
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            div().h(px(18.0)).child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::FG_SECONDARY))
                                    .when(!status_text.is_empty(), |this| this.child(status_text)),
                            ),
                        ),
                )
                .child(
                    ListItem::new("close-search")
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.toggle_search(window, cx);
                        }))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(rgb(theme::MUTED))
                                .child("×"),
                        ),
                ),
        )
}

fn render_scope_button(
    page: &ExplorerPage,
    scope: SearchScope,
    label: &str,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    let is_active = page.search_scope == scope;
    div()
        .cursor_pointer()
        .px(px(4.0))
        .rounded(px(4.0))
        .when(is_active, |this| {
            this.bg(rgb(theme::ACCENT)).text_color(rgb(theme::BG))
        })
        .when(!is_active, |this| {
            this.hover(|s| s.bg(rgb(theme::BG_HOVER)))
        })
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.set_search_scope(scope, cx);
            }),
        )
        .child(label.to_string())
}

fn render_type_button(
    page: &ExplorerPage,
    search_type: SearchType,
    label: &str,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    let is_active = page.search_type == search_type;
    div()
        .cursor_pointer()
        .px(px(4.0))
        .rounded(px(4.0))
        .when(is_active, |this| {
            this.bg(rgb(theme::ACCENT)).text_color(rgb(theme::BG))
        })
        .when(!is_active, |this| {
            this.hover(|s| s.bg(rgb(theme::BG_HOVER)))
        })
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.search_type = search_type;
                cx.notify();
            }),
        )
        .child(label.to_string())
}

fn render_toggle_button(
    label: &str,
    active: bool,
    on_click: impl Fn(&mut ExplorerPage, &mut Context<ExplorerPage>) + 'static + Copy,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    div()
        .cursor_pointer()
        .border_1()
        .border_color(if active {
            rgb(theme::ACCENT)
        } else {
            rgb(theme::BORDER)
        })
        .bg(if active {
            rgb(theme::ACCENT_LIGHT)
        } else {
            rgb(theme::BG)
        })
        .px(px(4.0))
        .rounded(px(4.0))
        .text_xs()
        .font_family("Mono")
        .child(label.to_string())
        .hover(|this| this.bg(rgb(theme::BG_HOVER)))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                on_click(this, cx);
            }),
        )
}
