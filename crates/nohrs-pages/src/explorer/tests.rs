//! `TestAppContext`-based behavior tests for the explorer page: navigation and
//! history, search lifecycle, preview loading, sorting/filtering, column resize,
//! and status reporting. They also serve as the reference patterns documented in
//! `docs/testing.md` / `CONTRIBUTING.md`: build the view inside a test window,
//! mutate it through `window.update`, drive async work with the GPUI executor
//! timer + `run_until_parked`, and assert state with `window.read_with` — never
//! `smol::Timer` / `tokio` sleeps, which the GPUI scheduler does not track.

// Test fixtures write files directly; the synchronous-fs ban targets app code.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext, ClipboardItem, Entity, TestAppContext, WindowHandle, point, px};
use gpui_component::Root;
use gpui_component::input::InputState;
use gpui_component::resizable::ResizableState;
use nohrs_core::config;
use nohrs_services::fs::listing::FileEntryDto;
use nohrs_services::fs::ops::ConflictResolution;
use nohrs_store::{KvStore, RedbKvStore, StoreLogConfig};

use nohrs_core::config::SplitDirection;

use super::types::{SortKey, StatusLevel, ViewMode};
use super::{ExplorerPage, ExplorerPane};

/// Build a real `ExplorerPane` inside a test window. The sub-entities
/// (`ResizableState`, `InputState`) are window-bound, so the page is
/// constructed in the `add_window` build closure — the canonical pattern for
/// views whose dependencies need a `Window`.
fn new_explorer(cx: &mut TestAppContext) -> WindowHandle<ExplorerPane> {
    // gpui-component installs the `Theme` global and input/list subsystems its
    // widgets rely on; initialize it once before building any window.
    cx.update(gpui_component::init);
    cx.add_window(|window, cx| {
        let resizable = cx.new(|_| ResizableState::default());
        let search_input = cx.new(|cx| InputState::new(window, cx));
        ExplorerPane::new(resizable, search_input, None, cx.focus_handle())
    })
}

/// Build the split-view container (`ExplorerPage`), which owns its panes. No KV
/// store, so session save/restore is inert.
fn new_explorer_page(cx: &mut TestAppContext) -> WindowHandle<ExplorerPage> {
    cx.update(gpui_component::init);
    cx.add_window(|window, cx| {
        let resizable = cx.new(|_| ResizableState::default());
        ExplorerPage::new(resizable, None, None, false, window, cx)
    })
}

/// Build the container backed by a `store`, optionally restoring its session,
/// for the persistence round-trip tests.
fn new_explorer_page_with_store(
    cx: &mut TestAppContext,
    store: Arc<dyn KvStore>,
    restore_tabs: bool,
) -> WindowHandle<ExplorerPage> {
    cx.update(gpui_component::init);
    cx.add_window(|window, cx| {
        let resizable = cx.new(|_| ResizableState::default());
        ExplorerPage::new(resizable, None, Some(store), restore_tabs, window, cx)
    })
}

fn file(name: &str, kind: &str, size: u64) -> FileEntryDto {
    FileEntryDto {
        name: name.to_string(),
        path: format!("/tmp/{name}"),
        kind: kind.to_string(),
        size,
        modified: 0,
    }
}

#[gpui::test]
async fn explorer_starts_with_default_view_state(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .read_with(cx, |page, _cx| {
            assert!(page.sort_key == SortKey::Name);
            assert!(page.sort_asc);
            assert!(!page.show_hidden);
            assert!(page.view_mode == ViewMode::List);
        })
        .unwrap();
}

#[gpui::test]
async fn set_sort_key_toggles_direction_then_resets(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            // Re-selecting the active key flips the direction...
            page.set_sort_key(SortKey::Name);
            assert!(page.sort_key == SortKey::Name);
            assert!(!page.sort_asc);
            // ...and a new key resets to ascending.
            page.set_sort_key(SortKey::Size);
            assert!(page.sort_key == SortKey::Size);
            assert!(page.sort_asc);
        })
        .unwrap();
}

#[gpui::test]
async fn apply_config_ui_reflects_ui_section(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, cx| {
            let ui = config::Ui {
                default_sort: config::SortOrder::Size,
                show_hidden: true,
                icon_pack: "default".to_string(),
            };
            page.apply_config_ui(&ui, cx);
            assert!(page.show_hidden);
            assert!(page.sort_key == SortKey::Size);
        })
        .unwrap();
}

#[gpui::test]
async fn apply_filter_hides_dotfiles_until_enabled(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.entries = vec![
                file(".hidden", "file", 1),
                file("visible.txt", "file", 2),
                file("docs", "dir", 0),
            ];

            page.show_hidden = false;
            page.apply_filter();
            assert!(
                page.filtered_entries
                    .iter()
                    .all(|entry| entry.name != ".hidden")
            );
            assert_eq!(page.filtered_entries.len(), 2);

            page.show_hidden = true;
            page.apply_filter();
            assert!(
                page.filtered_entries
                    .iter()
                    .any(|entry| entry.name == ".hidden")
            );
            assert_eq!(page.filtered_entries.len(), 3);
        })
        .unwrap();
}

#[gpui::test]
async fn status_message_set_and_clear(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.set_status(StatusLevel::Error, "boom");
            assert_eq!(page.status_for_footer(), Some(("boom".to_string(), true)));
            page.clear_status();
            assert_eq!(page.status_for_footer(), None);
        })
        .unwrap();
}

#[gpui::test]
async fn open_preview_loads_text_off_thread(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("note.txt");
    std::fs::write(&file_path, "hello, gpui test world\n").unwrap();
    let path = file_path.to_string_lossy().to_string();

    let window = new_explorer(cx);
    window
        .update(cx, |page, window, cx| {
            page.open_preview(path.clone(), window, cx);
        })
        .unwrap();

    // `open_preview` reads the file on the background executor and applies the
    // result on the foreground thread. Drive both to completion: the executor
    // timer advances the test clock, then `run_until_parked` flushes the spawned
    // tasks. (Using `smol`/`tokio` sleeps here would not be tracked by the GPUI
    // scheduler and `run_until_parked` would return early.)
    cx.background_executor
        .timer(Duration::from_millis(50))
        .await;
    cx.run_until_parked();

    window
        .read_with(cx, |page, _cx| {
            assert_eq!(
                page.preview_text.as_deref(),
                Some("hello, gpui test world\n")
            );
            assert!(page.preview_editor.is_some());
        })
        .unwrap();
}

#[gpui::test]
async fn navigation_history_supports_back_forward_and_truncation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::create_dir(root.join("other")).unwrap();
    std::fs::write(root.join("a.txt"), "x").unwrap();
    let root_s = root.to_string_lossy().to_string();
    let sub_s = root.join("sub").to_string_lossy().to_string();
    let other_s = root.join("other").to_string_lossy().to_string();

    let window = new_explorer(cx);
    window
        .update(cx, |page, window, cx| {
            page.change_dir(root_s.clone(), window, cx);
            assert!(page.entries.iter().any(|e| e.name == "a.txt"));
            assert!(page.entries.iter().any(|e| e.name == "sub"));

            // Navigating to the current directory is a no-op.
            let len = page.history.len();
            page.change_dir(root_s.clone(), window, cx);
            assert_eq!(page.history.len(), len);

            page.change_dir(sub_s.clone(), window, cx);
            assert_eq!(page.cwd, sub_s);

            page.go_back(window, cx);
            assert_eq!(page.cwd, root_s);
            page.go_forward(window, cx);
            assert_eq!(page.cwd, sub_s);

            // Back, then a new navigation drops the forward ("sub") entry.
            page.go_back(window, cx);
            page.change_dir(other_s.clone(), window, cx);
            assert_eq!(page.cwd, other_s);
            let cwd = page.cwd.clone();
            page.go_forward(window, cx);
            assert_eq!(page.cwd, cwd, "no forward history remains");
        })
        .unwrap();
}

#[gpui::test]
async fn activate_entry_routes_by_kind(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("folder")).unwrap();
    std::fs::write(root.join("file.txt"), "hi").unwrap();
    let folder = FileEntryDto {
        name: "folder".into(),
        path: root.join("folder").to_string_lossy().to_string(),
        kind: "dir".into(),
        size: 0,
        modified: 0,
    };
    let doc = FileEntryDto {
        name: "file.txt".into(),
        path: root.join("file.txt").to_string_lossy().to_string(),
        kind: "file".into(),
        size: 2,
        modified: 0,
    };

    let window = new_explorer(cx);
    window
        .update(cx, |page, window, cx| {
            page.activate_entry(folder.clone(), window, cx);
            assert_eq!(page.cwd, folder.path, "activating a dir navigates into it");
        })
        .unwrap();
    window
        .update(cx, |page, window, cx| {
            page.activate_entry(doc.clone(), window, cx);
            // open_preview records the target path synchronously before reading.
            assert_eq!(page.preview_path.as_deref(), Some(doc.path.as_str()));
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
async fn search_visibility_open_close_toggle(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, window, cx| {
            assert!(!page.search_visible);
            page.open_search(window, cx);
            assert!(page.search_visible);
            page.close_search(window, cx);
            assert!(!page.search_visible);
            page.toggle_search(window, cx);
            assert!(page.search_visible);
            page.toggle_search(window, cx);
            assert!(!page.search_visible);
        })
        .unwrap();
}

#[gpui::test]
async fn trigger_search_empty_clears_and_no_service_degrades(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, window, cx| {
            // Empty query clears results without raising an error.
            page.search_query = String::new();
            page.trigger_search(window, cx);
            assert!(page.search_results.is_none());
            assert!(page.status_for_footer().is_none());

            // A real query with no search service falls back to name filtering and
            // surfaces an error status.
            page.search_query = "needle".into();
            page.trigger_search(window, cx);
            assert!(page.search_results.is_none());
            let (_, is_error) = page.status_for_footer().expect("degraded status set");
            assert!(is_error);
        })
        .unwrap();
}

#[gpui::test]
async fn view_mode_and_match_toggles_flip_state(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, cx| {
            assert!(page.view_mode == ViewMode::List);
            page.set_view_mode(ViewMode::Grid, cx);
            assert!(page.view_mode == ViewMode::Grid);

            assert!(!page.match_case && !page.use_regex && !page.match_whole_word);
            page.toggle_match_case(cx);
            page.toggle_use_regex(cx);
            page.toggle_match_whole_word(cx);
            assert!(page.match_case && page.use_regex && page.match_whole_word);
        })
        .unwrap();
}

#[gpui::test]
async fn column_resize_applies_delta_with_min_clamp(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            let start = page.col_name_width;
            page.start_column_resize(0, point(px(100.0), px(0.0)));
            page.update_column_resize(point(px(150.0), px(0.0)));
            assert_eq!(page.col_name_width, start + 50.0);

            // A large negative drag clamps to the minimum column width.
            page.update_column_resize(point(px(-10_000.0), px(0.0)));
            assert_eq!(page.col_name_width, nohrs_core::config::MIN_COLUMN_WIDTH);

            page.stop_column_resize();
            assert!(page.resizing_column.is_none());
        })
        .unwrap();
}

#[gpui::test]
async fn item_sizes_track_entries_and_table_width_sums_columns(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.filtered_entries = vec![file("a", "file", 1), file("b", "file", 2)];
            page.update_item_sizes();
            assert_eq!(page.item_sizes.len(), 2);

            let expected = page.col_name_width
                + page.col_type_width
                + page.col_size_width
                + page.col_modified_width
                + page.col_action_width
                + nohrs_core::config::TABLE_HORIZONTAL_PADDING;
            assert_eq!(page.total_table_width(), expected);

            page.record_click(1, 2);
            assert!(page.last_click_info.is_some());
        })
        .unwrap();
}

#[gpui::test]
async fn reload_reports_error_for_unreadable_dir(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.cwd = "/nonexistent/nohrs/dir".to_string();
            page.loaded = false;
            page.reload();
            let (_, is_error) = page.status_for_footer().expect("error status set");
            assert!(is_error);
            assert!(page.entries.is_empty());
        })
        .unwrap();
}

#[gpui::test]
async fn explorer_page_starts_with_single_pane(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .read_with(cx, |page, _cx| {
            assert_eq!(page.pane_count(), 1);
            assert_eq!(page.active_index(), 0);
            assert!(!page.is_synced());
        })
        .unwrap();
}

#[gpui::test]
async fn split_opens_second_pane_then_reorients(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.pane_count(), 2, "first split opens a second pane");
            assert_eq!(page.active_index(), 1, "the new pane becomes active");
            assert_eq!(page.direction(), SplitDirection::Vertical);

            // The opposite split shortcut flips orientation without adding a pane
            // (2-way cap, §3.1).
            page.split(SplitDirection::Horizontal, window, cx);
            assert_eq!(page.pane_count(), 2);
            assert_eq!(page.direction(), SplitDirection::Horizontal);
        })
        .unwrap();
}

#[gpui::test]
async fn close_pane_keeps_at_least_one(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.pane_count(), 2);

            page.close_pane(1, window, cx);
            assert_eq!(page.pane_count(), 1);
            assert_eq!(page.active_index(), 0);

            // The final pane can never be closed.
            page.close_pane(0, window, cx);
            assert_eq!(page.pane_count(), 1);
        })
        .unwrap();
}

#[gpui::test]
async fn set_active_selects_pane_in_bounds(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            page.set_active(0, window, cx);
            assert_eq!(page.active_index(), 0);

            // Out-of-range indices are ignored rather than panicking.
            page.set_active(5, window, cx);
            assert_eq!(page.active_index(), 0);
        })
        .unwrap();
}

#[gpui::test]
async fn panes_navigate_independently_by_default(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("left")).unwrap();
    std::fs::create_dir(root.join("right")).unwrap();
    let left = root.join("left").to_string_lossy().to_string();
    let right = root.join("right").to_string_lossy().to_string();

    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            let pane0 = page.pane(0);
            let pane1 = page.pane(1);
            pane0.update(cx, |pane, cx| pane.change_dir(left.clone(), window, cx));
            pane1.update(cx, |pane, cx| pane.change_dir(right.clone(), window, cx));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |page, cx| {
            assert_eq!(page.pane_cwd(0, cx).as_deref(), Some(left.as_str()));
            assert_eq!(page.pane_cwd(1, cx).as_deref(), Some(right.as_str()));
        })
        .unwrap();
}

#[gpui::test]
async fn synced_panes_mirror_navigation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("shared")).unwrap();
    let shared = root.join("shared").to_string_lossy().to_string();

    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            let synced = config::Explorer {
                split_direction: SplitDirection::Vertical,
                synced_panes: true,
                restore_tabs: true,
            };
            page.apply_config_explorer(&synced, cx);
            assert!(page.is_synced());

            let pane0 = page.pane(0);
            pane0.update(cx, |pane, cx| pane.change_dir(shared.clone(), window, cx));
        })
        .unwrap();
    // Navigation events are delivered on the next effect flush; mirror happens there.
    cx.run_until_parked();
    window
        .read_with(cx, |page, cx| {
            assert_eq!(page.pane_cwd(0, cx).as_deref(), Some(shared.as_str()));
            assert_eq!(
                page.pane_cwd(1, cx).as_deref(),
                Some(shared.as_str()),
                "sibling pane mirrors the active pane's path"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn synced_navigation_clears_stale_search_state(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("shared")).unwrap();
    let shared = root.join("shared").to_string_lossy().to_string();

    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            let synced = config::Explorer {
                split_direction: SplitDirection::Vertical,
                synced_panes: true,
                restore_tabs: true,
            };
            page.apply_config_explorer(&synced, cx);

            // Leave the mirrored pane with a stale filter and results from a
            // prior search so the reset is actually exercised (not vacuous).
            page.pane(1).update(cx, |pane, _cx| {
                pane.search_query = "stale".to_string();
                pane.search_visible = true;
                pane.search_results = Some(Vec::new());
            });
            page.pane(0)
                .update(cx, |pane, cx| pane.change_dir(shared.clone(), window, cx));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |page, cx| {
            let pane1 = page.pane(1);
            let pane1 = pane1.read(cx);
            assert!(
                pane1.search_query.is_empty(),
                "stale filter cleared on sync"
            );
            assert!(!pane1.search_visible);
            assert!(pane1.search_results.is_none());
        })
        .unwrap();
}

#[gpui::test]
async fn split_pane_inherits_applied_ui_config(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            let ui = config::Ui {
                default_sort: config::SortOrder::Size,
                show_hidden: true,
                icon_pack: "default".to_string(),
            };
            page.apply_config_ui(&ui, cx);

            // A pane opened by a later split should pick up the applied config
            // rather than reverting to pane defaults.
            page.split(SplitDirection::Vertical, window, cx);
            let pane1 = page.pane(1);
            let pane1 = pane1.read(cx);
            assert!(pane1.show_hidden, "new pane inherits show_hidden");
            assert!(pane1.sort_key == SortKey::Size, "new pane inherits sort");
        })
        .unwrap();
}

#[gpui::test]
async fn root_pane_shows_sidebar_but_split_pane_hides_it(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            assert!(
                page.pane(0).read(cx).sidebar_visible,
                "the root pane shows its sidebar by default (§2)"
            );

            page.split(SplitDirection::Vertical, window, cx);
            assert!(
                !page.pane(1).read(cx).sidebar_visible,
                "a pane opened by a split starts with the sidebar collapsed"
            );
            assert!(
                page.pane(0).read(cx).sidebar_visible,
                "splitting does not disturb the original pane's sidebar"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn toggle_sidebar_is_independent_per_pane(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            let pane0 = page.pane(0); // sidebar visible (root)
            let pane1 = page.pane(1); // sidebar hidden (split)

            pane1.update(cx, |pane, cx| pane.toggle_sidebar(cx));
            assert!(pane1.read(cx).sidebar_visible, "pane 1 toggled on");
            assert!(
                pane0.read(cx).sidebar_visible,
                "toggling pane 1 leaves pane 0 untouched"
            );

            pane0.update(cx, |pane, cx| pane.toggle_sidebar(cx));
            assert!(!pane0.read(cx).sidebar_visible, "pane 0 toggled off");
            assert!(pane1.read(cx).sidebar_visible, "pane 1 unchanged");
        })
        .unwrap();
}

#[gpui::test]
async fn close_pane_keeps_subscriptions_aligned(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    let shared = shared.to_string_lossy().to_string();

    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.pane_count(), 2);
            assert_eq!(page.subscription_count(), 2, "one subscription per pane");

            // Closing index 0 must drop the subscription at the same index so the
            // Vec stays aligned with the surviving pane.
            page.close_pane(0, window, cx);
            assert_eq!(page.pane_count(), 1);
            assert_eq!(page.subscription_count(), 1);

            // Reuse the container: split again and enable sync. If the
            // subscription Vec had desynced, the mirror below would target the
            // wrong pane (or none).
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.subscription_count(), 2);
            let synced = config::Explorer {
                split_direction: SplitDirection::Vertical,
                synced_panes: true,
                restore_tabs: true,
            };
            page.apply_config_explorer(&synced, cx);
            page.pane(0)
                .update(cx, |pane, cx| pane.change_dir(shared.clone(), window, cx));
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |page, cx| {
            assert_eq!(
                page.pane_cwd(1, cx).as_deref(),
                Some(shared.as_str()),
                "the sibling still mirrors after a close/split cycle"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn new_tab_appends_and_activates(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            assert_eq!(page.tab_count(0), 1);
            page.test_new_tab(0, window, cx);
            assert_eq!(page.tab_count(0), 2, "a new tab is appended");
            assert_eq!(page.active_tab(0), 1, "the new tab becomes active");
            // A new tab defaults to home (§4).
            if let Ok(home) = std::env::var("HOME") {
                assert_eq!(page.tab_cwd(0, 1, cx).as_deref(), Some(home.as_str()));
            }

            // Switching back to the first tab makes it active again.
            page.test_activate_tab(0, 0, window, cx);
            assert_eq!(page.active_tab(0), 0);
        })
        .unwrap();
}

#[gpui::test]
async fn close_tab_keeps_pane_when_others_remain(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.test_new_tab(0, window, cx);
            page.test_new_tab(0, window, cx);
            assert_eq!(page.tab_count(0), 3);
            page.test_close_tab(0, 1, window, cx);
            assert_eq!(
                page.tab_count(0),
                2,
                "closing a non-last tab keeps the pane"
            );
            assert_eq!(page.pane_count(), 1);
        })
        .unwrap();
}

#[gpui::test]
async fn closing_last_tab_closes_its_pane(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.pane_count(), 2);
            assert_eq!(page.tab_count(1), 1);

            // Closing pane 1's only tab closes the pane (§3.1 / §4).
            page.test_close_tab(1, 0, window, cx);
            assert_eq!(page.pane_count(), 1);

            // The final tab of the final pane is never closed.
            page.test_close_tab(0, 0, window, cx);
            assert_eq!(page.pane_count(), 1);
            assert_eq!(page.tab_count(0), 1);
        })
        .unwrap();
}

#[gpui::test]
async fn reorder_tab_swaps_order_and_follows_active(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let first = first.to_string_lossy().to_string();
    let second = second.to_string_lossy().to_string();

    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            // Tab 0 -> first, tab 1 (new, active) -> second.
            page.pane(0)
                .update(cx, |pane, _cx| pane.cwd = first.clone());
            page.test_new_tab(0, window, cx);
            page.pane(0)
                .update(cx, |pane, _cx| pane.cwd = second.clone());
            assert_eq!(page.tab_cwd(0, 0, cx).as_deref(), Some(first.as_str()));
            assert_eq!(page.tab_cwd(0, 1, cx).as_deref(), Some(second.as_str()));
            assert_eq!(page.active_tab(0), 1);

            // Drag tab 0 to position 1; the active tab (second) follows its entity.
            page.test_reorder_tab(0, 0, 1, cx);
            assert_eq!(page.tab_cwd(0, 0, cx).as_deref(), Some(second.as_str()));
            assert_eq!(page.tab_cwd(0, 1, cx).as_deref(), Some(first.as_str()));
            assert_eq!(page.active_tab(0), 0, "the active tab follows the reorder");
        })
        .unwrap();
}

#[gpui::test]
async fn subscriptions_track_every_tab(cx: &mut TestAppContext) {
    let window = new_explorer_page(cx);
    window
        .update(cx, |page, window, cx| {
            assert_eq!(page.subscription_count(), 1);
            page.test_new_tab(0, window, cx);
            assert_eq!(page.subscription_count(), 2, "one subscription per tab");
            page.split(SplitDirection::Vertical, window, cx);
            assert_eq!(page.subscription_count(), 3, "the split's pane adds a tab");
            page.test_close_tab(0, 1, window, cx);
            assert_eq!(
                page.subscription_count(),
                2,
                "closing a tab drops its subscription"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn tabs_round_trip_through_the_store(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let alpha = dir.path().join("alpha");
    std::fs::create_dir(&alpha).unwrap();
    let alpha = alpha.to_string_lossy().to_string();
    let store: Arc<dyn KvStore> =
        Arc::new(RedbKvStore::open_in_memory(&StoreLogConfig::default()).unwrap());

    // First session: open a second tab and navigate it, then let the debounced
    // save fire and write to the store.
    let window = new_explorer_page_with_store(cx, store.clone(), true);
    window
        .update(cx, |page, window, cx| {
            page.test_new_tab(0, window, cx);
            page.pane(0)
                .update(cx, |pane, cx| pane.change_dir(alpha.clone(), window, cx));
        })
        .unwrap();
    // Drive the debounce timer past `SAVE_DEBOUNCE`, then flush the spawned save.
    cx.background_executor
        .timer(Duration::from_millis(600))
        .await;
    cx.run_until_parked();

    // Second session with the same store restores both tabs.
    let restored = new_explorer_page_with_store(cx, store.clone(), true);
    restored
        .read_with(cx, |page, cx| {
            assert_eq!(page.tab_count(0), 2, "both tabs are restored");
            assert_eq!(page.tab_cwd(0, 1, cx).as_deref(), Some(alpha.as_str()));
        })
        .unwrap();
}

#[gpui::test]
async fn restore_disabled_ignores_saved_session(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let beta = dir.path().join("beta");
    std::fs::create_dir(&beta).unwrap();
    let beta = beta.to_string_lossy().to_string();
    let store: Arc<dyn KvStore> =
        Arc::new(RedbKvStore::open_in_memory(&StoreLogConfig::default()).unwrap());

    let window = new_explorer_page_with_store(cx, store.clone(), true);
    window
        .update(cx, |page, window, cx| {
            page.test_new_tab(0, window, cx);
            page.pane(0)
                .update(cx, |pane, cx| pane.change_dir(beta.clone(), window, cx));
        })
        .unwrap();
    cx.background_executor
        .timer(Duration::from_millis(600))
        .await;
    cx.run_until_parked();

    // With restore disabled, a fresh session starts with a single default tab.
    let fresh = new_explorer_page_with_store(cx, store.clone(), false);
    fresh
        .read_with(cx, |page, _cx| {
            assert_eq!(page.tab_count(0), 1, "restore disabled: no extra tabs");
            assert_eq!(page.pane_count(), 1);
        })
        .unwrap();
}

#[gpui::test]
async fn selection_single_toggle_range_and_paths(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.filtered_entries = vec![
                file("a", "file", 1),
                file("b", "file", 2),
                file("c", "file", 3),
                file("d", "file", 4),
            ];

            // Single selection re-anchors and becomes active.
            page.select_single(1);
            assert!(page.is_selected(1));
            assert_eq!(page.selection.len(), 1);
            assert_eq!(page.active_index, Some(1));

            // Shift range spans from the anchor (row 1) to row 3 inclusive.
            page.select_range_to(3);
            assert_eq!(
                page.selection.iter().copied().collect::<Vec<_>>(),
                vec![1, 2, 3]
            );
            assert_eq!(page.active_index, Some(3));

            // Cmd/Ctrl toggle removes one without clearing the rest and adds
            // another, re-anchoring at the toggled row.
            page.toggle_select(2);
            assert!(!page.is_selected(2) && page.is_selected(1) && page.is_selected(3));
            page.toggle_select(0);
            assert!(page.is_selected(0));

            // `selected_paths` follows row order (rows 0, 1, 3).
            assert_eq!(page.selected_paths(), vec!["/tmp/a", "/tmp/b", "/tmp/d"]);
        })
        .unwrap();
}

#[gpui::test]
async fn selection_all_clear_and_arrow_navigation(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.filtered_entries = vec![
                file("a", "file", 1),
                file("b", "file", 2),
                file("c", "file", 3),
            ];

            page.select_all();
            assert_eq!(page.selection.len(), 3);
            assert_eq!(page.active_index, Some(2));

            page.clear_selection();
            assert!(page.selection.is_empty());
            assert_eq!(page.active_index, None);
            assert_eq!(page.selection_anchor, None);

            // Arrow-down from an empty selection lands on the first row.
            page.move_active(1, false);
            assert_eq!(page.active_index, Some(0));
            page.move_active(1, false);
            assert_eq!(page.active_index, Some(1));

            // Shift+down extends the range from the anchor; up is clamped at 0.
            page.move_active(1, true);
            assert_eq!(
                page.selection.iter().copied().collect::<Vec<_>>(),
                vec![1, 2]
            );
            page.select_single(0);
            page.move_active(-1, false);
            assert_eq!(page.active_index, Some(0));
        })
        .unwrap();
}

#[gpui::test]
async fn apply_filter_resets_stale_selection(cx: &mut TestAppContext) {
    let window = new_explorer(cx);
    window
        .update(cx, |page, _window, _cx| {
            page.entries = vec![file("a", "file", 1), file("b", "file", 2)];
            page.apply_filter();
            page.select_all();
            assert!(!page.selection.is_empty());

            // Re-filtering rebuilds the row set, so the selection must reset to
            // avoid pointing at stale indices.
            page.apply_filter();
            assert!(page.selection.is_empty());
            assert_eq!(page.active_index, None);
        })
        .unwrap();
}

// Opens a pane rooted at a real directory and loads its listing, so the file
// operation tests can act on actual files. Mirrors how `reload` is driven
// elsewhere, but anchored at a temp dir instead of the cwd.
fn open_pane_at(cx: &mut TestAppContext, root: &std::path::Path) -> WindowHandle<ExplorerPane> {
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, _cx| {
            pane.cwd = root.to_string_lossy().to_string();
            pane.reload();
        })
        .unwrap();
    window
}

// Builds an `ExplorerPane` wrapped in a `gpui_component::Root`, the way the real
// app mounts it. Operations that open a dialog (paste conflicts, permanent
// delete) or render a focused inline-rename input go through `Root`, so they
// need the Root layer present — a bare-pane window panics otherwise. Returns the
// pane entity so tests can drive it within the window's `Window`.
fn new_explorer_in_root(cx: &mut TestAppContext) -> (WindowHandle<Root>, Entity<ExplorerPane>) {
    cx.update(gpui_component::init);
    let mut pane = None;
    let window = cx.add_window(|window, cx| {
        let resizable = cx.new(|_| ResizableState::default());
        let search_input = cx.new(|cx| InputState::new(window, cx));
        let entity =
            cx.new(|cx| ExplorerPane::new(resizable, search_input, None, cx.focus_handle()));
        pane = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    (window, pane.expect("Root::new build closure ran"))
}

#[gpui::test]
async fn permanent_delete_removes_only_the_selected(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(root.join(name), name).unwrap();
    }
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, _window, cx| {
            // Sorted rows: a=0, b=1, c=2. Delete a and c.
            pane.select_single(0);
            pane.toggle_select(2);
            let paths = pane.selected_paths();
            pane.delete_permanent_paths(paths, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert!(!root.join("a.txt").exists());
    assert!(root.join("b.txt").exists(), "unselected file survives");
    assert!(!root.join("c.txt").exists());
}

// Sets up `src` (copied/cut from) and `dst` (pasted into) each containing
// `foo.txt`, so pasting always collides.
fn paste_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(src.join("foo.txt"), "SRC").unwrap();
    std::fs::write(dst.join("foo.txt"), "DST").unwrap();
    (dir, src, dst)
}

#[gpui::test]
async fn paste_copy_with_rename_keeps_both(cx: &mut TestAppContext) {
    let (_dir, src, dst) = paste_fixture();
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.copy_selection(cx);

            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            // `prepare_paste` is the dialog-free half of `paste_into_cwd`; the
            // dialog plumbing itself is covered by GUI verification.
            let has_pending = pane.prepare_paste(cx);
            assert!(has_pending, "the name collision is queued");
            let done = pane.resolve_current_conflict(ConflictResolution::Rename, cx);
            assert!(done, "only one conflict to resolve");
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(dst.join("foo.txt")).unwrap(), "DST");
    assert_eq!(
        std::fs::read_to_string(dst.join("foo (2).txt")).unwrap(),
        "SRC",
        "rename keeps the existing file and adds a numbered copy"
    );
}

#[gpui::test]
async fn paste_copy_with_overwrite_replaces(cx: &mut TestAppContext) {
    let (_dir, src, dst) = paste_fixture();
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.copy_selection(cx);

            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            pane.prepare_paste(cx);
            pane.resolve_current_conflict(ConflictResolution::Overwrite, cx);
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(dst.join("foo.txt")).unwrap(), "SRC");
    assert!(!dst.join("foo (2).txt").exists());
}

#[gpui::test]
async fn paste_copy_with_skip_leaves_destination(cx: &mut TestAppContext) {
    let (_dir, src, dst) = paste_fixture();
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.copy_selection(cx);

            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            pane.prepare_paste(cx);
            pane.resolve_current_conflict(ConflictResolution::Skip, cx);
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(dst.join("foo.txt")).unwrap(), "DST");
    assert!(!dst.join("foo (2).txt").exists());
    assert!(
        src.join("foo.txt").exists(),
        "a copy leaves the source intact"
    );
}

#[gpui::test]
async fn paste_apply_to_all_resolves_every_conflict_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(src.join("foo.txt"), "S1").unwrap();
    std::fs::write(src.join("bar.txt"), "S2").unwrap();
    std::fs::write(dst.join("foo.txt"), "D1").unwrap();
    std::fs::write(dst.join("bar.txt"), "D2").unwrap();
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.copy_selection(cx);

            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            pane.prepare_paste(cx);
            // Two conflicts queued; one Overwrite choice with "apply to all" set
            // resolves both.
            pane.set_apply_to_all(true, cx);
            let done = pane.resolve_current_conflict(ConflictResolution::Overwrite, cx);
            assert!(done, "apply-to-all clears every remaining conflict");
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(dst.join("foo.txt")).unwrap(), "S1");
    assert_eq!(std::fs::read_to_string(dst.join("bar.txt")).unwrap(), "S2");
}

#[gpui::test]
async fn cut_paste_without_conflict_moves_the_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(src.join("x.txt"), "X").unwrap();
    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.cut_selection(cx);
        })
        .unwrap();
    window
        .update(cx, |pane, window, cx| {
            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            // No collision, so the paste executes immediately with no dialog.
            pane.paste_into_cwd(window, cx);
            assert!(pane.paste_plan.is_none());
        })
        .unwrap();
    cx.run_until_parked();
    assert!(dst.join("x.txt").exists());
    assert!(!src.join("x.txt").exists(), "a cut removes the source");
}

#[gpui::test]
async fn paste_numbers_a_second_source_sharing_an_existing_name(cx: &mut TestAppContext) {
    // Two clipboard sources named `foo.txt`, and `foo.txt` already exists at the
    // destination. Both collide with the same single destination, so resolving
    // the collision as Overwrite must not point both at it — the second would
    // silently replace the first.
    let dir = tempfile::tempdir().unwrap();
    let (first, second, dst) = (
        dir.path().join("a"),
        dir.path().join("b"),
        dir.path().join("dst"),
    );
    for sub in [&first, &second, &dst] {
        std::fs::create_dir(sub).unwrap();
    }
    std::fs::write(first.join("foo.txt"), "A").unwrap();
    std::fs::write(second.join("foo.txt"), "B").unwrap();
    std::fs::write(dst.join("foo.txt"), "D").unwrap();

    let window = new_explorer(cx);
    // Two files in different directories can't be selected in one listing, so
    // seed them through the system-clipboard path text, which `read_clipboard`
    // accepts as a copy.
    cx.update(|cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(format!(
            "{}\n{}",
            first.join("foo.txt").display(),
            second.join("foo.txt").display()
        )));
    });
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            assert!(pane.prepare_paste(cx), "the existing name collides");
            pane.set_apply_to_all(true, cx);
            pane.resolve_current_conflict(ConflictResolution::Overwrite, cx);
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(dst.join("foo.txt")).unwrap(),
        "A",
        "the conflicting source overwrites the destination"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("foo (2).txt")).unwrap(),
        "B",
        "the second source sharing that name is numbered, not dropped"
    );
}

#[gpui::test]
async fn closing_the_conflict_dialog_leaves_the_plan_for_the_resolving_button(
    cx: &mut TestAppContext,
) {
    // Each conflict button resolves, then calls `window.close_dialog(cx)`, then
    // `execute_paste_plan`. That order is only safe because gpui-component's
    // `WindowExt::close_dialog` pops the dialog stack without running the
    // dialog's own `on_close` — which cancels the paste, and would otherwise
    // take the plan out from under the very paste the button just resolved.
    // Pin it, so an upgrade that starts firing `on_close` from `close_dialog`
    // fails here rather than silently pasting nothing.
    use gpui_component::WindowExt as _;

    let (_dir, src, dst) = paste_fixture();
    let (window, pane) = new_explorer_in_root(cx);
    // Opening the dialog updates the `Root` entity, so the window's own
    // `update` (which already holds it) would panic on the second lease.
    // `VisualTestContext` hands out the `Window` without taking that lease.
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

    pane.update_in(&mut cx, |pane, window, cx| {
        pane.cwd = src.to_string_lossy().to_string();
        pane.reload();
        pane.select_all();
        pane.copy_selection(cx);
        pane.cwd = dst.to_string_lossy().to_string();
        pane.reload();
        pane.paste_into_cwd(window, cx);
    });

    let done = pane.update(&mut cx, |pane, cx| {
        assert!(
            pane.paste_plan.is_some(),
            "the collision opens the dialog with a plan pending"
        );
        pane.resolve_current_conflict(ConflictResolution::Overwrite, cx)
    });
    assert!(done, "the only conflict is resolved");

    cx.update(|window, cx| window.close_dialog(cx));
    pane.update(&mut cx, |pane, cx| {
        assert!(
            pane.paste_plan.is_some(),
            "close_dialog must not cancel the paste it is closing over"
        );
        pane.execute_paste_plan(cx);
    });
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(dst.join("foo.txt")).unwrap(),
        "SRC",
        "the resolved overwrite actually ran"
    );
}

#[gpui::test]
async fn cut_paste_keeps_failed_sources_on_the_clipboard_as_a_cut(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(src.join("x.txt"), "X").unwrap();

    let window = new_explorer(cx);
    window
        .update(cx, |pane, _window, cx| {
            pane.cwd = src.to_string_lossy().to_string();
            pane.reload();
            pane.select_all();
            pane.cut_selection(cx);
        })
        .unwrap();

    // Make the move fail by removing the source out from under the paste.
    std::fs::remove_file(src.join("x.txt")).unwrap();
    window
        .update(cx, |pane, window, cx| {
            pane.cwd = dst.to_string_lossy().to_string();
            pane.reload();
            pane.paste_into_cwd(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |pane, _cx| {
            let (_, is_error) = pane.status_for_footer().expect("the failure is reported");
            assert!(is_error, "the move failed");
        })
        .unwrap();
    assert!(!dst.join("x.txt").exists());

    // Retrying must still be a *move*. Were the failed source dropped from the
    // clipboard, this would fall back to the system clipboard's path text, which
    // carries no cut/copy distinction and so would copy instead.
    std::fs::write(src.join("x.txt"), "X").unwrap();
    window
        .update(cx, |pane, window, cx| pane.paste_into_cwd(window, cx))
        .unwrap();
    cx.run_until_parked();

    assert_eq!(std::fs::read_to_string(dst.join("x.txt")).unwrap(), "X");
    assert!(
        !src.join("x.txt").exists(),
        "the retry moves the source rather than copying it"
    );
}

#[gpui::test]
async fn rename_collision_falls_back_to_numbering(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "a").unwrap();
    std::fs::write(root.join("b.txt"), "b").unwrap();
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, window, cx| {
            // Rename b.txt (row 1) to the already-taken "a.txt".
            pane.begin_rename(1, window, cx);
            let input = pane.renaming.as_ref().unwrap().input.clone();
            input.update(cx, |state, cx| state.set_value("a.txt", window, cx));
            pane.commit_rename(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a");
    assert_eq!(
        std::fs::read_to_string(root.join("a (2).txt")).unwrap(),
        "b",
        "the conflicting rename is numbered rather than overwriting"
    );
    assert!(!root.join("b.txt").exists());
}

#[gpui::test]
async fn new_folder_picks_a_unique_name(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("New Folder")).unwrap();
    let (window, pane) = new_explorer_in_root(cx);
    window
        .update(cx, |_root, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.cwd = root.to_string_lossy().to_string();
                pane.reload();
                pane.create_new_folder(window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    assert!(
        root.join("New Folder (2)").is_dir(),
        "a new folder avoids colliding with the existing one"
    );
}

#[gpui::test]
async fn copy_selection_reports_status(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "A").unwrap();
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, _window, cx| {
            pane.select_all();
            pane.copy_selection(cx);
            assert_eq!(
                pane.status_for_footer(),
                Some(("1 item(s) copied".to_string(), false))
            );
        })
        .unwrap();
}

#[gpui::test]
async fn paste_copy_into_same_directory_duplicates(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "A").unwrap();
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, _window, cx| {
            pane.select_all();
            pane.copy_selection(cx);
            // Pasting back into the same directory duplicates without a dialog.
            let has_pending = pane.prepare_paste(cx);
            assert!(!has_pending);
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "A");
    assert_eq!(
        std::fs::read_to_string(root.join("a (2).txt")).unwrap(),
        "A"
    );
}

#[gpui::test]
async fn prepare_paste_without_clipboard_is_noop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let window = open_pane_at(cx, dir.path());
    window
        .update(cx, |pane, _window, cx| {
            assert!(!pane.prepare_paste(cx));
            assert!(pane.paste_plan.is_none());
        })
        .unwrap();
}

#[gpui::test]
async fn trash_selection_removes_from_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "A").unwrap();
    std::fs::write(root.join("b.txt"), "B").unwrap();
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, _window, cx| {
            pane.select_single(0); // a.txt
            pane.trash_selection(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert!(
        !root.join("a.txt").exists(),
        "trashed file leaves the source"
    );
    assert!(root.join("b.txt").exists());
}

#[gpui::test]
async fn delete_permanent_reports_failure_in_status(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let window = open_pane_at(cx, dir.path());
    window
        .update(cx, |pane, _window, cx| {
            pane.delete_permanent_paths(vec!["/no/such/path/xyzzy".to_string()], cx);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |pane, _cx| {
            let (text, is_error) = pane.status_for_footer().expect("a status was set");
            assert!(is_error);
            assert!(text.contains("1 of 1"), "got: {text}");
        })
        .unwrap();
}

#[gpui::test]
async fn rename_to_invalid_name_reports_error(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "A").unwrap();
    let window = open_pane_at(cx, root);
    window
        .update(cx, |pane, window, cx| {
            pane.begin_rename(0, window, cx);
            let input = pane.renaming.as_ref().unwrap().input.clone();
            // A name with a path separator is rejected by `rename_in_place`.
            input.update(cx, |state, cx| state.set_value("bad/name", window, cx));
            pane.commit_rename(window, cx);
        })
        .unwrap();
    window
        .read_with(cx, |pane, _cx| {
            let (text, is_error) = pane.status_for_footer().expect("a status was set");
            assert!(is_error);
            assert!(text.contains("Rename failed"), "got: {text}");
        })
        .unwrap();
    assert!(root.join("a.txt").exists(), "the original is untouched");
}

#[gpui::test]
async fn paste_falls_back_to_system_clipboard_paths(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dst).unwrap();
    std::fs::write(src.join("x.txt"), "X").unwrap();
    let src_path = src.join("x.txt").to_string_lossy().to_string();
    let window = open_pane_at(cx, &dst);
    window
        .update(cx, |pane, _window, cx| {
            // No internal clipboard set; a bare path on the system clipboard is
            // read back as a copy source.
            cx.write_to_clipboard(ClipboardItem::new_string(src_path));
            let has_pending = pane.prepare_paste(cx);
            assert!(!has_pending);
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert!(
        dst.join("x.txt").exists(),
        "pasted from the system clipboard path"
    );
}

#[gpui::test]
async fn paste_numbers_same_basename_sources(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let dst = dir.path().join("dst");
    for d in [&a, &b, &dst] {
        std::fs::create_dir(d).unwrap();
    }
    std::fs::write(a.join("foo.txt"), "A1").unwrap();
    std::fs::write(b.join("foo.txt"), "B1").unwrap();
    let window = open_pane_at(cx, &dst);
    window
        .update(cx, |pane, _window, cx| {
            // Two sources with the same basename from different directories.
            let paths = format!(
                "{}\n{}",
                a.join("foo.txt").display(),
                b.join("foo.txt").display()
            );
            cx.write_to_clipboard(ClipboardItem::new_string(paths));
            let has_pending = pane.prepare_paste(cx);
            assert!(!has_pending, "batch collisions are numbered, not prompted");
            pane.execute_paste_plan(cx);
        })
        .unwrap();
    cx.run_until_parked();
    // Both land instead of one clobbering the other.
    let mut got = vec![
        std::fs::read_to_string(dst.join("foo.txt")).unwrap(),
        std::fs::read_to_string(dst.join("foo (2).txt")).unwrap(),
    ];
    got.sort();
    assert_eq!(got, vec!["A1".to_string(), "B1".to_string()]);
}
