use crate::theme::theme;
use gpui::{Context, IntoElement, div, prelude::*, px, rgb};
use gpui_component::{Icon, IconName};

/// Properties controlling the contents of the footer status bar.
#[derive(Clone)]
pub struct FooterProps {
    /// Number of currently selected items; shows a "N selected" indicator when greater than zero.
    pub selected_count: usize,
    /// Total number of items in the current view.
    pub total_count: usize,
    /// Pre-formatted total size of the items.
    pub total_size: String,
    /// Current path, displayed (truncated) on the right side of the footer.
    pub current_path: String,
    /// Active Git branch name, shown when present.
    pub git_branch: Option<String>,
    /// Storage backend status (e.g. S3 connection), shown when present.
    pub storage_status: Option<String>,
    /// Indexing progress in the range 0.0..=1.0; the indicator is hidden once it reaches 1.0.
    pub indexing_progress: Option<f32>,
    /// Transient message (e.g. an error) surfaced to the user. When
    /// `status_is_error` is set it is rendered in the error color.
    pub status_message: Option<String>,
    /// Whether `status_message` should be rendered using the error color.
    pub status_is_error: bool,
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
            status_message: None,
            status_is_error: false,
        }
    }
}

/// A VSCode-like footer (status bar)
pub fn footer<V: gpui::Render>(
    props: FooterProps,
    cx: &mut Context<V>,
) -> impl IntoElement + use<V> {
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
                ))
                // Transient status / error message
                .when_some(props.status_message.clone(), |this, message| {
                    let color = if props.status_is_error {
                        theme::DANGER
                    } else {
                        theme::GRAY_700
                    };
                    this.child(
                        div()
                            .id(("footer-status", 6_usize))
                            .h(px(24.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Icon::new(IconName::Info).size_3().text_color(rgb(color)))
                            .child(
                                // Keep the status on one line so a long or
                                // multi-line message can't overflow the footer.
                                div()
                                    .text_xs()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_color(rgb(color))
                                    .child(message),
                            ),
                    )
                }),
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

#[cfg(test)]
mod tests {
    use super::{FooterProps, footer, truncate_path};
    use gpui::{IntoElement, Render, TestAppContext, Window};

    #[test]
    fn short_paths_are_returned_unchanged() {
        assert_eq!(truncate_path("/a/b", 10), "/a/b");
    }

    #[test]
    fn long_multi_segment_paths_elide_the_middle() {
        let path = "/usr/local/share/nohrs/config.toml";
        assert_eq!(truncate_path(path, 10), "/.../config.toml");
    }

    #[test]
    fn long_single_segment_paths_keep_the_tail() {
        let truncated = truncate_path("averylongsinglefilename.txt", 8);
        assert!(truncated.starts_with("..."));
        assert!(truncated.ends_with("ame.txt"));
    }

    // Host view so `footer` (which needs `&mut Context<V: Render>`) can be built
    // inside a test window. `render` records that it ran; building the element
    // tree eagerly evaluates every `when`/`when_some` branch, so a single draw
    // exercises whichever footer sections the props enable.
    struct FooterHost {
        props: FooterProps,
        renders: usize,
    }

    impl Render for FooterHost {
        fn render(
            &mut self,
            _window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            self.renders += 1;
            footer(self.props.clone(), cx)
        }
    }

    #[gpui::test]
    async fn footer_renders_every_status_section(cx: &mut TestAppContext) {
        // The footer paints `Icon`s, which read the gpui-component `Theme` global.
        cx.update(gpui_component::init);
        let props = FooterProps {
            selected_count: 2,
            total_count: 5,
            total_size: "1.0 KB".into(),
            current_path: "/usr/local/share/nohrs/config.toml".into(),
            git_branch: Some("main".into()),
            storage_status: Some("S3: connected".into()),
            indexing_progress: Some(0.5),
            status_message: Some("scan failed".into()),
            status_is_error: true,
        };
        let (host, cx) = cx.add_window_view(|_window, _cx| FooterHost { props, renders: 0 });
        cx.run_until_parked();
        host.read_with(cx, |host, _cx| assert!(host.renders > 0));
    }

    #[gpui::test]
    async fn footer_renders_with_status_sections_absent(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        // The complementary branches: no git/storage, completed indexing
        // (>= 1.0 hides the indicator), no selection, non-error status.
        let props = FooterProps {
            total_count: 0,
            indexing_progress: Some(1.0),
            status_message: Some("ready".into()),
            status_is_error: false,
            ..FooterProps::default()
        };
        let (host, cx) = cx.add_window_view(|_window, _cx| FooterHost { props, renders: 0 });
        cx.run_until_parked();
        host.read_with(cx, |host, _cx| assert!(host.renders > 0));
    }
}
