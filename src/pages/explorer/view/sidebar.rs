use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme; // Assuming theme is accessible
use gpui::prelude::*;
use gpui::*;
use gpui_component::{Icon, IconName, ListItem};

pub fn render(
    page: &mut ExplorerPage,
    _window: &mut Window,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme::BG))
        .py(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px(px(8.0))
                .child(sidebar_item(IconName::Folder, "Home", true))
                .child(sidebar_item(IconName::Star, "Favorites", false))
                .child(sidebar_item(IconName::File, "Recent", false))
                .child(sidebar_item(IconName::Folder, "Trash", false)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .mt(px(16.0))
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme::FG_SECONDARY))
                        .child("Folder"),
                )
                .child(render_shortcuts(page, cx)),
        )
}

fn sidebar_item(icon: IconName, label: &str, _active: bool) -> impl IntoElement {
    let label = label.to_string();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(|this| this.bg(rgb(theme::BG_HOVER)))
        .child(Icon::new(icon).size_4().text_color(rgb(theme::GRAY_600)))
        .child(div().text_sm().text_color(rgb(theme::FG)).child(label))
}

fn render_shortcuts(_page: &mut ExplorerPage, cx: &mut Context<ExplorerPage>) -> impl IntoElement {
    let shortcuts = get_shortcuts();
    let mut shortcuts_el = div().flex().flex_col().gap_1().px(px(8.0));

    for (i, (label, path)) in shortcuts.into_iter().enumerate() {
        let p = path.clone();
        let icon = match label.as_str() {
            "Home" => IconName::Folder,
            "Desktop" => IconName::Folder,
            "Downloads" => IconName::Folder,
            "Documents" => IconName::Folder,
            "Pictures" => IconName::Folder,
            _ => IconName::Folder,
        };
        let label_str = label.clone();

        shortcuts_el = shortcuts_el.child(
            ListItem::new(("shortcut", i))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.change_dir(p.clone(), window, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(icon).size_4().text_color(rgb(theme::GRAY_600)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme::FG))
                                .child(label_str.clone()),
                        ),
                ),
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
