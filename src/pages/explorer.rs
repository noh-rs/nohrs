use crate::services::fs::listing::{list_dir_sync, FileEntryDto, ListParams};
use crate::ui::components::file_list::FileListDelegate;
use crate::ui::theme::theme;

use gpui::{
    div, prelude::*, px, rgb, size, AnyElement, Context, Entity, FocusHandle, Focusable,
    IntoElement, Render, Window,
};
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::input::{InputState, TextInput};
use gpui_component::list::{List, ListEvent};
use gpui_component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_component::{v_virtual_list, Icon, IconName, VirtualListScrollHandle};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SortKey {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    List,
    Grid,
}

pub struct ExplorerPage {
    cwd: String,
    history: Vec<String>,
    history_index: usize,
    entries: Vec<FileEntryDto>,
    filtered_entries: Vec<FileEntryDto>,
    sort_key: SortKey,
    sort_asc: bool,
    search_query: String,
    search_visible: bool,
    search_input: Entity<InputState>,
    resizable: Entity<ResizableState>,
    list: Option<Entity<List<FileListDelegate>>>,
    subs: Vec<gpui::Subscription>,
    preview_path: Option<String>,
    preview_text: Option<String>,
    selected_index: Option<usize>,
    virtual_scroll_handle: VirtualListScrollHandle,
    item_sizes: Rc<Vec<gpui::Size<gpui::Pixels>>>,
    // Column widths (resizable)
    col_name_width: f32,
    col_type_width: f32,
    col_size_width: f32,
    col_modified_width: f32,
    col_action_width: f32,
    // Resize state
    resizing_column: Option<ResizingColumn>,
    focus_handle: FocusHandle,
    focus_requested: bool,
    last_click_info: Option<LastClickInfo>,
    view_mode: ViewMode,
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone, Copy)]
struct ResizingColumn {
    column_index: usize,
    start_width: f32,
    start_x: gpui::Point<gpui::Pixels>,
}

struct LastClickInfo {
    row: usize,
    timestamp: Instant,
    click_count: usize,
}

const CONFIRM_SUPPRESS_WINDOW: Duration = Duration::from_millis(300);

impl ExplorerPage {
    pub fn new(
        resizable: Entity<ResizableState>,
        search_input: Entity<InputState>,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".into()),
            history: Vec::new(),
            history_index: 0,
            entries: Vec::new(),
            filtered_entries: Vec::new(),
            sort_key: SortKey::Name,
            sort_asc: true,
            search_query: String::new(),
            search_visible: false,
            search_input,
            resizable,
            list: None,
            subs: Vec::new(),
            preview_path: None,
            preview_text: None,
            selected_index: None,
            virtual_scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            // Initial column widths
            col_name_width: 400.0,
            col_type_width: 120.0,
            col_size_width: 120.0,
            col_modified_width: 180.0,
            col_action_width: 60.0,
            resizing_column: None,
            focus_handle,
            focus_requested: false,
            last_click_info: None,
            view_mode: ViewMode::List,
        }
    }

    fn ensure_loaded(&mut self) {
        if self.entries.is_empty() {
            self.reload();
        }
    }

    fn reload(&mut self) {
        if let Ok(res) = list_dir_sync(ListParams {
            path: &self.cwd,
            limit: 1000,
            cursor: None,
        }) {
            let mut e = res.entries;
            self.sort_entries(&mut e);
            self.entries = e;
            self.apply_filter();
            self.update_item_sizes();
            self.preview_text = None;
            self.preview_path = None;
        }
    }

    fn update_item_sizes(&mut self) {
        let total_width = self.col_name_width
            + self.col_type_width
            + self.col_size_width
            + self.col_modified_width
            + self.col_action_width
            + 48.0;

        let sizes = self
            .filtered_entries
            .iter()
            .map(|_| size(px(total_width), px(32.0)))
            .collect();
        self.item_sizes = Rc::new(sizes);
    }

    fn total_table_width(&self) -> f32 {
        self.col_name_width
            + self.col_type_width
            + self.col_size_width
            + self.col_modified_width
            + self.col_action_width
            + 48.0
    }

    fn apply_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_entries = self.entries.clone();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_entries = self
                .entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
        self.update_item_sizes();
    }

    fn set_sort_key(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = true;
        }
        let mut e = self.entries.clone();
        self.sort_entries(&mut e);
        self.entries = e;
        self.apply_filter();
    }

    fn sort_entries(&self, entries: &mut [FileEntryDto]) {
        match self.sort_key {
            SortKey::Name => {
                entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }
            SortKey::Size => entries.sort_by(|a, b| a.size.cmp(&b.size)),
            SortKey::Modified => entries.sort_by(|a, b| a.modified.cmp(&b.modified)),
            SortKey::Type => entries.sort_by(|a, b| {
                let ext_a = get_extension(&a.name, &a.kind);
                let ext_b = get_extension(&b.name, &b.kind);
                ext_a.cmp(&ext_b)
            }),
        }
        if !self.sort_asc {
            entries.reverse();
        }
        entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
            ("dir", "file") => std::cmp::Ordering::Less,
            ("file", "dir") => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
    }

    fn change_dir(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if path == self.cwd {
            return;
        }
        self.close_search(window, cx);
        if self.history.is_empty() {
            self.history.push(self.cwd.clone());
            self.history_index = 0;
        }
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        self.history.push(path.clone());
        self.history_index += 1;
        self.cwd = path;
        self.entries.clear();
        self.reload();
    }

    fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            if let Some(p) = self.history.get(self.history_index).cloned() {
                self.cwd = p;
                self.entries.clear();
                self.close_search(window, cx);
                self.reload();
            }
        }
    }

    fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            if let Some(p) = self.history.get(self.history_index).cloned() {
                self.cwd = p;
                self.entries.clear();
                self.close_search(window, cx);
                self.reload();
            }
        }
    }

    fn start_column_resize(&mut self, column_index: usize, start_pos: gpui::Point<gpui::Pixels>) {
        let start_width = match column_index {
            0 => self.col_name_width,
            1 => self.col_type_width,
            2 => self.col_size_width,
            3 => self.col_modified_width,
            _ => return,
        };

        self.resizing_column = Some(ResizingColumn {
            column_index,
            start_width,
            start_x: start_pos,
        });
    }

    fn update_column_resize(&mut self, current_pos: gpui::Point<gpui::Pixels>) {
        if let Some(resize) = self.resizing_column {
            let delta: f32 = (current_pos.x - resize.start_x.x).into();
            let new_width = (resize.start_width + delta).max(80.0);

            match resize.column_index {
                0 => self.col_name_width = new_width,
                1 => self.col_type_width = new_width,
                2 => self.col_size_width = new_width,
                3 => self.col_modified_width = new_width,
                _ => {}
            }

            self.update_item_sizes();
        }
    }

    fn stop_column_resize(&mut self) {
        self.resizing_column = None;
    }

    fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        if self.view_mode != mode {
            self.view_mode = mode;
            cx.notify();
        }
    }

    fn record_click(&mut self, row: usize, click_count: usize) {
        self.last_click_info = Some(LastClickInfo {
            row,
            timestamp: Instant::now(),
            click_count,
        });
    }

    fn activate_entry(&mut self, item: FileEntryDto, window: &mut Window, cx: &mut Context<Self>) {
        if item.kind == "dir" {
            self.change_dir(item.path, window, cx);
        } else {
            self.open_preview(item.path);
        }
    }

    fn ensure_list_initialized(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.list.is_none() {
            let mut delegate = FileListDelegate::new();
            delegate.set_items(self.filtered_entries.clone());
            let list = cx.new(|cx| List::new(delegate, window, cx).no_query());
            let sub = cx.subscribe_in(
                &list,
                window,
                |this, _list, event: &ListEvent, window, cx| match event {
                    ListEvent::Select(ix) => {
                        this.selected_index = Some(ix.row);
                        if let Some(item) = this.filtered_entries.get(ix.row).cloned() {
                            if item.kind == "file" {
                                this.open_preview(item.path);
                            }
                        }
                    }
                    ListEvent::Confirm(ix) => {
                        if let Some(info) = this.last_click_info.as_ref() {
                            if info.row == ix.row
                                && info.timestamp.elapsed() < CONFIRM_SUPPRESS_WINDOW
                                && info.click_count >= 2
                            {
                                this.last_click_info = None;
                                return;
                            }
                        }
                        this.last_click_info = None;
                        this.selected_index = Some(ix.row);
                        if let Some(item) = this.filtered_entries.get(ix.row).cloned() {
                            this.activate_entry(item, window, cx);
                        }
                    }
                    ListEvent::Cancel => {}
                },
            );
            self.subs.push(sub);
            self.list = Some(list);
        } else if let Some(list) = &self.list {
            let items = self.filtered_entries.clone();
            list.update(cx, |l, _cx| {
                l.delegate_mut().set_items(items);
            });
        }
    }

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_visible {
            return;
        }
        self.search_visible = true;
        self.search_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_visible {
            return;
        }
        self.search_visible = false;
        self.search_query.clear();
        self.apply_filter();
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_visible {
            self.close_search(window, cx);
        } else {
            self.open_search(window, cx);
        }
    }

    fn open_preview(&mut self, path: String) {
        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_file() && md.len() <= 1024 * 1024 * 2 {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        self.preview_path = Some(path);
                        self.preview_text = Some(text);
                        return;
                    }
                }
            }
        }
        self.preview_path = Some(path);
        self.preview_text = Some("(Preview not available for this file)".into());
    }

    fn shortcuts(&self) -> Vec<(String, String)> {
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
}

impl Render for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_loaded();
        if !self.focus_requested {
            self.focus_requested = true;
            cx.focus_self(window);
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .relative()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key_lc = event.keystroke.key.to_lowercase();
                let is_f = key_lc == "f" || event.keystroke.key == "KeyF";
                if is_f && (event.keystroke.modifiers.platform || event.keystroke.modifiers.control)
                {
                    this.toggle_search(window, cx);
                    cx.stop_propagation();
                } else if key_lc == "escape" && this.search_visible {
                    this.toggle_search(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                    if this.resizing_column.is_some() {
                        this.update_column_resize(event.position);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    if this.resizing_column.is_some() {
                        this.stop_column_resize();
                        cx.notify();
                    }
                }),
            )
            .child(self.render_header(window, cx))
            .child(
                div().flex().flex_row().flex_grow().min_h(px(0.0)).child(
                    h_resizable("file-explorer", self.resizable.clone())
                        .child(
                            resizable_panel()
                                .size(px(180.0))
                                .size_range(px(180.0)..px(360.0))
                                .child(
                                    div()
                                        .size_full()
                                        .overflow_hidden()
                                        .border_r_1()
                                        .border_color(rgb(theme::BORDER))
                                        .child(self.render_sidebar(window, cx)),
                                ),
                        )
                        .child(
                            resizable_panel().child(
                                div()
                                    .size_full()
                                    .flex()
                                    .flex_col()
                                    .min_h(px(0.0))
                                    .overflow_hidden()
                                    .child(self.render_listing(window, cx)),
                            ),
                        )
                        .child(
                            resizable_panel()
                                .size(px(240.0))
                                .size_range(px(240.0)..px(600.0))
                                .child(
                                    div()
                                        .size_full()
                                        .overflow_hidden()
                                        .border_l_1()
                                        .border_color(rgb(theme::BORDER))
                                        .child(self.render_preview()),
                                ),
                        )
                        .into_any_element(),
                ),
            )
            .when(self.search_visible, |this| {
                this.child(self.render_floating_search(window, cx))
            })
    }
}

impl ExplorerPage {
    fn render_header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parts = path_parts(&self.cwd);

        let (display_parts, is_truncated) = if parts.len() > 5 {
            (parts[(parts.len() - 5)..].to_vec(), true)
        } else {
            (parts.clone(), false)
        };

        let mut bc = Breadcrumb::new();

        if is_truncated {
            bc = bc.item(
                BreadcrumbItem::new("ellipsis", "…")
                    .on_click(cx.listener(move |_this, _, _, _| {})),
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
                path_here = self.cwd.clone();
            }

            bc = bc.item(
                BreadcrumbItem::new(("bc", actual_i), text).on_click(cx.listener(
                    move |this, _, window, cx| this.change_dir(path_here.clone(), window, cx),
                )),
            );
        }

        let can_go_back = self.history_index > 0;
        let can_go_forward = self.history_index + 1 < self.history.len();

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
                        gpui_component::ListItem::new("nav-back")
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
                        gpui_component::ListItem::new("nav-forward")
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
                            .child(format!("{} items", self.filtered_entries.len())),
                    )
                    .child(self.render_view_mode_toggle(cx))
                    .child(
                        gpui_component::ListItem::new("search-toggle")
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.toggle_search(window, cx);
                            }))
                            .child(Icon::new(IconName::Search).size_4().text_color(
                                if self.search_visible {
                                    rgb(theme::ACCENT)
                                } else {
                                    rgb(theme::GRAY_600)
                                },
                            )),
                    ),
            )
    }

    fn render_view_mode_toggle(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.view_mode_button(
                ViewMode::List,
                "view-mode-list",
                IconName::PanelBottomOpen,
                "List",
                cx,
            ))
            .child(self.view_mode_button(
                ViewMode::Grid,
                "view-mode-grid",
                IconName::LayoutDashboard,
                "Grid",
                cx,
            ))
    }

    fn view_mode_button(
        &mut self,
        mode: ViewMode,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.view_mode == mode;
        gpui_component::ListItem::new(id)
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

    fn render_sidebar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(self.sidebar_item(IconName::Folder, "Home", true, cx))
                    .child(self.sidebar_item(IconName::Star, "Favorites", false, cx))
                    .child(self.sidebar_item(IconName::File, "Recent", false, cx))
                    .child(self.sidebar_item(IconName::Folder, "Trash", false, cx)),
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
                    .child(self.render_shortcuts(cx)),
            )
    }

    fn sidebar_item(
        &self,
        icon: IconName,
        label: &str,
        _active: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

    fn render_shortcuts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let shortcuts = self.shortcuts();
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
                gpui_component::ListItem::new(("shortcut", i))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change_dir(p.clone(), window, cx)
                    }))
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

    fn render_listing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_list_initialized(window, cx);
        match self.view_mode {
            ViewMode::List => self.render_list_view(cx),
            ViewMode::Grid => self.render_grid_view(window, cx),
        }
    }

    fn render_list_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let table_width = self.total_table_width();
        let col_name = self.col_name_width;
        let col_type = self.col_type_width;
        let col_size = self.col_size_width;
        let col_modified = self.col_modified_width;
        let col_action = self.col_action_width;

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(self.render_table_with_header(
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

    fn render_grid_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let items = self.filtered_entries.clone();
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap_4()
            .items_start()
            .min_h(px(0.0));

        for (ix, item) in items.into_iter().enumerate() {
            let selected = self.selected_index == Some(ix);
            grid = grid.child(self.render_grid_item(item, ix, selected, window, cx));
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
        &mut self,
        item: FileEntryDto,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
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

    fn render_floating_search(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current_text = self.search_input.read(cx).text().to_string();
        if current_text != self.search_query {
            self.search_query = current_text;
            self.apply_filter();
        }

        let is_empty = self.search_query.is_empty();
        let match_count = self.filtered_entries.len();

        div()
            .absolute()
            .top(px(12.0))
            .right(px(24.0))
            .w(px(360.0))
            .bg(rgb(theme::BG))
            .border_1()
            .border_color(rgb(theme::BORDER))
            .rounded(px(8.0))
            .shadow_lg()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, _ev: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|_this, _ev: &gpui::MouseUpEvent, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|_this, _ev: &gpui::MouseMoveEvent, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(cx.listener(|_this, _ev, _window, cx| {
                cx.stop_propagation();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(12.0))
                    .py(px(10.0))
                    .child(
                        Icon::new(IconName::Search)
                            .size_4()
                            .text_color(rgb(theme::FG_SECONDARY)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child({
                                let si = self.search_input.clone();
                                TextInput::new(&si)
                            })
                            .child(
                                div().h(px(18.0)).child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::FG_SECONDARY))
                                        .when(!is_empty, |this| {
                                            this.child(format!("{} matches", match_count))
                                        }),
                                ),
                            ),
                    )
                    .child(
                        gpui_component::ListItem::new("close-search")
                            .px(px(4.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.toggle_search(window, cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme::MUTED))
                                    .child("×"),
                            ),
                    ),
            )
    }

    fn render_table_with_header(
        &mut self,
        table_width: f32,
        col_name: f32,
        col_type: f32,
        col_size: f32,
        col_modified: f32,
        col_action: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();

        let mut all_sizes = vec![gpui::size(px(table_width), px(48.0))];
        all_sizes.extend(self.item_sizes.as_ref().iter().copied());
        let all_sizes = Rc::new(all_sizes);
        let scroll_handle = self.virtual_scroll_handle.clone();

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
                                    view.render_header_row(
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
                                view.filtered_entries.get(data_ix).map(|item| {
                                    view.render_file_row(item, data_ix, cx).into_any_element()
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
        &self,
        table_width: f32,
        col_name: f32,
        col_type: f32,
        col_size: f32,
        col_modified: f32,
        col_action: f32,
        cx: &mut Context<Self>,
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
                    .child(self.render_resizable_column_header(
                        "Name",
                        SortKey::Name,
                        0,
                        col_name,
                        cx,
                    ))
                    .child(self.render_resizable_column_header(
                        "Type",
                        SortKey::Type,
                        1,
                        col_type,
                        cx,
                    ))
                    .child(self.render_resizable_column_header(
                        "Size",
                        SortKey::Size,
                        2,
                        col_size,
                        cx,
                    ))
                    .child(self.render_resizable_column_header(
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
        &self,
        label: &str,
        key: SortKey,
        column_index: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .w(px(width))
            .flex_shrink_0()
            .relative()
            .child(self.render_column_header(label, key, column_index, false, cx))
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

    fn render_file_row(
        &self,
        item: &FileEntryDto,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use crate::ui::components::file_list::{format_date, get_file_type, human_bytes};
        use gpui_component::ListItem;

        let icon_name = match item.kind.as_str() {
            "dir" => IconName::Folder,
            _ => IconName::File,
        };

        let bg_color = if ix % 2 == 0 {
            theme::BG
        } else {
            theme::GRAY_50
        };

        let file_type = get_file_type(&item.name, &item.kind);

        let max_chars = (self.col_name_width / 8.0) as usize;
        let display_name = truncate_middle(&item.name, max_chars.max(20));

        let total_width = self.total_table_width();
        let item_for_preview = item.clone();
        let item_for_activate = item.clone();

        ListItem::new(("file-row", ix))
            .w(px(total_width))
            .h(px(32.0))
            .px(px(24.0))
            .bg(rgb(bg_color))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if let gpui::ClickEvent::Mouse(mouse) = event {
                        if mouse.up.button == gpui::MouseButton::Left {
                            this.record_click(ix, mouse.up.click_count);
                            this.selected_index = Some(ix);
                            if item_for_preview.kind == "file" {
                                this.open_preview(item_for_preview.path.clone());
                            }
                            if mouse.up.click_count >= 2 {
                                this.activate_entry(item_for_activate.clone(), window, cx);
                            }
                        }
                    } else if let gpui::ClickEvent::Keyboard(_) = event {
                        this.selected_index = Some(ix);
                        this.activate_entry(item_for_activate.clone(), window, cx);
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .h_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .w(px(self.col_name_width))
                            .flex_shrink_0()
                            .child(
                                Icon::new(icon_name)
                                    .size_4()
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
                                    .child(display_name),
                            ),
                    )
                    .child(
                        div()
                            .w(px(self.col_type_width))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(file_type),
                    )
                    .child(
                        div()
                            .w(px(self.col_size_width))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .child(match item.kind.as_str() {
                                "file" => human_bytes(item.size),
                                "dir" => "-".to_string(),
                                other => other.to_string(),
                            }),
                    )
                    .child(
                        div()
                            .w(px(self.col_modified_width))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(format_date(&item.modified)),
                    )
                    .child(
                        div()
                            .w(px(self.col_action_width))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .child(
                                Icon::new(IconName::File)
                                    .size_4()
                                    .text_color(rgb(theme::MUTED)),
                            ),
                    ),
            )
    }

    fn render_column_header(
        &self,
        label: &str,
        key: SortKey,
        key_idx: usize,
        flex: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let is_active = self.sort_key == key;
        let label_str = label.to_string();
        let sort_icon = if is_active {
            Some(if self.sort_asc { "↑" } else { "↓" })
        } else {
            None
        };

        let mut wrapper = div();

        if flex {
            wrapper = wrapper.flex_1();
        }

        wrapper.child(
            gpui_component::ListItem::new(("sort-header", key_idx))
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

    fn render_preview(&mut self) -> impl IntoElement {
        let title = self
            .preview_path
            .as_ref()
            .map(|p| path_name(p))
            .unwrap_or_else(|| "Preview".to_string());

        let body: String = self
            .preview_text
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| "Select a file to see a preview".into());

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(rgb(theme::BORDER))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::FG))
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .px(px(16.0))
                    .py(px(16.0))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .line_height(px(20.0))
                            .child(body),
                    ),
            )
    }
}

impl crate::pages::Page for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}

fn path_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
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

fn truncate_middle(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();

    if char_count <= max_len {
        return text.to_string();
    }

    if let Some(dot_pos) = text.rfind('.') {
        let name_part = &text[..dot_pos];
        let ext_part = &text[dot_pos..];
        let name_chars: Vec<char> = name_part.chars().collect();
        let ext_chars = ext_part.chars().count();

        if name_chars.len() > max_len - ext_chars - 3 {
            let keep_start = (max_len - ext_chars - 3) / 2;
            let keep_end = (max_len - ext_chars - 3) - keep_start;

            let start_part: String = name_chars[..keep_start].iter().collect();
            let end_part: String = name_chars[name_chars.len() - keep_end..].iter().collect();

            format!("{}...{}{}", start_part, end_part, ext_part)
        } else {
            text.to_string()
        }
    } else {
        let chars: Vec<char> = text.chars().collect();
        let keep_start = (max_len - 3) / 2;
        let keep_end = (max_len - 3) - keep_start;

        let start_part: String = chars[..keep_start].iter().collect();
        let end_part: String = chars[chars.len() - keep_end..].iter().collect();

        format!("{}...{}", start_part, end_part)
    }
}

fn get_extension(name: &str, kind: &str) -> String {
    match kind {
        "dir" => "0_dir".to_string(),
        "file" => std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "zzz_noext".to_string()),
        other => other.to_string(),
    }
}
