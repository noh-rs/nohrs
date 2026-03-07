use crate::core::types::SearchQuery;
use crate::services::fs::listing::{list_dir_sync, FileEntryDto, ListParams};
use crate::services::search::{SearchProvider, SearchScope};
use crate::ui::components::file_list::FileListDelegate;

use crate::services::syntax::SyntaxService;
use gpui::{
    px, size, AnyElement, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Window,
};
use gpui_component::input::InputState;
use gpui_component::list::List;
use gpui_component::resizable::ResizableState;
use gpui_component::VirtualListScrollHandle;
use std::collections::HashMap;
use std::{rc::Rc, sync::Arc};
mod entries;
mod event;
mod preview_open;
mod search;
#[cfg(test)]
mod tests;
mod types;
pub mod view;
use types::*;
use view::preview::editor::PreviewEditor;

#[allow(dead_code)]
pub struct ExplorerPage {
    pub(super) cwd: String,
    pub(super) history: Vec<String>,
    pub(super) history_index: usize,
    pub(super) entries: Vec<FileEntryDto>,
    pub(super) filtered_entries: Vec<FileEntryDto>,
    pub(super) sort_key: SortKey,
    pub(super) sort_asc: bool,
    pub(super) search_query: String,
    pub(super) search_visible: bool,
    pub(super) search_input: Entity<InputState>,
    pub(super) resizable: Entity<ResizableState>,
    pub(super) list: Option<Entity<List<FileListDelegate>>>,
    pub(super) subs: Vec<gpui::Subscription>,
    pub(super) preview_path: Option<String>,
    pub(super) preview_text: Option<String>,
    pub(super) selected_index: Option<usize>,
    pub(super) virtual_scroll_handle: VirtualListScrollHandle,
    pub(super) item_sizes: Rc<Vec<gpui::Size<gpui::Pixels>>>,
    pub(super) col_name_width: f32,
    pub(super) col_type_width: f32,
    pub(super) col_size_width: f32,
    pub(super) col_modified_width: f32,
    pub(super) col_action_width: f32,
    pub(super) resizing_column: Option<ResizingColumn>,
    pub(super) focus_handle: FocusHandle,
    pub(super) focus_requested: bool,
    pub(super) last_click_info: Option<LastClickInfo>,
    pub(super) view_mode: ViewMode,
    pub(super) search_service: Arc<dyn SearchProvider>,
    pub(super) search_scope: SearchScope,
    pub(super) search_type: SearchType,
    pub(super) match_case: bool,
    pub(super) match_whole_word: bool,
    pub(super) use_regex: bool,
    pub(super) search_results: Option<Vec<SearchFileResult>>,
    /// O(1) パス → SearchFileResult インデックス参照用マップ (4.2.5)
    pub(super) search_results_map: HashMap<String, usize>,
    pub(super) is_performing_search: bool,
    pub(super) expanded_search_files: std::collections::HashSet<String>,
    pub(super) syntax_service: Arc<SyntaxService>,
    pub(super) preview_image_path: Option<String>,
    pub(super) preview_message: Option<String>,
    pub(super) preview_editor: Option<Entity<PreviewEditor>>,
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExplorerPage {
    pub fn new(
        resizable: Entity<ResizableState>,
        search_input: Entity<InputState>,
        search_service: Arc<dyn SearchProvider>,
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
            search_service,
            search_scope: SearchScope::Home,
            search_type: SearchType::All,
            match_case: false,
            match_whole_word: false,
            use_regex: false,
            search_results: None,
            search_results_map: HashMap::new(),
            is_performing_search: false,
            expanded_search_files: std::collections::HashSet::new(),
            syntax_service: Arc::new(SyntaxService::new()),
            preview_editor: None,
            preview_image_path: None,
            preview_message: None,
        }
    }

    /// 検索を非同期で実行 (4.1.1: UI スレッドブロックを解消)
    fn trigger_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_query.is_empty() {
            self.search_results = None;
            self.search_results_map.clear();
            self.apply_filter();
            cx.notify();
            return;
        }

        self.is_performing_search = true;
        cx.notify();

        // SearchQuery を構築して全オプションを渡す (4.1.2)
        let query = SearchQuery {
            query: self.search_query.clone(),
            search_type: self.search_type,
            match_case: self.match_case,
            match_whole_word: self.match_whole_word,
            use_regex: self.use_regex,
        };
        let scope = self.search_scope;
        let service = self.search_service.clone();

        // background_executor で非同期実行 (4.1.1)
        cx.spawn_in(window, async move |this: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncWindowContext| {
            let results = cx
                .background_executor()
                .spawn(async move { service.search_blocking(&query, scope) })
                .await;

            this.update(cx, |this: &mut Self, cx: &mut gpui::Context<Self>| {
                match results {
                    Ok(res) => {
                        let grouped = search::group_results(res);
                        let entries = search::results_to_entries(&grouped);
                        this.rebuild_search_results_map(&grouped);
                        this.filtered_entries = entries;
                        this.search_results = Some(grouped);
                    }
                    Err(e) => {
                        tracing::error!("Search failed: {}", e);
                        this.search_results = Some(Vec::new());
                        this.search_results_map.clear();
                        this.filtered_entries = Vec::new();
                    }
                }
                this.is_performing_search = false;
                this.update_item_sizes();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// search_results から HashMap を構築 (4.2.5)
    fn rebuild_search_results_map(&mut self, results: &[SearchFileResult]) {
        self.search_results_map.clear();
        for (i, r) in results.iter().enumerate() {
            self.search_results_map.insert(r.path.clone(), i);
        }
    }

    /// パスから SearchFileResult を O(1) で取得
    pub(super) fn get_search_result(&self, path: &str) -> Option<&SearchFileResult> {
        let results = self.search_results.as_ref()?;
        let &idx = self.search_results_map.get(path)?;
        results.get(idx)
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

    fn update_item_sizes(&mut self) {
        let total_width = self.total_table_width();
        let base_row_height = 32.0;
        let snippet_row_height = 24.0;
        let max_snippets = 10;

        let sizes = self
            .filtered_entries
            .iter()
            .map(|entry| {
                let is_expanded = self.expanded_search_files.contains(&entry.path);
                let snippet_count = if is_expanded {
                    // O(1) HashMap ルックアップ (4.2.5)
                    self.get_search_result(&entry.path)
                        .map(|r| r.matches.len().min(max_snippets))
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
        if self.search_results.is_some() {
            self.update_item_sizes();
            return;
        }
        self.filtered_entries = self.entries.clone();
        entries::sort_entries(&mut self.filtered_entries, self.sort_key, self.sort_asc);
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

    fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        if self.view_mode != mode {
            self.view_mode = mode;
            cx.notify();
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
        self.search_results_map.clear();
        self.search_query.clear();
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

    /// 検索バーの入力値が変わったときに呼ばれるハンドラ (4.2.2, 4.3.6)
    fn on_search_input_changed(&mut self, new_text: String) {
        if new_text != self.search_query {
            self.search_query = new_text;
            self.search_results = None;
            self.search_results_map.clear();
            self.apply_filter();
        }
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
