use nohrs_core::config;
use nohrs_services::fs::listing::{FileEntryDto, ListParams, list_dir_sync};

use gpui::{Context, Window};

use super::ExplorerPane;
use super::entries;
use super::types::{PaneEvent, StatusLevel};

impl ExplorerPane {
    pub(crate) fn ensure_loaded(&mut self) {
        if !self.loaded {
            self.reload();
        }
    }

    pub(crate) fn reload(&mut self) {
        // Mark as loaded regardless of outcome so an empty or unreadable
        // directory is not re-read on every subsequent render.
        self.loaded = true;
        match list_dir_sync(ListParams {
            path: &self.cwd,
            limit: config::DIR_LISTING_LIMIT,
            cursor: None,
        }) {
            Ok(res) => {
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
                self.clear_status();
            }
            Err(e) => {
                tracing::error!("Failed to list directory '{}': {}", self.cwd, e);
                self.entries = Vec::new();
                self.filtered_entries = Vec::new();
                self.update_item_sizes();
                self.set_status(
                    StatusLevel::Error,
                    format!("Cannot open '{}': {}", self.cwd, e),
                );
            }
        }
    }

    pub(crate) fn change_dir(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if path == self.cwd {
            return;
        }
        self.close_search(window, cx);
        self.push_history(path.clone());
        self.cwd = path;
        self.entries.clear();
        self.reload();
        cx.emit(PaneEvent::Navigated(self.cwd.clone()));
        cx.notify();
    }

    // Records a forward navigation in the back/forward history, seeding the
    // starting directory on first use and dropping any forward entries.
    fn push_history(&mut self, path: String) {
        if self.history.is_empty() {
            self.history.push(self.cwd.clone());
            self.history_index = 0;
        }
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        self.history.push(path);
        self.history_index += 1;
    }

    /// Mirrors a path requested by a sibling pane while `synced_panes` is on
    /// (§3.2). Unlike [`change_dir`], it does not re-emit a navigation event, so
    /// the originating pane is not driven back into an update loop.
    pub(crate) fn navigate_to_synced(&mut self, path: String, cx: &mut Context<Self>) {
        if path == self.cwd {
            return;
        }
        // Clear search state so mirrored navigation doesn't leave a stale filter
        // or full-text results from the previous directory visible. This mirrors
        // the `close_search` reset on `change_dir`, minus the window-bound editor
        // sync (the subscription that drives sync has no `Window`).
        self.search_visible = false;
        self.search_results = None;
        self.search_query.clear();
        self.push_history(path.clone());
        self.cwd = path;
        self.entries.clear();
        self.reload();
        cx.notify();
    }

    pub(crate) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index > 0 {
            self.history_index -= 1;
            if let Some(p) = self.history.get(self.history_index).cloned() {
                self.cwd = p;
                self.entries.clear();
                self.close_search(window, cx);
                self.reload();
                cx.emit(PaneEvent::Navigated(self.cwd.clone()));
                cx.notify();
            }
        }
    }

    pub(crate) fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            if let Some(p) = self.history.get(self.history_index).cloned() {
                self.cwd = p;
                self.entries.clear();
                self.close_search(window, cx);
                self.reload();
                cx.emit(PaneEvent::Navigated(self.cwd.clone()));
                cx.notify();
            }
        }
    }

    pub(crate) fn activate_entry(
        &mut self,
        item: FileEntryDto,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if item.kind == "dir" {
            self.change_dir(item.path, window, cx);
        } else {
            self.open_preview(item.path, window, cx);
        }
    }
}
