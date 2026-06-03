//! `TestAppContext`-based behavior tests for the explorer page: navigation and
//! history, search lifecycle, preview loading, sorting/filtering, column resize,
//! and status reporting. They also serve as the reference patterns documented in
//! `docs/testing.md` / `CONTRIBUTING.md`: build the view inside a test window,
//! mutate it through `window.update`, drive async work with the GPUI executor
//! timer + `run_until_parked`, and assert state with `window.read_with` — never
//! `smol::Timer` / `tokio` sleeps, which the GPUI scheduler does not track.

// Test fixtures write files directly; the synchronous-fs ban targets app code.
#![allow(clippy::disallowed_methods)]

use std::time::Duration;

use gpui::{point, px, AppContext, TestAppContext, WindowHandle};
use gpui_component::input::InputState;
use gpui_component::resizable::ResizableState;
use nohrs_core::config;
use nohrs_services::fs::listing::FileEntryDto;

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
        let resizable = ResizableState::new(cx);
        let search_input = cx.new(|cx| InputState::new(window, cx));
        ExplorerPane::new(resizable, search_input, None, cx.focus_handle())
    })
}

/// Build the split-view container (`ExplorerPage`), which owns its panes.
fn new_explorer_page(cx: &mut TestAppContext) -> WindowHandle<ExplorerPage> {
    cx.update(gpui_component::init);
    cx.add_window(|window, cx| {
        let resizable = ResizableState::new(cx);
        ExplorerPage::new(resizable, None, window, cx)
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
            assert!(page
                .filtered_entries
                .iter()
                .all(|entry| entry.name != ".hidden"));
            assert_eq!(page.filtered_entries.len(), 2);

            page.show_hidden = true;
            page.apply_filter();
            assert!(page
                .filtered_entries
                .iter()
                .any(|entry| entry.name == ".hidden"));
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
