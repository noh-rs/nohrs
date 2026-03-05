use super::*;

#[gpui::test]
async fn test_search_flow_basic(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("hello", test_data::multi_file_matches());
    let call_count = mock.call_count.clone();

    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "hello".to_string();
        page.trigger_search(window, cx);

        // trigger_search 内で同期的に結果がセットされる
        assert!(page.search_results.is_some(), "results should be set after trigger_search");
        let results = page.search_results.as_ref().unwrap();
        assert_eq!(results.len(), 3); // 3 files
        assert!(!page.filtered_entries.is_empty());
        assert!(!page.is_performing_search);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    });
}

#[gpui::test]
async fn test_search_no_results(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new(); // No results configured

    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "nonexistent".to_string();
        page.trigger_search(window, cx);

        assert!(page.search_results.is_some());
        assert!(page.search_results.as_ref().unwrap().is_empty());
        assert!(page.filtered_entries.is_empty());
    });
}

#[gpui::test]
async fn test_search_error_handling(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_error_mode();

    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);

        // Error fallback: empty results
        assert!(page.search_results.is_some());
        assert!(page.search_results.as_ref().unwrap().is_empty());
        assert!(page.filtered_entries.is_empty());
        assert!(!page.is_performing_search);
    });
}

#[gpui::test]
async fn test_search_clear_restores_listing(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("file1.txt"), "hello").unwrap();
    std::fs::write(tmp.path().join("file2.txt"), "world").unwrap();

    let mock = MockSearchProvider::new().with_results("hello", test_data::single_match());
    let (page, cx) = build_explorer(cx, tmp.path().to_str().unwrap(), mock);

    // render で自動ロード済み
    let original_count = page.read_with(cx, |page, _| page.entries.len());
    assert_eq!(original_count, 2);

    // 検索実行 → クローズ → 元一覧復帰
    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "hello".to_string();
        page.trigger_search(window, cx);
        assert!(page.search_results.is_some());

        page.close_search(window, cx);
        assert!(page.search_results.is_none());
        assert!(page.search_query.is_empty());
        assert!(!page.search_visible);
        assert_eq!(page.filtered_entries.len(), original_count);
    });
}

#[gpui::test]
async fn test_search_empty_query_clears_results(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("file.txt"), "content").unwrap();

    let mock = MockSearchProvider::new().with_results("test", test_data::single_match());
    let (page, cx) = build_explorer(cx, tmp.path().to_str().unwrap(), mock);

    let original_count = page.read_with(cx, |page, _| page.entries.len());

    cx.update_window_entity(&page, |page, window, cx| {
        // Search
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
        assert!(page.search_results.is_some());

        // Clear query and re-trigger
        page.search_query.clear();
        page.trigger_search(window, cx);
        assert!(page.search_results.is_none());
        assert_eq!(page.filtered_entries.len(), original_count);
    });
}

#[gpui::test]
async fn test_search_result_activate_dir(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("subdir");
    std::fs::create_dir_all(&sub).unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    // Activate directory entry
    cx.update_window_entity(&page, |page, window, cx| {
        let dir_entry = page
            .filtered_entries
            .iter()
            .find(|e| e.kind == "dir")
            .cloned()
            .unwrap();
        page.activate_entry(dir_entry, window, cx);
        assert_eq!(page.cwd, sub.to_str().unwrap());
    });
}
