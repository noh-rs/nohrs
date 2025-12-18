use super::row;
use crate::pages::explorer::types::SortKey;
use crate::pages::explorer::ExplorerPage;
use crate::ui::theme::theme;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{v_virtual_list, ListItem};
use std::rc::Rc;

pub fn render(page: &mut ExplorerPage, cx: &mut Context<ExplorerPage>) -> AnyElement {
    let table_width = page.total_table_width();
    let col_name = page.col_name_width;
    let col_type = page.col_type_width;
    let col_size = page.col_size_width;
    let col_modified = page.col_modified_width;
    let col_action = page.col_action_width;

    div()
        .size_full()
        .flex()
        .flex_col()
        .min_h(px(0.0))
        .overflow_hidden()
        .child(render_table_with_header(
            page,
            table_width,
            col_name,
            col_type,
            col_size,
            col_modified,
            col_action,
            cx,
        ))
        .into_any_element()
}

fn render_table_with_header(
    page: &mut ExplorerPage,
    table_width: f32,
    col_name: f32,
    col_type: f32,
    col_size: f32,
    col_modified: f32,
    col_action: f32,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    let entity = cx.entity().clone();

    let mut all_sizes = vec![gpui::size(px(table_width), px(48.0))];
    all_sizes.extend(page.item_sizes.as_ref().iter().copied());
    let all_sizes = Rc::new(all_sizes);
    let scroll_handle = page.virtual_scroll_handle.clone();

    div().flex_1().overflow_hidden().child(
        v_virtual_list(
            entity,
            "file-table",
            all_sizes,
            move |view, visible_range, _window, cx| {
                visible_range
                    .filter_map(|ix| {
                        if ix == 0 {
                            Some(
                                render_header_row(
                                    view,
                                    table_width,
                                    col_name,
                                    col_type,
                                    col_size,
                                    col_modified,
                                    col_action,
                                    cx,
                                )
                                .into_any_element(),
                            )
                        } else {
                            let data_ix = ix - 1;
                            view.filtered_entries.get(data_ix).cloned().map(|item| {
                                row::render(view, &item, data_ix, cx).into_any_element()
                            })
                        }
                    })
                    .collect()
            },
        )
        .track_scroll(&scroll_handle),
    )
}

fn render_header_row(
    page: &ExplorerPage,
    table_width: f32,
    col_name: f32,
    col_type: f32,
    col_size: f32,
    col_modified: f32,
    col_action: f32,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    div()
        .w(px(table_width))
        .h(px(48.0))
        .px(px(24.0))
        .bg(rgb(theme::BG))
        .border_b_1()
        .border_color(rgb(theme::BORDER))
        .child(
            div()
                .flex()
                .items_center()
                .w_full()
                .h_full()
                .child(render_resizable_column_header(
                    page,
                    "Name",
                    SortKey::Name,
                    0,
                    col_name,
                    cx,
                ))
                .child(render_resizable_column_header(
                    page,
                    "Type",
                    SortKey::Type,
                    1,
                    col_type,
                    cx,
                ))
                .child(render_resizable_column_header(
                    page,
                    "Size",
                    SortKey::Size,
                    2,
                    col_size,
                    cx,
                ))
                .child(render_resizable_column_header(
                    page,
                    "Modified",
                    SortKey::Modified,
                    3,
                    col_modified,
                    cx,
                ))
                .child(div().w(px(col_action)).flex_shrink_0()),
        )
}

fn render_resizable_column_header(
    page: &ExplorerPage,
    label: &str,
    key: SortKey,
    column_index: usize,
    width: f32,
    cx: &mut Context<ExplorerPage>,
) -> impl IntoElement {
    // We need to call page methods like start_column_resize which take &mut self.
    // But `v_virtual_list` closure gives `view` (&mut ExplorerPage), so `page` here could be &mut?
    // In `render_table_with_header` closure, `view` is passed to `render_header_row`.
    // So `page` is &mut ExplorerPage.
    // I should change signature to &mut ExplorerPage if needed.
    // But `start_column_resize` is called in event handler which takes `this` (mut).
    // The `page` argument is used to bind listener.
    // So `page` doesn't need to be mut here, the lambda captures things?
    // Wait, `cx.listener` callback receives `this: &mut V` (ExplorerPage).
    // So `page` is just used for rendering state (width etc).
    // `render_resizable_column_header` logic:
    // ... .child(render_column_header(...))
    // ... .child(div()... on_mouse_down(... this.start_column_resize ...))

    div()
        .w(px(width))
        .flex_shrink_0()
        .relative()
        .child(render_column_header(
            page,
            label,
            key,
            column_index,
            false,
            cx,
        ))
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .w(px(8.0))
                .h_full()
                .cursor_col_resize()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        this.start_column_resize(column_index, event.position);
                        cx.stop_propagation();
                    }),
                )
                .child(div().w(px(1.0)).h_full().ml(px(3.5)).bg(rgb(theme::BORDER))),
        )
}

fn render_column_header(
    page: &ExplorerPage,
    label: &str,
    key: SortKey,
    key_idx: usize,
    flex: bool,
    cx: &mut Context<ExplorerPage>,
) -> gpui::Div {
    let is_active = page.sort_key == key;
    let label_str = label.to_string();
    let sort_icon = if is_active {
        Some(if page.sort_asc { "↑" } else { "↓" })
    } else {
        None
    };

    let mut wrapper = div();

    if flex {
        wrapper = wrapper.flex_1();
    }

    wrapper.child(
        ListItem::new(("sort-header", key_idx))
            .on_click(cx.listener(move |this, _, _, _| {
                this.set_sort_key(key);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if is_active {
                                rgb(theme::FG)
                            } else {
                                rgb(theme::FG_SECONDARY)
                            })
                            .child(label_str),
                    )
                    .when(is_active, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::FG))
                                .child(sort_icon.unwrap_or("")),
                        )
                    }),
            ),
    )
}
