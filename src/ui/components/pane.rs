#![cfg(feature = "gui")]

use crate::ui::theme::theme;
use gpui::{div, prelude::*, px, rgb, IntoElement};

/// Non-functional tab bar (placeholder)
pub fn tab_bar<'a>() -> impl IntoElement {
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
