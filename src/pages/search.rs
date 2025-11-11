use crate::ui::theme::theme;
use gpui::{div, prelude::*, px, rgb, AnyElement, Context, Render, Window};

pub struct SearchPage;

impl SearchPage {
    pub fn new() -> Self {
        Self
    }
}

impl Render for SearchPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(rgb(theme::BG))
            .child(
                div()
                    .text_2xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(theme::FG))
                    .child("🔍 Search"),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .text_base()
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child("Full-text search feature to be implemented"),
            )
    }
}

impl crate::pages::Page for SearchPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}
