use crate::explorer::ExplorerPane;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{Icon, IconName};
use nohrs_ui::theme::theme;

/// Renders the explorer sidebar listing quick-access locations.
pub fn render(
    page: &mut ExplorerPane,
    _window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .py(px(12.0))
        .child(section_label("Library"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .px(px(8.0))
                .child(sidebar_item(Icon::new(IconName::Star), "Favorites", false))
                .child(sidebar_item(asset_icon("icons/clock.svg"), "Recent", false))
                .child(sidebar_item(
                    asset_icon("icons/trash-2.svg"),
                    "Trash",
                    false,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .mt(px(12.0))
                .child(section_label("Places"))
                .child(render_shortcuts(page, cx)),
        )
}

fn section_label(label: &'static str) -> impl IntoElement + use<> {
    div()
        .px(px(16.0))
        .pt(px(4.0))
        .pb(px(6.0))
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(theme::MUTED))
        .child(label)
}

/// An icon loaded straight from the embedded asset path, for the glyphs that
/// ship with this crate but have no `IconName` variant.
fn asset_icon(path: &'static str) -> Icon {
    Icon::new(Icon::empty()).path(SharedString::from(path))
}

/// One sidebar row. Every group shares this so the two lists keep the same row
/// height and icon/label alignment.
fn sidebar_item(icon: Icon, label: &str, active: bool) -> impl IntoElement + use<> {
    let label = label.to_string();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .h(px(28.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .when(active, |this| this.bg(rgb(theme::ACCENT_SUBTLE)))
        .when(!active, |this| {
            this.hover(|this| this.bg(rgb(theme::BG_HOVER)))
        })
        .child(icon.size_4().flex_shrink_0().text_color(rgb(if active {
            theme::ACCENT
        } else {
            theme::GRAY_500
        })))
        .child(
            div()
                .text_sm()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(active, |this| this.font_weight(gpui::FontWeight::MEDIUM))
                .text_color(rgb(theme::FG))
                .child(label),
        )
}

fn render_shortcuts(
    page: &mut ExplorerPane,
    cx: &mut Context<ExplorerPane>,
) -> impl IntoElement + use<> {
    let shortcuts = get_shortcuts();
    let cwd = page.cwd.clone();
    let mut shortcuts_el = div().flex().flex_col().gap(px(1.0)).px(px(8.0));

    for (i, (label, path)) in shortcuts.into_iter().enumerate() {
        let p = path.clone();
        let is_current = cwd == path;
        let icon = if is_current {
            Icon::new(IconName::FolderOpen)
        } else {
            Icon::new(IconName::Folder)
        };

        shortcuts_el = shortcuts_el.child(
            div()
                .id(("shortcut", i))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.change_dir(p.clone(), window, cx)),
                )
                .child(sidebar_item(icon, &label, is_current)),
        );
    }

    shortcuts_el
}

fn get_shortcuts() -> Vec<(String, String)> {
    let mut v = Vec::new();
    let home = std::env::var("HOME").ok();
    #[cfg(target_os = "windows")]
    let home = home.or_else(|| std::env::var("USERPROFILE").ok());
    if let Some(h) = home {
        let p = |s: &str| {
            std::path::Path::new(&h)
                .join(s)
                .to_string_lossy()
                .to_string()
        };
        v.push(("Home".into(), h.clone()));
        for (label, sub) in [
            ("Desktop", "Desktop"),
            ("Downloads", "Downloads"),
            ("Documents", "Documents"),
            ("Pictures", "Pictures"),
        ] {
            let path = p(sub);
            if std::path::Path::new(&path).exists() {
                v.push((label.into(), path));
            }
        }
    }
    v
}
