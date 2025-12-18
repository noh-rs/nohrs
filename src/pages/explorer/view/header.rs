use super::super::types::ViewMode;
use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::{Icon, IconName, ListItem};

pub fn render(
    page: &mut ExplorerPage,
    _window: &mut Window,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    let parts = path_parts(&page.cwd);

    let (display_parts, is_truncated) = if parts.len() > 5 {
        (parts[(parts.len() - 5)..].to_vec(), true)
    } else {
        (parts.clone(), false)
    };

    let mut bc = Breadcrumb::new();

    if is_truncated {
        bc = bc.item(
            BreadcrumbItem::new("ellipsis", "…").on_click(cx.listener(move |_this, _, _, _| {})),
        );
    }

    let start_idx = if is_truncated { parts.len() - 5 } else { 0 };

    for (display_i, p) in display_parts.iter().enumerate() {
        let actual_i = start_idx + display_i;
        let text = if p.is_empty() {
            String::from("/")
        } else {
            p.clone()
        };

        let mut path_here = String::new();
        for (j, part) in parts.iter().enumerate() {
            if j == 0 {
                path_here = if part.is_empty() {
                    "/".to_string()
                } else {
                    part.clone()
                };
            } else {
                path_here.push(std::path::MAIN_SEPARATOR);
                path_here.push_str(part);
            }
            if j >= actual_i {
                break;
            }
        }
        if path_here.is_empty() {
            path_here = page.cwd.clone();
        }

        bc = bc.item(BreadcrumbItem::new(("bc", actual_i), text).on_click(
            cx.listener(move |this, _, window, cx| this.change_dir(path_here.clone(), window, cx)),
        ));
    }

    let can_go_back = page.history_index > 0;
    let can_go_forward = page.history_index + 1 < page.history.len();

    // Store search_visible for use in search toggle style
    let search_visible = page.search_visible;
    let entry_count = page.filtered_entries.len();

    div()
        .bg(rgb(theme::BG))
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .flex()
        .items_center()
        .text_color(rgb(theme::FG))
        .px(px(24.0))
        .py(px(12.0))
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .child(
                    ListItem::new("nav-back")
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .when(!can_go_back, |this| this.opacity(0.3))
                        .when(can_go_back, |this| {
                            this.on_click(
                                cx.listener(|view, _, window, cx| view.go_back(window, cx)),
                            )
                        })
                        .child(div().text_sm().text_color(rgb(theme::GRAY_600)).child("←")),
                )
                .child(
                    ListItem::new("nav-forward")
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .when(!can_go_forward, |this| this.opacity(0.3))
                        .when(can_go_forward, |this| {
                            this.on_click(
                                cx.listener(|view, _, window, cx| view.go_forward(window, cx)),
                            )
                        })
                        .child(div().text_sm().text_color(rgb(theme::GRAY_600)).child("→")),
                )
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(20.0))
                        .bg(rgb(theme::BORDER))
                        .mx(px(4.0)),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .min_w(px(0.0))
                .child(div().flex().items_center().child(bc)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme::FG_SECONDARY))
                        .whitespace_nowrap()
                        .child(format!("{} items", entry_count)),
                )
                .child(render_view_mode_toggle(page, cx))
                .child(
                    ListItem::new("search-toggle")
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.toggle_search(window, cx);
                        }))
                        .child(Icon::new(IconName::Search).size_4().text_color(
                            if search_visible {
                                rgb(theme::ACCENT)
                            } else {
                                rgb(theme::GRAY_600)
                            },
                        )),
                ),
        )
}

fn render_view_mode_toggle(
    page: &mut ExplorerPage,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(view_mode_button(
            page,
            ViewMode::List,
            "view-mode-list",
            IconName::PanelBottomOpen,
            "List",
            cx,
        ))
        .child(view_mode_button(
            page,
            ViewMode::Grid,
            "view-mode-grid",
            IconName::LayoutDashboard,
            "Grid",
            cx,
        ))
}

fn view_mode_button(
    page: &mut ExplorerPage,
    mode: ViewMode,
    id: &'static str,
    icon: IconName,
    label: &'static str,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    let is_active = page.view_mode == mode;
    ListItem::new(id)
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .when(is_active, |this| this.bg(rgb(theme::BG_HOVER)))
        .on_click(cx.listener(move |this, _, _, cx| this.set_view_mode(mode, cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(Icon::new(icon).size_4().text_color(if is_active {
                    rgb(theme::ACCENT)
                } else {
                    rgb(theme::GRAY_600)
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_active {
                            rgb(theme::FG)
                        } else {
                            rgb(theme::FG_SECONDARY)
                        })
                        .child(label),
                ),
        )
}

fn path_parts(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for c in std::path::Path::new(path).components() {
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    if parts.is_empty() {
        parts.push(path.to_string());
    }
    parts
}
