use super::truncate_middle;
use crate::explorer::ExplorerPane;
use gpui::prelude::*;
use gpui::*;
use gpui_component::input::Input;
use gpui_component::{Icon, IconName};
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_ui::theme::theme;

/// Renders the file listing as a grid of icon tiles.
pub fn render(
    page: &mut ExplorerPane,
    window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> AnyElement {
    let items = page.filtered_entries.clone();
    let mut grid = div()
        .flex()
        .flex_wrap()
        .gap_4()
        .items_start()
        .min_h(px(0.0));

    for (ix, item) in items.into_iter().enumerate() {
        let selected = page.is_selected(ix);
        grid = grid.child(render_grid_item(page, item, ix, selected, window, cx));
    }

    div()
        .id("grid-scroll")
        .flex_1()
        .overflow_scroll()
        .px(px(24.0))
        .py(px(16.0))
        .child(grid)
        .into_any_element()
}

fn render_grid_item(
    page: &mut ExplorerPane,
    item: FileEntryDto,
    ix: usize,
    selected: bool,
    _window: &mut Window,
    cx: &mut Context<ExplorerPane>,
) -> AnyElement {
    use nohrs_ui::components::file_list::{format_date, human_bytes};

    let rename_input = page
        .renaming
        .as_ref()
        .filter(|state| state.index == ix)
        .map(|state| state.input.clone());

    let icon_name = match item.kind.as_str() {
        "dir" => IconName::Folder,
        _ => IconName::File,
    };

    let is_dir = item.kind == "dir";
    // Budgeted to the tile's inner width: the label is centered, so anything
    // wider is clipped at *both* ends rather than ellipsized.
    let name = truncate_middle(&item.name, 16);
    // A folder has no meaningful byte size, so it shows its date alone rather
    // than repeating "Folder" on a second line.
    let meta_text = match item.kind.as_str() {
        "dir" => format_date(&item.modified),
        "file" => format!(
            "{} · {}",
            human_bytes(item.size),
            format_date(&item.modified)
        ),
        other => format!("{} · {}", other, format_date(&item.modified)),
    };
    let activation_item = item.clone();
    let preview_item = item.clone();

    let bg_color = if selected {
        rgb(theme::ACCENT_SUBTLE)
    } else {
        rgb(theme::BG)
    };

    let border_color = if selected {
        rgb(theme::ACCENT)
    } else {
        rgb(theme::BORDER)
    };

    div()
        .w(px(168.0))
        .h(px(136.0))
        .px(px(12.0))
        .py(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(border_color)
        .bg(bg_color)
        .when(!selected, |this| {
            this.hover(|this| this.bg(rgb(theme::BG_SECONDARY)))
        })
        .cursor_pointer()
        .flex()
        .flex_col()
        .items_center()
        .text_center()
        .gap_2()
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.record_click(ix, event.click_count);
                let modifiers = event.modifiers;
                if modifiers.shift {
                    this.select_range_to(ix);
                } else if modifiers.platform || modifiers.control {
                    this.toggle_select(ix);
                } else {
                    this.select_single(ix);
                }
                if preview_item.kind == "file" {
                    this.open_preview(preview_item.path.clone(), window, cx);
                }
                if event.click_count >= 2 {
                    this.activate_entry(activation_item.clone(), window, cx);
                }
                cx.notify();
            }),
        )
        .child(div().flex().flex_1().items_center().justify_center().child(
            Icon::new(icon_name).size_10().text_color(rgb(if is_dir {
                theme::ACCENT
            } else {
                theme::GRAY_500
            })),
        ))
        .child(match rename_input {
            Some(input) => div().w_full().child(Input::new(&input)).into_any_element(),
            None => div()
                .w_full()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme::FG))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(name)
                .into_any_element(),
        })
        .child(
            div()
                .w_full()
                .text_xs()
                .text_color(rgb(theme::MUTED))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(meta_text),
        )
        .into_any_element()
}
