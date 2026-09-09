use crate::theme::theme;
use gpui::{IntoElement, div, prelude::*, px, rgb};

/// Non-functional tab bar (placeholder)
pub fn tab_bar() -> impl IntoElement {
    div()
        .flex()
        .gap_2()
        .p_2()
        .border_1()
        .border_color(rgb(theme::ACCENT))
        .bg(rgb(theme::BG))
        .text_color(rgb(theme::FG))
        .child(
            div()
                .px_2()
                .py_1()
                .border_1()
                .border_color(rgb(theme::ACCENT))
                .child("Tab 1"),
        )
        .child(
            div()
                .px_2()
                .py_1()
                .border_1()
                .border_color(rgb(theme::ACCENT))
                .child("Tab 2"),
        )
}

/// Split container with a vertical resize bar (non-functional placeholder)
pub fn split_container<L: IntoElement, R: IntoElement>(left: L, right: R) -> impl IntoElement {
    div()
        .flex()
        .gap_1()
        .child(
            div()
                .border_1()
                .border_color(rgb(theme::ACCENT))
                .child(left),
        )
        .child(
            // Resize bar placeholder
            div().w(px(4.0)).bg(rgb(theme::ACCENT)),
        )
        .child(
            div()
                .border_1()
                .border_color(rgb(theme::ACCENT))
                .child(right),
        )
}

#[cfg(test)]
mod tests {
    use super::{split_container, tab_bar};
    use gpui::{ParentElement, TestAppContext, div, point, px, size};

    #[gpui::test]
    async fn placeholders_lay_out_without_panicking(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        // `draw` runs the element through layout and paint, so the tab-bar and
        // split-container builders are actually exercised (not just constructed).
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_window, _cx| {
                div()
                    .child(tab_bar())
                    .child(split_container(div().child("left"), div().child("right")))
            },
        );
    }
}
