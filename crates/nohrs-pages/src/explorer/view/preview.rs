use crate::explorer::ExplorerPane;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{Icon, IconName};
use nohrs_ui::theme::theme;

// Calculate the maximum line width in characters for horizontal scroll sizing
/// Renders the preview pane for the selected file, showing a text editor,
/// image, status message, or an empty placeholder.
pub fn render(page: &mut ExplorerPane, _window: &mut Window) -> impl IntoElement + use<> {
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
            .bg(rgb(theme::PREVIEW_BACKDROP))
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
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Icon::new(IconName::File)
                    .size_8()
                    .text_color(rgb(theme::GRAY_300)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme::MUTED))
                    .child("No file selected"),
            )
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .child(
            // Header. Matches the listing's column-header height so the two
            // line up across the divider between the panes.
            div()
                .h(px(super::listing::list::HEADER_HEIGHT))
                .flex()
                .items_center()
                .flex_shrink_0()
                .px(px(16.0))
                .border_b_1()
                .border_color(rgb(theme::BORDER))
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme::FG))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
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

/// The text editor used to render file previews.
pub mod editor;
