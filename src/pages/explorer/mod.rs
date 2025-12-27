use crate::services::fs::listing::{list_dir_sync, FileEntryDto, ListParams};
use crate::services::search::{SearchScope, SearchService};
use crate::ui::components::file_list::FileListDelegate;

use crate::services::syntax::SyntaxService;
use gpui::{
    px, size, AnyElement, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, Render,
    ScrollHandle, Window,
};
use gpui_component::input::InputState;
use gpui_component::list::{List, ListEvent};
use gpui_component::resizable::ResizableState;
use gpui_component::VirtualListScrollHandle;
use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;
use tokio::task;

mod entries;
mod search;
mod types;
pub mod view;
use types::*;
use view::preview::editor::PreviewEditor;

pub struct ExplorerPage {
    pub cwd: String,
    pub history: Vec<String>,
    pub history_index: usize,
    pub entries: Vec<FileEntryDto>,
    pub filtered_entries: Vec<FileEntryDto>,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub search_query: String,
    pub search_visible: bool,
    pub search_input: Entity<InputState>,
    pub resizable: Entity<ResizableState>,
    pub list: Option<Entity<List<FileListDelegate>>>,
    pub subs: Vec<gpui::Subscription>,
    pub preview_path: Option<String>,
    pub preview_text: Option<String>,
    pub selected_index: Option<usize>,
    pub virtual_scroll_handle: VirtualListScrollHandle,
    pub item_sizes: Rc<Vec<gpui::Size<gpui::Pixels>>>,
    // Column widths (resizable)
    pub col_name_width: f32,
    pub col_type_width: f32,
    pub col_size_width: f32,
    pub col_modified_width: f32,
    pub col_action_width: f32,
    // Resize state
    pub resizing_column: Option<ResizingColumn>,
    pub focus_handle: FocusHandle,
    pub focus_requested: bool,
    pub last_click_info: Option<LastClickInfo>,
    pub view_mode: ViewMode,

    // Search
    pub search_service: Arc<SearchService>,
    pub search_scope: SearchScope,
    pub search_type: SearchType,
    pub match_case: bool,
    pub match_whole_word: bool,
    pub use_regex: bool,
    pub search_results: Option<Vec<SearchFileResult>>, // Changed type
    pub is_performing_search: bool,
    pub expanded_search_files: std::collections::HashSet<String>,
    // Syntax
    pub syntax_service: Arc<SyntaxService>,
    // Preview State
    pub preview_image_path: Option<String>,
    pub preview_message: Option<String>,
    pub preview_editor: Option<Entity<PreviewEditor>>,
    // Removed legacy preview fields
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

const CONFIRM_SUPPRESS_WINDOW: Duration = Duration::from_millis(300);

impl ExplorerPage {
    pub fn new(
        resizable: Entity<ResizableState>,
        search_input: Entity<InputState>,
        search_service: Arc<SearchService>,
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

            // Search
            search_service,
            search_scope: SearchScope::Home,
            search_type: SearchType::All,
            match_case: false,
            match_whole_word: false,
            use_regex: false,
            search_results: None,
            is_performing_search: false,
            expanded_search_files: std::collections::HashSet::new(),
            syntax_service: Arc::new(SyntaxService::new()),
            preview_editor: None,
            preview_image_path: None,
            preview_message: None,
        }
    }

    fn trigger_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_query.is_empty() {
            self.search_results = None;
            self.apply_filter();
            cx.notify();
            return;
        }

        self.is_performing_search = true;
        cx.notify();

        let service = self.search_service.clone();
        let query = self.search_query.clone();
        let scope = self.search_scope;

        let handle = Handle::current();
        let results = task::block_in_place(move || handle.block_on(service.search(query, scope)));

        match results {
            Ok(res) => {
                let grouped = search::group_results(res);
                let entries = search::results_to_entries(&grouped);

                self.filtered_entries = entries;
                self.search_results = Some(grouped);
            }
            Err(e) => {
                tracing::error!("Search failed: {}", e);
                self.search_results = Some(Vec::new());
                self.filtered_entries = Vec::new(); // Clear filtered entries on search error
            }
        }
        self.is_performing_search = false;
        self.update_item_sizes();
        self.update_editor_search(window, cx);
        cx.notify();
    }

    fn set_search_scope(&mut self, scope: SearchScope, cx: &mut Context<Self>) {
        if self.search_scope != scope {
            self.search_scope = scope;
            cx.notify();
        }
    }

    fn toggle_match_case(&mut self, cx: &mut Context<Self>) {
        self.match_case = !self.match_case;
        cx.notify();
    }

    fn toggle_match_whole_word(&mut self, cx: &mut Context<Self>) {
        self.match_whole_word = !self.match_whole_word;
        cx.notify();
    }

    fn toggle_use_regex(&mut self, cx: &mut Context<Self>) {
        self.use_regex = !self.use_regex;
        cx.notify();
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
            entries::sort_entries(&mut e, self.sort_key, self.sort_asc);
            self.entries = e;
            self.apply_filter();
            self.update_item_sizes();
            self.preview_text = None;
            self.preview_path = None;
            self.preview_editor = None;
            self.preview_image_path = None;
            self.preview_message = None;
        }
    }

    fn update_editor_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor_entity) = &self.preview_editor {
            let query = self.search_query.clone();
            let editor_entity = editor_entity.clone();
            editor_entity.update(cx, |editor, cx| {
                editor.set_search_query(query, window, cx);
            });
        }
    }

    pub fn scroll_to_line(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = &self.preview_text {
            let mut current_off = 0;
            let mut found_offset = None;
            // 1-based line number to 0-based index
            let target_idx = line.saturating_sub(1);

            for (i, line_str) in text.lines().enumerate() {
                if i == target_idx {
                    found_offset = Some(current_off);
                    break;
                }
                let consumed = line_str.len();
                let remainder = &text[current_off + consumed..];
                let newline_len = if remainder.starts_with("\r\n") {
                    2
                } else if remainder.starts_with('\n') {
                    1
                } else {
                    0
                };
                current_off += consumed + newline_len;
            }

            if let Some(offset) = found_offset {
                if let Some(editor) = &self.preview_editor {
                    let editor = editor.clone();
                    editor.update(cx, |editor, cx| {
                        editor.scroll_to(offset, window, cx);
                    });
                }
            }
        }
    }

    fn update_item_sizes(&mut self) {
        let total_width = self.total_table_width();
        let base_row_height = 32.0;
        let snippet_row_height = 24.0;
        let max_snippets = 10;

        let sizes = self
            .filtered_entries
            .iter()
            .map(|entry| {
                // Check if there are match snippets for this file AND it's expanded
                let is_expanded = self.expanded_search_files.contains(&entry.path);
                let snippet_count = if is_expanded {
                    self.search_results
                        .as_ref()
                        .and_then(|results| {
                            results
                                .iter()
                                .find(|r| r.path == entry.path)
                                .map(|r| r.matches.len().min(max_snippets))
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                let total_height = base_row_height + (snippet_count as f32 * snippet_row_height);
                size(px(total_width), px(total_height))
            })
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
        if self.search_results.is_some() {
            return;
        }

        if self.entries.is_empty() {
            self.filtered_entries = Vec::new();
            return;
        }

        self.filtered_entries = self.entries.clone();

        // Apply sorting
        entries::sort_entries(&mut self.filtered_entries, self.sort_key, self.sort_asc);
    }

    fn set_sort_key(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = true;
        }
        let mut e = self.entries.clone();
        entries::sort_entries(&mut e, self.sort_key, self.sort_asc);
        self.entries = e;
        self.apply_filter();
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
            self.open_preview(item.path, window, cx);
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
                                this.open_preview(item.path, window, cx);
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
        self.search_results = None;
        self.search_query.clear();
        self.apply_filter();
        self.apply_filter();
        self.update_editor_search(window, cx);
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_visible {
            self.close_search(window, cx);
        } else {
            self.open_search(window, cx);
        }
    }

    fn open_preview(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.preview_editor = None;
        self.preview_image_path = None;
        self.preview_message = None;
        self.preview_text = None;
        self.preview_path = None;

        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_file() {
                if md.len() > 1024 * 1024 * 2 {
                    self.preview_path = Some(path);
                    self.preview_message = Some("(File too large to preview)".to_string());
                    return;
                }

                if let Ok(bytes) = std::fs::read(&path) {
                    // check if binary
                    // Simple check: null byte usually (text check) or just try utf8
                    if let Ok(text) = String::from_utf8(bytes) {
                        self.preview_path = Some(path.clone());
                        self.preview_text = Some(text.clone());

                        let editor_view = cx.new(|cx| PreviewEditor::new(window, cx));
                        let text_clone = text.clone();
                        let extension = std::path::Path::new(&path)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        let language = match extension.as_str() {
                            "rs" => "rust",
                            "md" => "markdown",
                            "json" => "json",
                            "js" => "javascript",
                            "ts" => "typescript",
                            "html" => "html",
                            "go" => "go",
                            "zig" => "zig",
                            "toml" => "toml",
                            "yaml" | "yml" => "yaml",
                            "css" => "css",
                            "c" => "c",
                            "cpp" => "cpp",
                            _ => "plain",
                        }
                        .to_string();

                        editor_view.update(cx, |editor, cx| {
                            editor.set_text(text_clone, window, cx);
                            if language != "plain" {
                                editor.set_language(language, window, cx);
                            }
                        });

                        self.preview_editor = Some(editor_view.into());

                        // Highlights (Search only, syntax handled by editor)
                        self.update_editor_search(window, cx);

                        // Scroll to first match
                        if let Some(results) = &self.search_results {
                            if let Some(file_result) = results.iter().find(|r| &r.path == &path) {
                                if let Some(first_match) = file_result.matches.first() {
                                    // Calculate offset
                                    // We need to re-calculate offset from line number here or do it inside update logic?
                                    // Better: calculate offset here.
                                    let mut current_off = 0;
                                    let target_line = first_match.line_number;
                                    let mut found_offset = None;

                                    for (i, line) in text.lines().enumerate() {
                                        if i == target_line {
                                            found_offset = Some(current_off);
                                            break;
                                        }
                                        let consumed = line.len();
                                        // Handle newline length approximation (since lines() trips it)
                                        // We check actual chars in text?
                                        // Safer: slice the original text to find newlines.
                                        // Or logic reused from update_editor_highlights.
                                        // Let's copy logic for consistency.
                                        let remainder = &text[current_off + consumed..];
                                        let newline_len = if remainder.starts_with("\r\n") {
                                            2
                                        } else if remainder.starts_with('\n') {
                                            1
                                        } else {
                                            0
                                        };
                                        current_off += consumed + newline_len;
                                    }

                                    if let Some(offset) = found_offset {
                                        if let Some(editor) = &self.preview_editor {
                                            let editor = editor.clone();
                                            editor.update(cx, |editor, cx| {
                                                editor.scroll_to(offset, window, cx);
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        return;
                    } else {
                        // Binary? Check extension for image
                        let extension = std::path::Path::new(&path)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        match extension.as_str() {
                            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" => {
                                self.preview_path = Some(path.clone());
                                self.preview_image_path = Some(path);
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        self.preview_path = Some(path);
        self.preview_message = Some("(Preview not available for this file)".to_string());
    }
}

impl Render for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        view::render(self, window, cx)
    }
}

impl crate::pages::Page for ExplorerPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        <Self as Render>::render(self, window, cx).into_any_element()
    }
}
