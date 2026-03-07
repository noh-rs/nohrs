use super::*;

#[gpui::test]
async fn test_search_state_lifecycle(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", test_data::single_match());
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    // 初期状態
    page.read_with(cx, |page, _| {
        assert!(!page.search_visible);
        assert!(page.search_results.is_none());
        assert!(page.search_query.is_empty());
    });

    // 検索バー表示 + 検索実行
    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        assert!(page.search_visible);
        assert!(page.search_results.is_none());

        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });

    // 非同期検索の完了を待つ
    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert!(page.search_results.is_some());
        assert!(!page.is_performing_search);
    });

    // 検索クローズ → 初期状態に復帰
    cx.update_window_entity(&page, |page, window, cx| {
        page.close_search(window, cx);
        assert!(!page.search_visible);
        assert!(page.search_results.is_none());
        assert!(page.search_query.is_empty());
    });
}

#[gpui::test]
async fn test_dir_change_clears_search(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("subdir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("file.txt"), "content").unwrap();

    let mock = MockSearchProvider::new().with_results("query", test_data::single_match());
    let (page, cx) = build_explorer(cx, tmp.path().to_str().unwrap(), mock);

    // 検索実行
    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "query".to_string();
        page.trigger_search(window, cx);
    });

    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert!(page.search_visible);
        assert!(page.search_results.is_some());
    });

    // ディレクトリ移動 → 検索クリア
    cx.update_window_entity(&page, |page, window, cx| {
        let sub_str = sub.to_str().unwrap().to_string();
        page.change_dir(sub_str, window, cx);
        assert!(!page.search_visible);
        assert!(page.search_results.is_none());
        assert!(page.search_query.is_empty());
        assert_eq!(page.cwd, sub.to_str().unwrap());
        assert!(!page.entries.is_empty());
    });
}

#[gpui::test]
async fn test_search_expanded_files_state(cx: &mut TestAppContext) {
    let mock =
        MockSearchProvider::new().with_results("search", test_data::multi_match_same_file());
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.search_query = "search".to_string();
        page.trigger_search(window, cx);
    });

    // 非同期検索の完了を待つ
    cx.run_until_parked();

    // ファイル展開
    page.update(cx, |page, _cx| {
        page.expanded_search_files
            .insert("/tmp/test/lib.rs".to_string());
        page.update_item_sizes();
    });

    let expanded_sizes = page.read_with(cx, |page, _| page.item_sizes.clone());

    // ファイル折りたたみ
    page.update(cx, |page, _cx| {
        page.expanded_search_files.remove("/tmp/test/lib.rs");
        page.update_item_sizes();
    });

    let collapsed_sizes = page.read_with(cx, |page, _| page.item_sizes.clone());

    // 展開時の方がサイズが大きい（スニペット行分）
    if !expanded_sizes.is_empty() && !collapsed_sizes.is_empty() {
        assert!(expanded_sizes[0].height > collapsed_sizes[0].height);
    }
}

#[gpui::test]
async fn test_toggle_search(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    cx.update_window_entity(&page, |page, window, cx| {
        assert!(!page.search_visible);
        page.toggle_search(window, cx);
        assert!(page.search_visible);
        page.toggle_search(window, cx);
        assert!(!page.search_visible);
    });
}

#[gpui::test]
async fn test_open_search_idempotent(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        assert!(page.search_visible);
        // Opening again should be a no-op
        page.open_search(window, cx);
        assert!(page.search_visible);
    });
}
