use nohrs_core::config;
use nohrs_ui::components::file_list::FileListDelegate;

use gpui::{AppContext, Context, Window};
use gpui_component::list::{List, ListEvent};

use super::ExplorerPage;
use super::types::ResizingColumn;

impl ExplorerPage {
    pub(crate) fn ensure_list_initialized(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                                this.open_preview(item.path, window, cx);
                            }
                        }
                    }
                    ListEvent::Confirm(ix) => {
                        if let Some(info) = this.last_click_info.as_ref() {
                            if info.row == ix.row
                                && info.timestamp.elapsed() < config::CONFIRM_SUPPRESS_WINDOW
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

    pub(crate) fn start_column_resize(
        &mut self,
        column_index: usize,
        start_pos: gpui::Point<gpui::Pixels>,
    ) {
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

    pub(crate) fn update_column_resize(&mut self, current_pos: gpui::Point<gpui::Pixels>) {
        if let Some(resize) = self.resizing_column {
            let delta: f32 = (current_pos.x - resize.start_x.x).into();
            let new_width = (resize.start_width + delta).max(config::MIN_COLUMN_WIDTH);

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

    pub(crate) fn stop_column_resize(&mut self) {
        self.resizing_column = None;
    }
}
