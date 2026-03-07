// Phase 3: ExplorerPage エンティティテスト
// Phase 4: 検索 UI 結合テスト

mod navigation_test;
mod search_flow_test;
mod search_scope_test;
mod search_state_test;
mod sort_test;

use super::*;
use crate::core::types::SearchQuery;
use crate::services::search::{SearchProvider, SearchResult, SearchScope};
use anyhow::Result;
use gpui::{AppContext, TestAppContext, VisualContext, VisualTestContext};
use gpui_component::input::InputState;
use gpui_component::resizable::ResizableState;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// テスト用モック検索プロバイダー
pub struct MockSearchProvider {
    pub results: HashMap<String, Vec<SearchResult>>,
    pub call_count: Arc<AtomicUsize>,
    pub error_mode: bool,
}

impl MockSearchProvider {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
            call_count: Arc::new(AtomicUsize::new(0)),
            error_mode: false,
        }
    }

    pub fn with_results(mut self, query: &str, results: Vec<SearchResult>) -> Self {
        self.results.insert(query.to_string(), results);
        self
    }

    pub fn with_error_mode(mut self) -> Self {
        self.error_mode = true;
        self
    }
}

impl SearchProvider for MockSearchProvider {
    fn search_blocking(
        &self,
        query: &SearchQuery,
        _scope: SearchScope,
    ) -> Result<Vec<SearchResult>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.error_mode {
            return Err(anyhow::anyhow!("Mock search error"));
        }
        Ok(self.results.get(&query.query).cloned().unwrap_or_default())
    }
}

/// テスト用テストデータ生成
pub mod test_data {
    use crate::services::search::SearchResult;
    use std::path::PathBuf;

    pub fn single_match() -> Vec<SearchResult> {
        vec![SearchResult {
            path: PathBuf::from("/tmp/test/hello.rs"),
            line_number: 5,
            line_content: "fn hello() { println!(\"hello\"); }".to_string(),
            match_start: 3,
            match_end: 8,
        }]
    }

    pub fn multi_match_same_file() -> Vec<SearchResult> {
        vec![
            SearchResult {
                path: PathBuf::from("/tmp/test/lib.rs"),
                line_number: 1,
                line_content: "use std::collections::HashMap;".to_string(),
                match_start: 0,
                match_end: 0,
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/lib.rs"),
                line_number: 10,
                line_content: "let map = HashMap::new();".to_string(),
                match_start: 0,
                match_end: 0,
            },
        ]
    }

    pub fn multi_file_matches() -> Vec<SearchResult> {
        vec![
            SearchResult {
                path: PathBuf::from("/tmp/test/a.rs"),
                line_number: 3,
                line_content: "fn search() {}".to_string(),
                match_start: 3,
                match_end: 9,
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/b.rs"),
                line_number: 7,
                line_content: "fn search_inner() {}".to_string(),
                match_start: 3,
                match_end: 9,
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/c/d.rs"),
                line_number: 1,
                line_content: "mod search;".to_string(),
                match_start: 4,
                match_end: 10,
            },
        ]
    }
}

/// テスト用 ExplorerPage を生成するヘルパー (ウィンドウ付き)
/// Root でラップすることで gpui_component の各コンポーネントが正常に動作する
fn build_explorer<'a>(
    cx: &'a mut TestAppContext,
    cwd: &str,
    mock: MockSearchProvider,
) -> (Entity<ExplorerPage>, &'a mut VisualTestContext) {
    use gpui_component::Root;

    // gpui_component のグローバル状態（Theme 等）を初期化
    cx.update(|cx| {
        if !cx.has_global::<gpui_component::theme::Theme>() {
            gpui_component::init(cx);
        }
    });
    let cwd = cwd.to_string();
    let search_service: Arc<dyn SearchProvider> = Arc::new(mock);

    // ExplorerPage を先に作成して Entity を保持し、Root でラップ
    let page: std::cell::Cell<Option<Entity<ExplorerPage>>> = std::cell::Cell::new(None);
    let page_ref = &page;
    let window = cx.add_window(|window, cx| {
        let explorer = cx.new(|cx| {
            let resizable = ResizableState::new(cx);
            let search_input = cx.new(|cx| InputState::new(window, cx));
            let focus_handle = cx.focus_handle();
            let mut p = ExplorerPage::new(resizable, search_input, search_service, focus_handle);
            p.cwd = cwd;
            p
        });
        page_ref.set(Some(explorer.clone()));
        Root::new(explorer.into(), window, cx)
    });
    let entity = page.take().unwrap();
    let vcx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    vcx.run_until_parked();
    (entity, vcx)
}

/// テスト用 ExplorerPage（デフォルトモック）
fn build_explorer_default<'a>(
    cx: &'a mut TestAppContext,
    cwd: &str,
) -> (Entity<ExplorerPage>, &'a mut VisualTestContext) {
    build_explorer(cx, cwd, MockSearchProvider::new())
}

// --- Phase 3: エンティティテスト ---

#[gpui::test]
async fn test_initial_state(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.read_with(cx, |page, _| {
        assert_eq!(page.sort_key, SortKey::Name);
        assert!(page.sort_asc);
        assert_eq!(page.view_mode, ViewMode::List);
        assert!(!page.search_visible);
        assert!(page.search_query.is_empty());
        assert!(page.search_results.is_none());
        // 空ディレクトリなので entries も空
        assert!(page.entries.is_empty());
        assert!(page.history.is_empty());
    });
}

#[gpui::test]
async fn test_reload_loads_entries(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
    std::fs::write(tmp.path().join("world.txt"), "world").unwrap();

    // add_window_view の render で ensure_loaded → reload が自動実行される
    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.read_with(cx, |page, _| {
        assert_eq!(page.entries.len(), 2);
    });
}

#[gpui::test]
async fn test_sort_key_change(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("beta.txt"), "bb").unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.update(cx, |page, _cx| {
        page.reload();
        page.set_sort_key(SortKey::Name);
        assert_eq!(page.sort_key, SortKey::Name);
        assert!(!page.sort_asc); // Same key toggles direction

        page.set_sort_key(SortKey::Size);
        assert_eq!(page.sort_key, SortKey::Size);
        assert!(page.sort_asc); // New key resets to asc
    });
}

#[gpui::test]
async fn test_select_entry(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, _cx| {
        assert!(page.selected_index.is_none());
        page.selected_index = Some(3);
        assert_eq!(page.selected_index, Some(3));
    });
}

#[gpui::test]
async fn test_view_mode_toggle(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, cx| {
        assert_eq!(page.view_mode, ViewMode::List);
        page.set_view_mode(ViewMode::Grid, cx);
        assert_eq!(page.view_mode, ViewMode::Grid);
        page.set_view_mode(ViewMode::List, cx);
        assert_eq!(page.view_mode, ViewMode::List);
    });
}

#[gpui::test]
async fn test_column_resize(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, _cx| {
        let initial_width = page.col_name_width;
        page.start_column_resize(0, gpui::point(gpui::px(100.0), gpui::px(0.0)));
        assert!(page.resizing_column.is_some());

        page.update_column_resize(gpui::point(gpui::px(150.0), gpui::px(0.0)));
        assert!(page.col_name_width > initial_width);

        page.stop_column_resize();
        assert!(page.resizing_column.is_none());
    });
}

#[gpui::test]
async fn test_column_resize_min_width(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, _cx| {
        page.start_column_resize(0, gpui::point(gpui::px(400.0), gpui::px(0.0)));
        // Move far left to try to make width smaller than min
        page.update_column_resize(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        // Min width is 80.0
        assert!(page.col_name_width >= 80.0);
    });
}
