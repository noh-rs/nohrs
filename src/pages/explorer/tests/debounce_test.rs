use super::*;

#[gpui::test]
async fn test_debounce_auto_search(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("hello", test_data::single_match());
    let call_count = mock.call_count.clone();
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        // 入力変更→デバウンス発火
        page.on_search_input_changed("hello".to_string(), window, cx);
    });

    // デバウンス完了を待つ
    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert!(page.search_results.is_some());
        assert!(!page.is_performing_search);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    });
}

#[gpui::test]
async fn test_debounce_cancels_previous(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new()
        .with_results("he", test_data::single_match())
        .with_results("hello", test_data::multi_file_matches());
    let call_count = mock.call_count.clone();
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        // 連続入力: 前のデバウンスがキャンセルされる
        page.on_search_input_changed("he".to_string(), window, cx);
        page.on_search_input_changed("hello".to_string(), window, cx);
    });

    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        // 最後の入力のみ検索される
        assert!(page.search_results.is_some());
        let results = page.search_results.as_ref().unwrap();
        assert_eq!(results.len(), 3); // multi_file_matches
        // "he" の検索はキャンセルされているので1回だけ呼ばれる
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    });
}

#[gpui::test]
async fn test_debounce_empty_input_no_search(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", test_data::single_match());
    let call_count = mock.call_count.clone();
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.on_search_input_changed("".to_string(), window, cx);
    });

    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert!(page.search_results.is_none());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    });
}

#[gpui::test]
async fn test_enter_bypasses_debounce(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", test_data::single_match());
    let call_count = mock.call_count.clone();
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        // Enter で即座に検索（デバウンスをバイパス）
        page.trigger_search(window, cx);
    });

    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert!(page.search_results.is_some());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    });
}
