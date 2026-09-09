use gpui::{AnyElement, Context, Render, Window, div, prelude::*, px, rgb};
use nohrs_ui::theme::theme;

/// The extensions page, a placeholder for a future extension store.
pub struct ExtensionsPage;

impl Default for ExtensionsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionsPage {
    /// Creates a new extensions page.
    pub fn new() -> Self {
        Self
    }
}

impl Render for ExtensionsPage {
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
                    .child("🧩 Extensions"),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .text_base()
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child("Extension store to be implemented"),
            )
    }
}

impl crate::Page for ExtensionsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}
