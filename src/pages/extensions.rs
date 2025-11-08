use gpui::{prelude::*, div, px, rgb, AnyElement, Context, Render, Window};
use crate::ui::theme::theme;

pub struct ExtensionsPage;

impl ExtensionsPage {
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
                    .child("🧩 拡張機能")
            )
            .child(
                div()
                    .mt(px(16.0))
                    .text_base()
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child("拡張機能ストアを実装予定")
            )
    }
}

impl crate::pages::Page for ExtensionsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}

