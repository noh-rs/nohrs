use super::truncate_middle;
use crate::pages::explorer::ExplorerPage;
use crate::services::fs::listing::FileEntryDto;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{Icon, IconName};

pub fn render(
    page: &mut ExplorerPage,
    window: &mut Window,
    cx: &mut Context<ExplorerPage>,
) -> AnyElement {
    let items = page.filtered_entries.clone();
    let mut grid = div()
        .flex()
        .flex_wrap()
        .gap_4()
        .items_start()
        .min_h(px(0.0));

    for (ix, item) in items.into_iter().enumerate() {
        let selected = page.selected_index == Some(ix);
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
    _page: &mut ExplorerPage,
    item: FileEntryDto,
    ix: usize,
    selected: bool,
    _window: &mut Window,
    cx: &mut Context<ExplorerPage>,
) -> AnyElement {
    use crate::ui::components::file_list::{format_date, get_file_type, human_bytes};

    let icon_name = match item.kind.as_str() {
        "dir" => IconName::Folder,
        _ => IconName::File,
    };

    let name = truncate_middle(&item.name, 28);
    let file_type = get_file_type(&item.name, &item.kind);
    let size_text = match item.kind.as_str() {
        "file" => human_bytes(item.size),
        "dir" => file_type.clone(),
        _ => file_type.clone(),
    };
    let modified_text = format_date(&item.modified);
    let activation_item = item.clone();
    let preview_item = item.clone();

    let bg_color = if selected {
        rgb(theme::BG_HOVER)
    } else {
        rgb(theme::BG)
    };

    let border_color = if selected {
        rgb(theme::ACCENT)
    } else {
        rgb(theme::BORDER)
    };

    div()
        .w(px(180.0))
        .min_h(px(140.0))
        .p(px(16.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(border_color)
        .bg(bg_color)
        .hover(|this| this.bg(rgb(theme::BG_HOVER)))
        .cursor_pointer()
        .flex()
        .flex_col()
        .items_start()
        .gap_3()
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                this.record_click(ix, event.click_count);
                this.selected_index = Some(ix);
                if preview_item.kind == "file" {
                    this.open_preview(preview_item.path.clone());
                }
                if event.click_count >= 2 {
                    this.activate_entry(activation_item.clone(), window, cx);
                }
            }),
        )
        .child(
            Icon::new(icon_name)
                .size_6()
                .text_color(rgb(theme::GRAY_600)),
        )
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme::FG))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(name),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme::FG_SECONDARY))
                .child(file_type),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme::FG_SECONDARY))
                .child(size_text),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme::MUTED))
                .child(modified_text),
        )
        .into_any_element()
}
