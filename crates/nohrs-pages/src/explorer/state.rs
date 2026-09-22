use nohrs_core::config;
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_services::fs::trash::Ledger as TrashLedger;
use nohrs_services::search::{SearchScope, SearchService};
use nohrs_services::syntax::SyntaxService;
use nohrs_ui::components::file_list::FileListDelegate;

use gpui::{AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Window, px, size};
use gpui_component::VirtualListScrollHandle;
use gpui_component::input::InputState;
use gpui_component::list::ListState;
use gpui_component::resizable::ResizableState;
use std::{rc::Rc, sync::Arc, time::Instant};

use super::entries;
use super::types::*;
use super::view::preview::editor::PreviewEditor;

/// State for the file explorer page: the current directory listing, navigation
/// history, sorting and filtering, search, preview, and view layout.
pub struct ExplorerPane {
    /// Absolute path of the currently displayed directory.
    pub cwd: String,
    /// Navigation history of visited directories for back/forward.
    pub history: Vec<String>,
    /// Index of the current position within `history`.
    pub history_index: usize,
    /// All entries read from `cwd`, before filtering.
    pub entries: Vec<FileEntryDto>,
    /// Entries currently shown after applying hidden/search filters and sorting.
    pub filtered_entries: Vec<FileEntryDto>,
    // Whether the current directory has been loaded. Tracked explicitly rather
    // than via `entries.is_empty()` so an empty (or failed-to-read) directory is
    // not reloaded on every render.
    /// Whether `cwd` has been loaded into `entries` at least once.
    pub loaded: bool,
    /// Column the listing is sorted by.
    pub sort_key: SortKey,
    /// Whether the sort is ascending.
    pub sort_asc: bool,
    // Whether dotfiles are listed; driven by `ui.show_hidden` (config.md §5).
    /// Whether dotfiles are listed; driven by `ui.show_hidden`.
    pub show_hidden: bool,
    // Identifier of the active icon pack; driven by `ui.icon_pack`.
    /// Identifier of the active icon pack; driven by `ui.icon_pack`.
    pub icon_pack: String,
    /// Current name-filter query for the in-directory listing.
    pub search_query: String,
    /// Whether the search bar is visible.
    pub search_visible: bool,
    /// Input state backing the search field.
    pub search_input: Entity<InputState>,
    /// State of the resizable split between listing and preview.
    pub resizable: Entity<ResizableState>,
    /// The listing's virtualized list entity, once initialized.
    pub list: Option<Entity<ListState<FileListDelegate>>>,
    /// GPUI subscriptions kept alive for the lifetime of the page.
    pub subs: Vec<gpui::Subscription>,
    /// Path of the file currently shown in the preview pane.
    pub preview_path: Option<String>,
    /// Text content of the previewed file, when it is textual.
    pub preview_text: Option<String>,
    /// Paths of all currently selected entries.
    ///
    /// Held as paths rather than row indices: an index-addressed selection stops
    /// meaning anything the moment the visible rows are rebuilt, so sorting or
    /// filtering had to clear it to avoid acting on whichever entries had moved
    /// into those slots. Rows are matched back to it when the listing renders.
    pub selection: std::collections::HashSet<String>,
    /// Path a Shift range selection extends from, or `None` when nothing is
    /// anchored.
    pub selection_anchor: Option<String>,
    /// The active/primary entry that drives the preview and keyboard navigation.
    pub active_path: Option<String>,
    /// Scroll handle for the virtualized listing.
    pub virtual_scroll_handle: VirtualListScrollHandle,
    /// Per-row sizes for the virtualized listing.
    pub item_sizes: Rc<Vec<gpui::Size<gpui::Pixels>>>,
    // Column widths (resizable)
    /// Width of the name column.
    pub col_name_width: f32,
    /// Width of the type column.
    pub col_type_width: f32,
    /// Width of the size column.
    pub col_size_width: f32,
    /// Width of the modified-time column.
    pub col_modified_width: f32,
    /// Width of the action column.
    pub col_action_width: f32,
    // Resize state
    /// The column currently being resized by a drag, if any.
    pub resizing_column: Option<ResizingColumn>,
    /// Focus handle for the explorer page.
    pub focus_handle: FocusHandle,
    /// Whether focus should be requested on the next render.
    pub focus_requested: bool,
    /// Information about the most recent row click, used for double-click detection.
    pub last_click_info: Option<LastClickInfo>,
    /// Whether the listing is shown as a list or a grid.
    pub view_mode: ViewMode,
    /// Whether the left quick-access sidebar is shown (toggled with `Cmd/Ctrl+B`).
    pub sidebar_visible: bool,

    // Search
    /// The full-text search service, when available.
    pub search_service: Option<Arc<SearchService>>,
    /// The directory scope for full-text search.
    pub search_scope: SearchScope,
    /// The kind of items full-text search targets.
    pub search_type: SearchType,
    /// Whether full-text search is case-sensitive.
    pub match_case: bool,
    /// Whether full-text search matches whole words only.
    pub match_whole_word: bool,
    /// Whether the full-text search query is treated as a regular expression.
    pub use_regex: bool,
    /// Results of the active full-text search, when one is displayed.
    pub search_results: Option<Vec<SearchFileResult>>,
    /// Whether a full-text search is currently running.
    pub is_performing_search: bool,
    // Monotonic counter incremented on every search request; used to discard
    // results from a stale in-flight search that completes after a newer one.
    /// Monotonic counter used to discard results from stale in-flight searches.
    pub search_generation: u64,
    /// Paths of search-result files whose match snippets are expanded.
    pub expanded_search_files: std::collections::HashSet<String>,
    // Syntax
    /// Service providing syntax highlighting for previews.
    pub syntax_service: Arc<SyntaxService>,
    // Preview State
    /// Path of the image currently shown in the preview pane.
    pub preview_image_path: Option<String>,
    /// Message shown in the preview pane when a file cannot be previewed.
    pub preview_message: Option<String>,
    /// The editor entity backing a text preview.
    pub preview_editor: Option<Entity<PreviewEditor>>,
    // Transient message shown in the footer status bar.
    /// Transient message shown in the footer status bar.
    pub status_message: Option<StatusMessage>,

    // File operations (`docs/explorer-essentials.md` §1)
    /// Where a trashed item's origin is recorded, on platforms with no OS trash
    /// index of their own — or why it cannot be, which has to stop a delete
    /// rather than let one through unrecorded.
    pub(crate) trash_ledger: TrashLedger,
    /// In-progress inline rename of a listing row, if any.
    pub(crate) renaming: Option<super::file_ops::RenameState>,
    /// In-progress paste whose name conflicts are being resolved via the dialog.
    pub(crate) paste_plan: Option<super::file_ops::PastePlan>,
}

impl Focusable for ExplorerPane {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PaneEvent> for ExplorerPane {}

impl crate::pane_group::PaneItem for ExplorerPane {
    fn tab_title(&self, _cx: &gpui::App) -> String {
        std::path::Path::new(&self.cwd)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| self.cwd.clone())
    }
}

impl ExplorerPane {
    /// Builds a self-contained pane, creating the window-bound sub-entities
    /// (listing/preview resizable, search input) and focus handle it owns. Used
    /// by the split-view container, which may hold several independent panes.
    pub fn build(
        search_service: Option<Arc<SearchService>>,
        trash_ledger: TrashLedger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let resizable = cx.new(|_| ResizableState::default());
        let search_input = cx.new(|cx| InputState::new(window, cx));
        Self::new(
            resizable,
            search_input,
            search_service,
            trash_ledger,
            cx.focus_handle(),
        )
    }

    /// Creates a new explorer pane rooted at the current working directory,
    /// wired to the given resizable, search input, and optional search service.
    pub fn new(
        resizable: Entity<ResizableState>,
        search_input: Entity<InputState>,
        search_service: Option<Arc<SearchService>>,
        trash_ledger: TrashLedger,
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
            loaded: false,
            sort_key: SortKey::Name,
            sort_asc: true,
            show_hidden: false,
            icon_pack: "default".to_string(),
            search_query: String::new(),
            search_visible: false,
            search_input,
            resizable,
            list: None,
            subs: Vec::new(),
            preview_path: None,
            preview_text: None,
            selection: std::collections::HashSet::new(),
            selection_anchor: None,
            active_path: None,
            virtual_scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            col_name_width: config::COL_NAME_WIDTH,
            col_type_width: config::COL_TYPE_WIDTH,
            col_size_width: config::COL_SIZE_WIDTH,
            col_modified_width: config::COL_MODIFIED_WIDTH,
            col_action_width: config::COL_ACTION_WIDTH,
            resizing_column: None,
            focus_handle,
            focus_requested: false,
            last_click_info: None,
            view_mode: ViewMode::List,
            // Hidden by default; the root pane is revealed by `ExplorerPage::new`,
            // split-created panes stay collapsed (issue #164, §2).
            sidebar_visible: false,

            // Search
            search_service,
            search_scope: SearchScope::Home,
            search_type: SearchType::All,
            match_case: false,
            match_whole_word: false,
            use_regex: false,
            search_results: None,
            is_performing_search: false,
            search_generation: 0,
            expanded_search_files: std::collections::HashSet::new(),
            syntax_service: Arc::new(SyntaxService::new()),
            preview_editor: None,
            preview_image_path: None,
            preview_message: None,
            status_message: None,
            trash_ledger,
            renaming: None,
            paste_plan: None,
        }
    }

    pub(crate) fn set_status(&mut self, level: StatusLevel, text: impl Into<String>) {
        self.status_message = Some(StatusMessage {
            text: text.into(),
            level,
        });
    }

    pub(crate) fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Toggles the left quick-access sidebar (issue #164, §2). Mirrors the
    /// `toggle_search` open/close pattern.
    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    /// Returns the current status text and what it is reporting, for rendering
    /// in the footer status bar.
    pub fn status_for_footer(&self) -> Option<(String, StatusLevel)> {
        self.status_message
            .as_ref()
            .map(|status| (status.text.clone(), status.level))
    }

    pub(crate) fn set_search_scope(&mut self, scope: SearchScope, cx: &mut Context<Self>) {
        if self.search_scope != scope {
            self.search_scope = scope;
            cx.notify();
        }
    }

    pub(crate) fn toggle_match_case(&mut self, cx: &mut Context<Self>) {
        self.match_case = !self.match_case;
        cx.notify();
    }

    pub(crate) fn toggle_match_whole_word(&mut self, cx: &mut Context<Self>) {
        self.match_whole_word = !self.match_whole_word;
        cx.notify();
    }

    pub(crate) fn toggle_use_regex(&mut self, cx: &mut Context<Self>) {
        self.use_regex = !self.use_regex;
        cx.notify();
    }

    pub(crate) fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        if self.view_mode != mode {
            self.view_mode = mode;
            cx.notify();
        }
    }

    pub(crate) fn record_click(&mut self, row: usize, click_count: usize) {
        self.last_click_info = Some(LastClickInfo {
            row,
            timestamp: Instant::now(),
            click_count,
        });
    }

    /// Path of the entry shown at row `ix`, if there is one.
    fn path_at(&self, ix: usize) -> Option<&str> {
        self.filtered_entries
            .get(ix)
            .map(|entry| entry.path.as_str())
    }

    /// Row currently showing `path`, if it is visible.
    fn row_of(&self, path: &str) -> Option<usize> {
        self.filtered_entries
            .iter()
            .position(|entry| entry.path == path)
    }

    /// Returns whether the row at `ix` (an index into `filtered_entries`) is
    /// part of the current selection.
    pub fn is_selected(&self, ix: usize) -> bool {
        self.path_at(ix)
            .is_some_and(|path| self.selection.contains(path))
    }

    /// Row of the active/primary entry, if it is visible.
    pub fn active_row(&self) -> Option<usize> {
        self.active_path
            .as_deref()
            .and_then(|path| self.row_of(path))
    }

    /// Replaces the selection with the single row `ix`, making it both the
    /// anchor and the active row (a plain click or arrow-key move).
    pub(crate) fn select_single(&mut self, ix: usize) {
        let Some(path) = self.path_at(ix).map(str::to_string) else {
            return;
        };
        self.selection.clear();
        self.selection.insert(path.clone());
        self.selection_anchor = Some(path.clone());
        self.active_path = Some(path);
    }

    /// Toggles row `ix` in the selection (Cmd/Ctrl+click) and re-anchors there.
    pub(crate) fn toggle_select(&mut self, ix: usize) {
        let Some(path) = self.path_at(ix).map(str::to_string) else {
            return;
        };
        if !self.selection.remove(&path) {
            self.selection.insert(path.clone());
        }
        self.selection_anchor = Some(path.clone());
        self.active_path = Some(path);
    }

    /// Selects the contiguous range between the current anchor and `ix`
    /// (Shift+click / Shift+arrow). Falls back to a single selection when there
    /// is no anchor yet, or when the anchored entry is no longer visible.
    pub(crate) fn select_range_to(&mut self, ix: usize) {
        let anchor = match self
            .selection_anchor
            .as_deref()
            .and_then(|a| self.row_of(a))
        {
            Some(anchor) => anchor,
            None => {
                self.select_single(ix);
                return;
            }
        };
        let (low, high) = if anchor <= ix {
            (anchor, ix)
        } else {
            (ix, anchor)
        };
        let range = (low..=high)
            .filter_map(|row| self.path_at(row).map(str::to_string))
            .collect();
        self.selection = range;
        self.active_path = self.path_at(ix).map(str::to_string);
    }

    /// Selects every visible row.
    pub(crate) fn select_all(&mut self) {
        self.selection = self
            .filtered_entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        self.selection_anchor = self.path_at(0).map(str::to_string);
        self.active_path = self
            .filtered_entries
            .len()
            .checked_sub(1)
            .and_then(|last| self.path_at(last))
            .map(str::to_string);
    }

    /// Clears the selection, anchor, and active row.
    pub(crate) fn clear_selection(&mut self) {
        self.selection.clear();
        self.selection_anchor = None;
        self.active_path = None;
    }

    // Drops selected, anchored, and active paths that the visible rows no longer
    // hold, so a hidden or deleted entry cannot be acted on and cannot come back
    // selected if something later takes its path.
    pub(crate) fn prune_selection(&mut self) {
        let visible: std::collections::HashSet<&str> = self
            .filtered_entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        self.selection
            .retain(|path| visible.contains(path.as_str()));
        let still_visible =
            |path: &Option<String>| path.as_deref().is_some_and(|path| visible.contains(path));
        if !still_visible(&self.selection_anchor) {
            self.selection_anchor = None;
        }
        if !still_visible(&self.active_path) {
            self.active_path = None;
        }
    }

    /// Moves the active row by `delta` rows, clamped to the visible range. With
    /// `extend` (Shift held) the selection grows from the anchor; otherwise the
    /// moved-to row becomes the sole selection. Selecting from an empty state
    /// lands on the first row.
    pub(crate) fn move_active(&mut self, delta: isize, extend: bool) {
        let len = self.filtered_entries.len();
        if len == 0 {
            return;
        }
        let next = match self.active_row() {
            Some(current) => (current as isize + delta).clamp(0, len as isize - 1) as usize,
            None => 0,
        };
        if extend {
            self.select_range_to(next);
        } else {
            self.select_single(next);
        }
    }

    /// Paths of the currently selected rows, in row order. Used by file
    /// operations and drag-and-drop.
    pub fn selected_paths(&self) -> Vec<String> {
        self.filtered_entries
            .iter()
            .filter(|entry| self.selection.contains(&entry.path))
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub(crate) fn update_item_sizes(&mut self) {
        let total_width = self.total_table_width();

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
                                .map(|r| r.matches.len().min(config::MAX_SNIPPETS))
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                let total_height =
                    config::BASE_ROW_HEIGHT + (snippet_count as f32 * config::SNIPPET_ROW_HEIGHT);
                size(px(total_width), px(total_height))
            })
            .collect();
        self.item_sizes = Rc::new(sizes);
    }

    pub(crate) fn total_table_width(&self) -> f32 {
        self.col_name_width
            + self.col_type_width
            + self.col_size_width
            + self.col_modified_width
            + self.col_action_width
            + config::TABLE_HORIZONTAL_PADDING
    }

    pub(crate) fn apply_filter(&mut self) {
        // The selection is addressed by path, so rebuilding the visible rows
        // keeps it: sorting or filtering no longer throws away what the user
        // picked. `prune_selection` below drops whatever is no longer visible.

        // When explicit search results are displayed, `filtered_entries` is owned
        // by the search path; only refresh row sizes here.
        if self.search_results.is_some() {
            self.prune_selection();
            self.update_item_sizes();
            return;
        }

        let show_hidden = self.show_hidden;
        let visible = self
            .entries
            .iter()
            .filter(move |entry| show_hidden || !is_hidden(&entry.name));

        if self.search_query.is_empty() {
            self.filtered_entries = visible.cloned().collect();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_entries = visible
                .filter(|e| e.name.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }

        entries::sort_entries(&mut self.filtered_entries, self.sort_key, self.sort_asc);
        self.prune_selection();
        self.update_item_sizes();
    }

    /// Apply the `[ui]` config section to this open view, re-sorting and
    /// re-filtering when anything changed. Used both at startup and on hot
    /// reload (config.md §5). Note: `icon_pack` is stored for the row renderer to
    /// consult; there is no icon cache to invalidate yet.
    pub fn apply_config_ui(&mut self, ui: &config::Ui, cx: &mut Context<Self>) {
        let sort_key = sort_key_from_config(ui.default_sort);
        let mut changed = false;
        if self.sort_key != sort_key {
            self.sort_key = sort_key;
            self.sort_asc = true;
            changed = true;
        }
        if self.show_hidden != ui.show_hidden {
            self.show_hidden = ui.show_hidden;
            changed = true;
        }
        if self.icon_pack != ui.icon_pack {
            self.icon_pack = ui.icon_pack.clone();
            changed = true;
        }
        if changed {
            entries::sort_entries(&mut self.entries, self.sort_key, self.sort_asc);
            self.apply_filter();
            cx.notify();
        }
    }

    pub(crate) fn set_sort_key(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = true;
        }
        entries::sort_entries(&mut self.entries, self.sort_key, self.sort_asc);
        self.apply_filter();
    }
}

fn sort_key_from_config(order: config::SortOrder) -> SortKey {
    match order {
        config::SortOrder::Name => SortKey::Name,
        config::SortOrder::Modified => SortKey::Modified,
        config::SortOrder::Size => SortKey::Size,
        config::SortOrder::Kind => SortKey::Type,
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_hidden_matches_dotfiles_only() {
        assert!(is_hidden(".gitignore"));
        assert!(is_hidden("..git"));
        assert!(!is_hidden("visible.txt"));
        assert!(!is_hidden(""));
    }

    #[test]
    fn sort_key_from_config_maps_every_order() {
        assert!(sort_key_from_config(config::SortOrder::Name) == SortKey::Name);
        assert!(sort_key_from_config(config::SortOrder::Modified) == SortKey::Modified);
        assert!(sort_key_from_config(config::SortOrder::Size) == SortKey::Size);
        assert!(sort_key_from_config(config::SortOrder::Kind) == SortKey::Type);
    }
}
