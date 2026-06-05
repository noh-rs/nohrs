use nohrs_core::config;
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_services::search::{SearchScope, SearchService};
use nohrs_services::syntax::SyntaxService;
use nohrs_ui::components::file_list::FileListDelegate;

use gpui::{Context, Entity, FocusHandle, Focusable, px, size};
use gpui_component::VirtualListScrollHandle;
use gpui_component::input::InputState;
use gpui_component::list::List;
use gpui_component::resizable::ResizableState;
use std::{rc::Rc, sync::Arc, time::Instant};

use super::entries;
use super::types::*;
use super::view::preview::editor::PreviewEditor;

/// State for the file explorer page: the current directory listing, navigation
/// history, sorting and filtering, search, preview, and view layout.
pub struct ExplorerPage {
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
    pub list: Option<Entity<List<FileListDelegate>>>,
    /// GPUI subscriptions kept alive for the lifetime of the page.
    pub subs: Vec<gpui::Subscription>,
    /// Path of the file currently shown in the preview pane.
    pub preview_path: Option<String>,
    /// Text content of the previewed file, when it is textual.
    pub preview_text: Option<String>,
    /// Index of the selected row within `filtered_entries`.
    pub selected_index: Option<usize>,
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
}

impl Focusable for ExplorerPage {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ExplorerPage {
    /// Creates a new explorer page rooted at the current working directory,
    /// wired to the given resizable, search input, and optional search service.
    pub fn new(
        resizable: Entity<ResizableState>,
        search_input: Entity<InputState>,
        search_service: Option<Arc<SearchService>>,
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
            selected_index: None,
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

    /// Returns the current status text and whether it represents an error, for
    /// rendering in the footer status bar.
    pub fn status_for_footer(&self) -> Option<(String, bool)> {
        self.status_message
            .as_ref()
            .map(|status| (status.text.clone(), status.level == StatusLevel::Error))
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
        // When explicit search results are displayed, `filtered_entries` is owned
        // by the search path; only refresh row sizes here.
        if self.search_results.is_some() {
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
