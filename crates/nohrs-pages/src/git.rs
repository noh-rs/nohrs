use gpui::{AnyElement, Context, Render, Window, div, prelude::*, px, rgb};
use nohrs_ui::theme::theme;

/// The git page, a placeholder for future version-control functionality.
pub struct GitPage;

impl Default for GitPage {
    fn default() -> Self {
        Self::new()
    }
}

impl GitPage {
    /// Creates a new git page.
    pub fn new() -> Self {
        Self
    }
}

impl Render for GitPage {
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
                    .child("📦 Git"),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .text_base()
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child("Git integration feature to be implemented"),
            )
    }
}

impl crate::Page for GitPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}
