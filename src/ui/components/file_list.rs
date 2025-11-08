#![cfg(feature = "gui")]

use crate::services::fs::listing::FileEntryDto;
use crate::ui::theme::theme;
use gpui::{div, Window, ParentElement, Styled, px, rgb};
use gpui_component::list::{List, ListDelegate, ListItem};
use gpui_component::{IndexPath, Icon, IconName};

pub struct FileListDelegate {
    pub items: Vec<FileEntryDto>,
    pub selected: Option<IndexPath>,
    // Callback hooks
    pub on_confirm: Option<Box<dyn Fn(&FileEntryDto) + 'static>>,
}

impl FileListDelegate {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            on_confirm: None,
        }
    }

    pub fn set_items(&mut self, items: Vec<FileEntryDto>) {
        self.items = items;
        self.selected = None;
    }

    pub fn get_selected(&self) -> Option<&FileEntryDto> {
        self.selected.map(|ix| self.items.get(ix.row)).flatten()
    }
}

impl ListDelegate for FileListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &gpui::App) -> usize {
        self.items.len()
    }

    fn render_item(
        &self,
        ix: IndexPath,
        _window: &mut Window,
        _cx: &mut gpui::Context<List<Self>>,
    ) -> Option<Self::Item> {
        let item = self.items.get(ix.row)?;

        // Icon based on file type
        let icon_name = match item.kind.as_str() {
            "dir" => IconName::Folder,
            _ => IconName::File,
        };

        // Alternate row background for zebra striping
        let bg_color = if ix.row % 2 == 0 {
            theme::BG
        } else {
            theme::GRAY_50
        };

        let file_type = get_file_type(&item.name, &item.kind);

        let mut row = ListItem::new(ix.clone())
            .py(px(6.0))  // Reduced from 12.0 for compact rows
            .px(px(24.0))
            .bg(rgb(bg_color))
            .child(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .child(
                        // Name column with icon - flexible, takes remaining space
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .min_w(px(150.0))
                            .child(
                                Icon::new(icon_name)
                                    .size_4()
                                    .text_color(rgb(theme::GRAY_600))
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme::FG))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(item.name.clone())
                            )
                    )
                    .child(
                        // Type column - compact
                        div()
                            .w(px(70.0))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(file_type)
                    )
                    .child(
                        // Size column - compact
                        div()
                            .w(px(70.0))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(match item.kind.as_str() {
                                "file" => human_bytes(item.size),
                                "dir" => "-".to_string(),
                                other => other.to_string(),
                            })
                    )
                    .child(
                        // Modified column - compact
                        div()
                            .w(px(90.0))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(format_date(&item.modified))
                    )
                    .child(
                        // Actions column - compact
                        div()
                            .w(px(40.0))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .child(
                                Icon::new(IconName::File)
                                    .size_4()
                                    .text_color(rgb(theme::MUTED))
                                    .cursor_pointer()
                            )
                    )
            );

        // enable click to confirm
        let item_clone = item.clone();
        if self.on_confirm.is_some() {
            let cb = self.on_confirm.as_ref().unwrap();
            // We cannot capture trait object by move directly; wrap call inside closure
            let ptr = cb as *const _;
            row = row.on_click(move |_, _, _| unsafe {
                // SAFETY: lifetime tied to delegate existence within app
                let f: &Box<dyn Fn(&FileEntryDto)> = &*ptr;
                (f)(&item_clone);
            });
        }
        Some(row)
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut gpui::Context<List<Self>>,
    ) {
        self.selected = ix;
    }

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, _cx: &mut gpui::Context<List<Self>>) {
        if let Some(ix) = self.selected {
            if let Some(item) = self.items.get(ix.row) {
                if let Some(cb) = &self.on_confirm {
                    cb(item);
                }
            }
        }
    }
}

pub fn human_bytes(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;
    let s = size as f64;
    if s >= GB {
        format!("{:.1} GB", s / GB)
    } else if s >= MB {
        format!("{:.1} MB", s / MB)
    } else if s >= KB {
        format!("{:.1} KB", s / KB)
    } else {
        format!("{} B", size)
    }
}

pub fn format_date(timestamp: &u64) -> String {
    // Convert unix timestamp to readable date
    use std::time::{Duration, UNIX_EPOCH};

    let d = UNIX_EPOCH + Duration::from_secs(*timestamp);
    if let Ok(datetime) = d.duration_since(UNIX_EPOCH) {
        let secs = datetime.as_secs();
        let days = secs / 86400;
        let years_since_epoch = days / 365;
        let year = 1970 + years_since_epoch;
        let remaining_days = days % 365;
        let month = (remaining_days / 30) + 1;
        let day = (remaining_days % 30) + 1;
        format!("{:04}/{:02}/{:02}", year, month, day)
    } else {
        "-".to_string()
    }
}

pub fn get_file_type(name: &str, kind: &str) -> String {
    match kind {
        "dir" => "フォルダ".to_string(),
        "file" => {
            if let Some(ext) = std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
            {
                ext.to_uppercase()
            } else {
                "ファイル".to_string()
            }
        }
        "symlink" => "リンク".to_string(),
        other => other.to_string(),
    }
}
