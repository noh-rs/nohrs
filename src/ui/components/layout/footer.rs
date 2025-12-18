#![cfg(feature = "gui")]

use crate::ui::theme::theme;
use gpui::{div, prelude::*, px, rgb, Context, IntoElement};
use gpui_component::{Icon, IconName};

#[derive(Clone)]
pub struct FooterProps {
    pub selected_count: usize,
    pub total_count: usize,
    pub total_size: String,
    pub current_path: String,
    pub git_branch: Option<String>,
    pub storage_status: Option<String>,
    pub indexing_progress: Option<f32>,
}

impl Default for FooterProps {
    fn default() -> Self {
        Self {
            selected_count: 0,
            total_count: 0,
            total_size: String::from("0 B"),
            current_path: String::from("/"),
            git_branch: None,
            storage_status: None,
            indexing_progress: None,
        }
    }
}

/// A VSCode-like footer (status bar)
pub fn footer<V: gpui::Render>(props: FooterProps, cx: &mut Context<V>) -> impl IntoElement {
    div()
        .h(px(28.0))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .bg(rgb(theme::GRAY_200))
        .border_t_1()
        .border_color(rgb(theme::BORDER))
        .child(
            // Left section - Status items
            div()
                .flex()
                .items_center()
                .gap_2()
                // Git branch
                .when_some(props.git_branch.clone(), |this, branch| {
                    this.child(footer_button(
                        ("footer-git", 0_usize),
                        IconName::File,
                        &branch,
                        cx,
                    ))
                })
                // Indexing Progress
                .when_some(props.indexing_progress, |this, progress| {
                    if progress < 1.0 {
                        let percent = (progress * 100.0) as u32;
                        this.child(footer_button(
                            ("footer-indexing", 99_usize),
                            IconName::File, // Use a spinner icon if available? IconName::Sync?
                            &format!("Indexing: {}%", percent),
                            cx,
                        ))
                    } else {
                        this
                    }
                })
                // Selected items
                .when(props.selected_count > 0, |this| {
                    this.child(footer_button(
                        ("footer-selected", 1_usize),
                        IconName::File,
                        &format!("{} selected", props.selected_count),
                        cx,
                    ))
                })
                // Total items
                .child(footer_button(
                    ("footer-total", 2_usize),
                    IconName::Folder,
                    &format!("{} items", props.total_count),
                    cx,
                ))
                // Total size
                .child(footer_button(
                    ("footer-size", 3_usize),
                    IconName::File,
                    &props.total_size,
                    cx,
                )),
        )
        .child(
            // Right section - Info items
            div()
                .flex()
                .items_center()
                .gap_2()
                // Storage status (S3 connection, etc)
                .when_some(props.storage_status, |this, status| {
                    this.child(footer_button(
                        ("footer-storage", 4_usize),
                        IconName::Folder,
                        &status,
                        cx,
                    ))
                })
                // Current path indicator
                .child(footer_button(
                    ("footer-path", 5_usize),
                    IconName::Folder,
                    &truncate_path(&props.current_path, 30),
                    cx,
                )),
        )
}

fn footer_button<V: gpui::Render>(
    id: impl Into<gpui::ElementId>,
    icon: IconName,
    label: &str,
    _cx: &mut Context<V>,
) -> impl IntoElement {
    let label = label.to_string();
    let has_label = !label.is_empty();

    div()
        .id(id)
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme::GRAY_300)))
        .child(Icon::new(icon).size_3().text_color(rgb(theme::GRAY_700)))
        .when(has_label, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::GRAY_700))
                    .child(label),
            )
        })
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return format!("...{}", &path[path.len().saturating_sub(max_len)..]);
    }

    // Show first and last parts
    format!("{}/.../{}", parts[0], parts[parts.len() - 1])
}
