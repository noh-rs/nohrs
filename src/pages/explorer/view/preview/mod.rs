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

    let content = if let Some(editor) = &page.preview_editor {
        div().flex_1().child(editor.clone()).into_any_element()
    } else if let Some(image_path) = &page.preview_image_path {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x181818)) // Dark background for images
            .child(
                img(image_path.clone())
                    .h_full()
                    .w_full()
                    .object_fit(gpui::ObjectFit::Contain),
            )
            .into_any_element()
    } else if let Some(msg) = &page.preview_message {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme::MUTED))
            .child(msg.clone())
            .into_any_element()
    } else {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme::MUTED))
            .child("No file selected")
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
