use crate::services::fs::listing::{list_dir_sync, FileEntryDto, ListParams};
use crate::services::search::{SearchResult, SearchScope, SearchService};
use crate::ui::components::file_list::FileListDelegate;
use crate::ui::theme::theme;

use crate::services::syntax::SyntaxService;
use gpui::{
    div, prelude::*, px, rgb, size, AnyElement, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, Render, ScrollHandle, SharedString, StyledText, Window,
};
use gpui_component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_component::input::{InputState, TextInput};
use gpui_component::list::{List, ListEvent};
use gpui_component::resizable::{h_resizable, resizable_panel, ResizableState};
use gpui_component::{v_virtual_list, Icon, IconName, VirtualListScrollHandle};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;
use tokio::task;

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Filename,
    Content,
    All,
}

// Search Data Structures
#[derive(Clone)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Clone)]
pub struct SearchFileResult {
    pub path: String,
    pub folder: String,
    pub filename: String,
    pub matches: Vec<SearchMatch>,
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

    // Search
    search_service: Arc<SearchService>,
    search_scope: SearchScope,
    search_type: SearchType,
    match_case: bool,
    match_whole_word: bool,
    use_regex: bool,
    search_results: Option<Vec<SearchFileResult>>, // Changed type
    is_performing_search: bool,
    expanded_search_files: std::collections::HashSet<String>,
    // Syntax
    syntax_service: Arc<SyntaxService>,
    preview_highlights: Option<Vec<(std::ops::Range<usize>, gpui::Hsla)>>,
    // Virtual Preview
    preview_lines: Vec<String>,
    preview_line_highlights: HashMap<usize, Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>>,
    preview_virtual_handle: VirtualListScrollHandle,
    preview_scroll_handle: ScrollHandle,
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
            preview_highlights: None,
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
            preview_lines: Vec::new(),
            preview_line_highlights: HashMap::new(),
            preview_virtual_handle: VirtualListScrollHandle::new(),
            preview_scroll_handle: ScrollHandle::new(),
        }
    }

    fn trigger_search(&mut self, cx: &mut Context<Self>) {
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
                let grouped = self.group_results(res);
                let entries: Vec<FileEntryDto> = grouped
                    .iter()
                    .map(|res| {
                        let meta = std::fs::metadata(&res.path).ok();
                        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                        let modified = meta
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                        FileEntryDto {
                            name: if res.folder.is_empty() {
                                res.filename.clone()
                            } else {
                                format!("{}/{}", res.folder, res.filename)
                            },
                            path: res.path.clone(),
                            kind: if is_dir {
                                "dir".to_string()
                            } else {
                                "file".to_string()
                            },
                            size,
                            modified,
                        }
                    })
                    .collect();

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
        cx.notify();
    }

    fn group_results(&self, results: Vec<SearchResult>) -> Vec<SearchFileResult> {
        let mut file_map: HashMap<String, SearchFileResult> = HashMap::new();

        for res in results {
            let entry = file_map
                .entry(res.path.to_string_lossy().to_string())
                .or_insert_with(|| {
                    let path = std::path::Path::new(&res.path);
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    // Improved folder logic: show relative path from CWD or Home
                    // Since we index Documents, let's try to be relative to CWD if possible
                    let folder = if let Some(parent) = path.parent() {
                        parent.to_string_lossy().to_string()
                    } else {
                        String::new()
                    };

                    SearchFileResult {
                        path: res.path.to_string_lossy().to_string(),
                        folder, // simplified
                        filename,
                        matches: Vec::new(),
                    }
                });

            // Only add to matches if it's a real content match (not filename-only)
            if res.line_number > 0 {
                entry.matches.push(SearchMatch {
                    line_number: res.line_number,
                    line_content: res.line_content,
                    match_start: 0,
                    match_end: 0,
                });
            }
        }

        let mut sorted_results: Vec<SearchFileResult> = file_map.into_values().collect();
        sorted_results.sort_by(|a, b| a.path.cmp(&b.path));

        // Debug log if NOHR_DEBUG is set
        if std::env::var("NOHR_DEBUG").is_ok() {
            for r in &sorted_results {
                tracing::info!(
                    "[DEBUG] group_results: file='{}', matches={}",
                    r.filename,
                    r.matches.len()
                );
            }
        }

        sorted_results
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
            self.sort_entries(&mut e);
            self.entries = e;
            self.apply_filter();
            self.update_item_sizes();
            self.preview_text = None;
            self.preview_path = None;
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
        let key = self.sort_key;
        let asc = self.sort_asc;

        self.filtered_entries.sort_by(|a, b| {
            let order = match key {
                SortKey::Name => a.name.cmp(&b.name),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Modified => a.modified.cmp(&b.modified),
                SortKey::Type => {
                    let type_a = crate::ui::components::file_list::get_file_type(&a.name, &a.kind);
                    let type_b = crate::ui::components::file_list::get_file_type(&b.name, &b.kind);
                    type_a.cmp(&type_b)
                }
            };
            if asc {
                order
            } else {
                order.reverse()
            }
        });
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
        entries.sort_by(|a, b| {
            // Directories before files
            let is_dir_a = a.kind == "dir";
            let is_dir_b = b.kind == "dir";

            match is_dir_b.cmp(&is_dir_a) {
                std::cmp::Ordering::Equal => {
                    let order = match self.sort_key {
                        SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                        SortKey::Size => a.size.cmp(&b.size),
                        SortKey::Modified => a.modified.cmp(&b.modified),
                        SortKey::Type => {
                            let ext_a = get_extension(&a.name, &a.kind);
                            let ext_b = get_extension(&b.name, &b.kind);
                            ext_a.cmp(&ext_b)
                        }
                    };
                    if self.sort_asc {
                        order
                    } else {
                        order.reverse()
                    }
                }
                kind_order => kind_order,
            }
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
        self.search_results = None;
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
        // Reset virtual preview data
        self.preview_lines.clear();
        self.preview_line_highlights.clear();

        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_file() && md.len() <= 1024 * 1024 * 2 {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        let highlights = self.syntax_service.highlight(
                            &text,
                            std::path::Path::new(&path)
                                .extension()
                                .and_then(|s| s.to_str()),
                        );
                        self.preview_path = Some(path);

                        // Populate lines
                        self.preview_lines = text.lines().map(|s| s.to_string()).collect();
                        if self.preview_lines.is_empty() && !text.is_empty() {
                            // Handle case with no newlines but content
                            self.preview_lines.push(text.clone());
                        }

                        // Map highlights to lines
                        let mut line_highlights: HashMap<
                            usize,
                            Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
                        > = HashMap::new();
                        let mut current_offset = 0;

                        for (line_idx, line) in self.preview_lines.iter().enumerate() {
                            let line_len = line.len();
                            let line_end = current_offset + line_len; // End of content, excluding newline (usually)

                            // Check for overlapping highlights
                            for (range, color) in &highlights {
                                // Range overlap check
                                let start = range.start.max(current_offset);
                                let end = range.end.min(line_end);

                                if start < end {
                                    // Map to relative offset
                                    let rel_start = start - current_offset;
                                    let rel_end = end - current_offset;

                                    // Ensure valid range
                                    if rel_start <= line_len && rel_end <= line_len {
                                        line_highlights.entry(line_idx).or_default().push((
                                            rel_start..rel_end,
                                            gpui::HighlightStyle {
                                                color: Some(*color),
                                                ..Default::default()
                                            },
                                        ));
                                    }
                                }
                            }

                            // Advance offset (+1 for newline if not last line... technically lines() consumes newlines)
                            // We need to account for the newline character in the original text to keep offsets valid
                            // But `lines()` strips them.
                            // If we assume standard \n, we add 1. If \r\n, we add 2.
                            // To be precise, we should just scan the original text or use `match_indices`.
                            // Text byte indices might drift if we just +1.
                            // Use accumulation of line.len() + 1 serves as approximation but could fail on \r\n.
                            // Better: use `text[current_offset..]` to find the next line break.
                            let consumed = line.len();
                            let remainder = &text[current_offset + consumed..];
                            let newline_len = if remainder.starts_with("\r\n") {
                                2
                            } else if remainder.starts_with('\n') {
                                1
                            } else {
                                0
                            };
                            current_offset += consumed + newline_len;
                        }

                        self.preview_line_highlights = line_highlights;
                        self.preview_text = Some(text);
                        self.preview_highlights = Some(highlights);
                        return;
                    }
                }
            }
        }

        self.preview_path = Some(path);
        let msg = "(Preview not available for this file)".to_string();
        self.preview_text = Some(msg.clone());
        self.preview_lines = vec![msg];
        self.preview_highlights = None;
        self.preview_line_highlights.clear();
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
                                .size_range(px(240.0)..px(2000.0))
                                .child(
                                    div()
                                        .size_full()
                                        .overflow_hidden()
                                        .border_l_1()
                                        .border_color(rgb(theme::BORDER))
                                        .child(self.render_preview(window)),
                                ),
                        )
                        .into_any_element(),
                ),
            )
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

        let file_list = match self.view_mode {
            ViewMode::List => self.render_list_view(cx),
            ViewMode::Grid => self.render_grid_view(window, cx),
        };

        if self.search_visible {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_inline_search_bar(cx))
                .child(file_list)
                .into_any_element()
        } else {
            file_list
        }
    }

    fn render_search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let results = match &self.search_results {
            Some(r) => r.clone(),
            None => return div().into_any_element(),
        };
        let query = self.search_query.clone();

        div()
            .id("search-results-scroll")
            .size_full()
            .overflow_scroll()
            .flex()
            .flex_col()
            .children(
                results
                    .into_iter()
                    .enumerate()
                    .map(|(file_idx, file_result)| {
                        let path = file_result.path.clone();
                        let display_name = if file_result.folder.is_empty() {
                            file_result.filename.clone()
                        } else {
                            format!("{}/{}", file_result.folder, file_result.filename)
                        };

                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .child(
                                // File header
                                div()
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .bg(rgb(theme::BG_HOVER))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::File)
                                            .size_4()
                                            .text_color(rgb(theme::GRAY_600)),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(theme::FG))
                                            .child(display_name),
                                    )
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _, _, _| {
                                            this.open_preview(path.clone());
                                        }),
                                    ),
                            )
                            .children(
                                file_result
                                    .matches
                                    .into_iter()
                                    .enumerate()
                                    .map(|(match_idx, m)| {
                                        let line_content = m.line_content.clone();
                                        let q = query.clone();

                                        // Highlight query in line content
                                        let highlights =
                                            Self::find_query_highlights(&line_content, &q);
                                        let styled = StyledText::new(line_content.clone())
                                            .with_highlights(highlights);

                                        div()
                                            .id(gpui::SharedString::from(format!(
                                                "match-{}-{}",
                                                file_idx, match_idx
                                            )))
                                            .px(px(24.0))
                                            .py(px(4.0))
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(theme::MUTED))
                                                    .w(px(40.0))
                                                    .child(format!("{}", m.line_number)),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(theme::FG_SECONDARY))
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .whitespace_nowrap()
                                                    .child(styled),
                                            )
                                    })
                                    .collect::<Vec<_>>(),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    }

    fn find_query_highlights(
        text: &str,
        query: &str,
    ) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> {
        let mut highlights = Vec::new();
        if query.is_empty() {
            return highlights;
        }

        let query_lower: Vec<char> = query.to_lowercase().chars().collect();
        let text_chars: Vec<(usize, char)> = text.char_indices().collect();

        let mut i = 0;
        while i < text_chars.len() {
            let mut match_found = true;
            let mut q_idx = 0;

            // Try to match query against text starting at i
            // We iterate text characters (i + t_offset) and their lowercased expansion
            let mut current_t_offset = 0;

            while q_idx < query_lower.len() {
                if i + current_t_offset >= text_chars.len() {
                    match_found = false;
                    break;
                }

                let (_, t_char) = text_chars[i + current_t_offset];
                let t_lower = t_char.to_lowercase();

                // Iterate through expansion of current text char
                for tc in t_lower {
                    if q_idx >= query_lower.len() || query_lower[q_idx] != tc {
                        match_found = false;
                        break;
                    }
                    q_idx += 1;
                }

                if !match_found {
                    break;
                }
                current_t_offset += 1;
            }

            if match_found && q_idx == query_lower.len() {
                // Match found! Calculate indices safely from char_indices
                let start_byte = text_chars[i].0;
                let end_byte = if i + current_t_offset < text_chars.len() {
                    text_chars[i + current_t_offset].0
                } else {
                    text.len()
                };

                highlights.push((
                    start_byte..end_byte,
                    gpui::HighlightStyle {
                        background_color: Some(gpui::Hsla::from(gpui::Rgba {
                            r: 1.0,
                            g: 0.9,
                            b: 0.0,
                            a: 0.5,
                        })),
                        color: Some(gpui::Hsla::from(gpui::Rgba {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        })),
                        ..Default::default()
                    },
                ));

                i += current_t_offset;
            } else {
                i += 1;
            }
        }

        // Universal defensive check: Ensure all highlights are valid char boundaries
        // This is strictly necessary to prevent panics in GPUI if logic drift occurs
        highlights.retain(|(range, _)| {
            let start_ok = text.is_char_boundary(range.start);
            let end_ok = text.is_char_boundary(range.end);
            if !start_ok || !end_ok {
                if std::env::var("NOHR_DEBUG").is_ok() {
                    tracing::error!("[CRITICAL] find_query_highlights: Removing invalid highlight: {:?} (start_ok={}, end_ok={}) in text len {}", range, start_ok, end_ok, text.len());
                }
                return false;
            }
            true
        });

        highlights
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

    fn render_inline_search_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_text = self.search_input.read(cx).text().to_string();
        if current_text != self.search_query {
            self.search_query = current_text;
            // If query changed, revert to file list filter until user explicitly triggers search?
            // Or we could auto-search? Auto-search is expensive for full content.
            // So default to file filter.
            self.search_results = None;
            self.apply_filter();
        }

        let is_empty = self.search_query.is_empty();
        let match_count = if let Some(results) = &self.search_results {
            results.iter().map(|r| r.matches.len()).sum()
        } else {
            self.filtered_entries.len()
        };
        let is_full_search = self.search_results.is_some();
        let status_text = if is_full_search {
            format!("{} matches in content", match_count)
        } else if !is_empty {
            format!("{} files filtered", match_count)
        } else {
            String::new()
        };

        div()
            .w_full()
            .bg(rgb(theme::BG))
            .border_b_1()
            .border_color(rgb(theme::BORDER))
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
                // Scope Selection
                div()
                    .flex()
                    .gap_4()
                    .px(px(12.0))
                    .pt(px(8.0))
                    .text_xs()
                    .text_color(rgb(theme::FG_SECONDARY))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child("Scope:")
                            .child(self.render_scope_button(SearchScope::Home, "Home", cx))
                            .child(self.render_scope_button(SearchScope::Root, "Root", cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child("Type:")
                            .child(self.render_type_button(SearchType::All, "All", cx))
                            .child(self.render_type_button(SearchType::Filename, "Filename", cx))
                            .child(self.render_type_button(SearchType::Content, "Content", cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .px(px(12.0))
                    .py(px(10.0))
                    .child(
                        Icon::new(IconName::Search)
                            .size_4()
                            .text_color(rgb(theme::FG_SECONDARY))
                            .mt(px(8.0)), // Align with input
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child({
                                let si = self.search_input.clone();
                                div()
                                    .on_key_down(cx.listener(
                                        |this, event: &gpui::KeyDownEvent, window, cx| {
                                            if event.keystroke.key == "enter" {
                                                this.trigger_search(cx);
                                            } else if event.keystroke.key == "escape" {
                                                this.toggle_search(window, cx);
                                            }
                                        },
                                    ))
                                    .child(TextInput::new(&si))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(self.render_toggle_button(
                                        "Aa",
                                        self.match_case,
                                        |this, cx| this.toggle_match_case(cx),
                                        cx,
                                    ))
                                    .child(self.render_toggle_button(
                                        "ab",
                                        self.match_whole_word,
                                        |this, cx| this.toggle_match_whole_word(cx),
                                        cx,
                                    ))
                                    .child(self.render_toggle_button(
                                        ".*",
                                        self.use_regex,
                                        |this, cx| this.toggle_use_regex(cx),
                                        cx,
                                    ))
                                    .child(div().flex_1())
                                    .child(
                                        // Run Search Button
                                        div()
                                            .cursor_pointer()
                                            .bg(rgb(theme::ACCENT))
                                            .text_color(rgb(theme::BG))
                                            .px(px(8.0))
                                            .py(px(2.0))
                                            .rounded(px(4.0))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("Search")
                                            .hover(|this| this.opacity(0.8))
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.trigger_search(cx);
                                                }),
                                            ),
                                    ),
                            )
                            .child(
                                div().h(px(18.0)).child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::FG_SECONDARY))
                                        .when(!status_text.is_empty(), |this| {
                                            this.child(status_text)
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

    fn render_scope_button(
        &self,
        scope: SearchScope,
        label: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.search_scope == scope;
        div()
            .cursor_pointer()
            .px(px(4.0))
            .rounded(px(4.0))
            .when(is_active, |this| {
                this.bg(rgb(theme::ACCENT)).text_color(rgb(theme::BG))
            })
            .when(!is_active, |this| {
                this.hover(|s| s.bg(rgb(theme::BG_HOVER)))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.set_search_scope(scope, cx);
                }),
            )
            .child(label.to_string())
    }

    fn render_type_button(
        &self,
        search_type: SearchType,
        label: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.search_type == search_type;
        div()
            .cursor_pointer()
            .px(px(4.0))
            .rounded(px(4.0))
            .when(is_active, |this| {
                this.bg(rgb(theme::ACCENT)).text_color(rgb(theme::BG))
            })
            .when(!is_active, |this| {
                this.hover(|s| s.bg(rgb(theme::BG_HOVER)))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.search_type = search_type;
                    cx.notify();
                }),
            )
            .child(label.to_string())
    }

    fn render_toggle_button(
        &self,
        label: &str,
        active: bool,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static + Copy,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .cursor_pointer()
            .border_1()
            .border_color(if active {
                rgb(theme::ACCENT)
            } else {
                rgb(theme::BORDER)
            })
            .bg(if active {
                rgb(theme::ACCENT_LIGHT)
            } else {
                rgb(theme::BG)
            })
            .px(px(4.0))
            .rounded(px(4.0))
            .text_xs()
            .font_family("Mono") // Monospace for regex/code like buttons
            .child(label.to_string())
            .hover(|this| this.bg(rgb(theme::BG_HOVER)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    on_click(this, cx);
                }),
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
        let icon_color = match item.kind.as_str() {
            "dir" => rgb(theme::ACCENT),
            _ => rgb(theme::GRAY_600),
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

        // Check if query matches filename (for highlighting)
        let query_lower = self.search_query.to_lowercase();
        let has_filename_match =
            !self.search_query.is_empty() && item.name.to_lowercase().contains(&query_lower);

        // Check if there are content matches (for expand arrow)
        let has_content_matches = self
            .search_results
            .as_ref()
            .map(|results| {
                results
                    .iter()
                    .any(|r| r.path == item.path && !r.matches.is_empty())
            })
            .unwrap_or(false);

        let is_expanded = self.expanded_search_files.contains(&item.path);

        let match_snippets: Vec<(usize, String)> = if is_expanded && has_content_matches {
            self.search_results
                .as_ref()
                .and_then(|results| {
                    results.iter().find(|r| r.path == item.path).map(|r| {
                        r.matches
                            .iter()
                            .take(10) // Limit to 10 snippets
                            .map(|m| (m.line_number, m.line_content.clone()))
                            .collect()
                    })
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let query = self.search_query.clone();
        let path_for_toggle = item.path.clone();
        let expand_icon = if is_expanded {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };

        // Create styled filename with highlighted matches
        let styled_name = if has_filename_match && !self.search_query.is_empty() {
            let highlights = Self::find_query_highlights(&display_name, &query);
            StyledText::new(display_name.clone()).with_highlights(highlights)
        } else {
            StyledText::new(display_name.clone())
        };

        div()
            .flex()
            .flex_col()
            .w(px(total_width))
            .child(
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
                                    .gap_1()
                                    .w(px(self.col_name_width))
                                    .flex_shrink_0()
                                    .when(has_content_matches, |this| {
                                        this.child(
                                            div()
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(rgb(theme::BG_HOVER)).rounded(px(4.0))
                                                })
                                                .p(px(2.0))
                                                .on_mouse_down(
                                                    gpui::MouseButton::Left,
                                                    cx.listener({
                                                        let path = path_for_toggle.clone();
                                                        move |this, _, _, cx| {
                                                            if this
                                                                .expanded_search_files
                                                                .contains(&path)
                                                            {
                                                                this.expanded_search_files
                                                                    .remove(&path);
                                                            } else {
                                                                this.expanded_search_files
                                                                    .insert(path.clone());
                                                            }
                                                            this.update_item_sizes();
                                                            cx.notify();
                                                        }
                                                    }),
                                                )
                                                .child(
                                                    Icon::new(expand_icon)
                                                        .size_3()
                                                        .text_color(rgb(theme::GRAY_600)),
                                                ),
                                        )
                                    })
                                    .when(!has_content_matches, |this| {
                                        this.child(div().w(px(20.0)))
                                    })
                                    .child(Icon::new(icon_name).size_4().text_color(icon_color))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme::FG))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .child(styled_name),
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
                            ),
                    ),
            )
            .children(
                match_snippets
                    .into_iter()
                    .map(|(line_num, content)| {
                        let highlights = Self::find_query_highlights(&content, &query);
                        let styled = StyledText::new(content.clone()).with_highlights(highlights);
                        let path = item.path.clone();
                        div()
                            .id(SharedString::from(format!(
                                "snippet-{}-{}",
                                path.clone(),
                                line_num
                            )))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let offset = ((line_num.max(1) - 1) as f32) * 20.0;
                                this.preview_scroll_handle
                                    .set_offset(gpui::Point::new(px(0.0), px(offset)));
                                cx.notify();
                            }))
                            .h(px(24.0)) // Fixed height matching update_item_sizes
                            .pl(px(48.0))
                            .pr(px(24.0))
                            .bg(rgb(theme::GRAY_50))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::MUTED))
                                    .w(px(32.0))
                                    .child(format!("{}", line_num)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::FG_SECONDARY))
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(styled),
                            )
                    })
                    .collect::<Vec<_>>(),
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

    fn render_preview(&mut self, window: &mut Window) -> impl IntoElement {
        let title = self
            .preview_path
            .as_ref()
            .map(|p| path_name(p))
            .unwrap_or_else(|| "Preview".to_string());

        let line_count = self.preview_lines.len().max(1);
        let max_digits = line_count.to_string().len();

        // Clone for closure
        let query = self.search_query.clone();

        // Virtual Scrolling Constants
        let row_height_px = px(20.0);
        // Use window height as approximation
        let viewport_height = window.viewport_size().height;

        // Scroll handle returns Point<Pixels>
        // Note: scroll offset is usually negative.
        let scroll_y = self.preview_scroll_handle.offset().y.abs();

        let visible_lines = (viewport_height / row_height_px) as usize + 1;
        let start_line = (scroll_y / row_height_px) as usize;
        let buffer = 20; // Extra lines

        let render_start = start_line.saturating_sub(buffer);
        let render_end = (start_line + visible_lines + buffer).min(self.preview_lines.len());

        // Spacers
        let padding_top = render_start as f32 * 20.0;
        let padding_bottom = (self.preview_lines.len().saturating_sub(render_end)) as f32 * 20.0;

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
                    .overflow_hidden() // Container needs simple overflow hidden
                    .px(px(16.0))
                    .py(px(16.0))
                    .child(
                        div()
                            .id("preview-scroll")
                            .size_full()
                            .overflow_scroll() // Native scrolling
                            .track_scroll(&self.preview_scroll_handle)
                            .flex()
                            .flex_col() // Vertical stack of lines
                            .text_sm()
                            .font_family("Mono")
                            .line_height(row_height_px)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .pt(px(padding_top))
                                    .pb(px(padding_bottom))
                                    .children(
                                        self.preview_lines
                                            .iter()
                                            .enumerate()
                                            .skip(render_start)
                                            .take(render_end - render_start)
                                            .map(|(ix, line_content)| {
                                                let line_num = ix + 1;
                                                let num_str = format!(
                                                    "{:>width$}",
                                                    line_num,
                                                    width = max_digits
                                                );

                                                // Get syntax highlights
                                                let syntax = self
                                                    .preview_line_highlights
                                                    .get(&ix)
                                                    .cloned()
                                                    .unwrap_or_default();

                                                // Get query highlights
                                                let query_highlights = if !query.is_empty() {
                                                    Self::find_query_highlights(
                                                        line_content,
                                                        &query,
                                                    )
                                                } else {
                                                    Vec::new()
                                                };

                                                let mut combined: Vec<(
                                                    std::ops::Range<usize>,
                                                    gpui::HighlightStyle,
                                                )> = syntax;
                                                combined.extend(query_highlights);

                                                // Flatten logic
                                                let mut points: Vec<(usize, bool, usize)> =
                                                    Vec::new();
                                                for (i, (range, _)) in combined.iter().enumerate() {
                                                    points.push((range.start, true, i));
                                                    points.push((range.end, false, i));
                                                }
                                                points.sort_by(|a, b| {
                                                    a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
                                                });

                                                let mut flattened: Vec<(
                                                    std::ops::Range<usize>,
                                                    gpui::HighlightStyle,
                                                )> = Vec::new();
                                                let mut active: Vec<usize> = Vec::new();
                                                let mut prev = 0;

                                                for (pos, is_start, idx) in points {
                                                    if pos > prev {
                                                        if line_content.is_char_boundary(prev)
                                                            && line_content.is_char_boundary(pos)
                                                        {
                                                            if let Some(&top) = active.last() {
                                                                flattened.push((
                                                                    prev..pos,
                                                                    combined[top].1.clone(),
                                                                ));
                                                            }
                                                        }
                                                    }
                                                    if is_start {
                                                        active.push(idx);
                                                    } else {
                                                        if let Some(p) =
                                                            active.iter().rposition(|&x| x == idx)
                                                        {
                                                            active.remove(p);
                                                        }
                                                    }
                                                    prev = pos;
                                                }

                                                let styled = StyledText::new(line_content.clone())
                                                    .with_highlights(flattened);

                                                div()
                                                    .flex()
                                                    .items_start()
                                                    .gap_4()
                                                    .child(
                                                        div()
                                                            .flex_shrink_0()
                                                            .text_color(rgb(theme::MUTED))
                                                            .child(num_str),
                                                    )
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .whitespace_normal()
                                                            .child(styled),
                                                    )
                                            }),
                                    ),
                            ),
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
