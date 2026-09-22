use super::super::types::ViewMode;
use crate::explorer::ExplorerPane;
use gpui::prelude::*;
use gpui::*;
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::list::ListItem;
use gpui_component::{Icon, IconName};
use nohrs_ui::theme::theme;

/// Renders the explorer header with navigation buttons, the breadcrumb path
/// bar, and view-mode controls.
pub fn render(
    page: &mut ExplorerPane,
    _window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    let parts = path_parts(&page.cwd);

    let (display_parts, is_truncated) = if parts.len() > 5 {
        (parts[(parts.len() - 5)..].to_vec(), true)
    } else {
        (parts.clone(), false)
    };

    let mut bc = Breadcrumb::new();

    if is_truncated {
        bc = bc.child(BreadcrumbItem::new("…").on_click(cx.listener(move |_this, _, _, _| {})));
    }

    let start_idx = if is_truncated { parts.len() - 5 } else { 0 };

    for (display_i, p) in display_parts.iter().enumerate() {
        let actual_i = start_idx + display_i;
        let text = if p.is_empty() {
            String::from("/")
        } else {
            p.clone()
        };

        // `PathBuf::push` knows the root component already ends in a separator;
        // joining the strings by hand produced "//tmp" for every crumb under "/".
        let mut prefix = std::path::PathBuf::new();
        for (j, part) in parts.iter().enumerate() {
            prefix.push(part);
            if j >= actual_i {
                break;
            }
        }
        let mut path_here = prefix.to_string_lossy().to_string();
        if path_here.is_empty() {
            path_here = page.cwd.clone();
        }

        bc = bc.child(BreadcrumbItem::new(text).on_click(
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
        .px(px(16.0))
        .py(px(10.0))
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .flex_shrink_0()
                .child(
                    ListItem::new("nav-back")
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .when(!can_go_back, |this| this.opacity(0.35))
                        .when(can_go_back, |this| {
                            this.on_click(
                                cx.listener(|view, _, window, cx| view.go_back(window, cx)),
                            )
                        })
                        .child(
                            Icon::new(IconName::ArrowLeft)
                                .size_4()
                                .text_color(rgb(theme::GRAY_600)),
                        ),
                )
                .child(
                    ListItem::new("nav-forward")
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .when(!can_go_forward, |this| this.opacity(0.35))
                        .when(can_go_forward, |this| {
                            this.on_click(
                                cx.listener(|view, _, window, cx| view.go_forward(window, cx)),
                            )
                        })
                        .child(
                            Icon::new(IconName::ArrowRight)
                                .size_4()
                                .text_color(rgb(theme::GRAY_600)),
                        ),
                )
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(20.0))
                        .bg(rgb(theme::BORDER))
                        .mx(px(6.0)),
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
                        .mr(px(4.0))
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
    page: &mut ExplorerPane,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    div()
        .flex()
        .items_center()
        .gap_1()
        .p(px(2.0))
        .rounded(px(8.0))
        .bg(rgb(theme::BG_SECONDARY))
        .border_1()
        .border_color(rgb(theme::BORDER))
        // `list.svg` ships with this crate but has no `IconName` variant, so it
        // is loaded by asset path.
        .child(view_mode_button(
            page,
            ViewMode::List,
            "view-mode-list",
            Icon::new(Icon::empty()).path(SharedString::from("icons/list.svg")),
            "List",
            cx,
        ))
        .child(view_mode_button(
            page,
            ViewMode::Grid,
            "view-mode-grid",
            Icon::new(IconName::LayoutDashboard),
            "Grid",
            cx,
        ))
}

fn view_mode_button(
    page: &mut ExplorerPane,
    mode: ViewMode,
    id: &'static str,
    icon: Icon,
    label: &'static str,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    let is_active = page.view_mode == mode;
    // The active segment lifts onto white so its accent icon reads against the
    // group's tinted track, rather than accent-on-gray.
    ListItem::new(id)
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .when(is_active, |this| this.bg(rgb(theme::BG)).shadow_sm())
        .on_click(cx.listener(move |this, _, _, cx| this.set_view_mode(mode, cx)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(icon.size_4().text_color(if is_active {
                    rgb(theme::ACCENT)
                } else {
                    rgb(theme::GRAY_500)
                }))
                .child(
                    div()
                        .text_xs()
                        .when(is_active, |this| this.font_weight(gpui::FontWeight::MEDIUM))
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
