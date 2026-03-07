use super::*;

fn make_many_results(count: usize) -> Vec<SearchResult> {
    (0..count)
        .map(|i| SearchResult {
            path: std::path::PathBuf::from(format!("/tmp/test/file_{}.rs", i)),
            line_number: 1,
            line_content: format!("match line {}", i),
            match_start: 0,
            match_end: 5,
        })
        .collect()
}

#[gpui::test]
async fn test_pagination_basic(cx: &mut TestAppContext) {
    // 10件の結果、1ページ3件
    let mock = MockSearchProvider::new().with_results("test", make_many_results(10));
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    page.update(cx, |page, _cx| {
        page.search_results_per_page = 3;
    });

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });

    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert_eq!(page.search_total_results, 10);
        assert_eq!(page.search_page, 0);
        assert_eq!(page.search_total_pages(), 4); // ceil(10/3) = 4
        assert_eq!(page.filtered_entries.len(), 3);
    });
}

#[gpui::test]
async fn test_pagination_next_prev(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", make_many_results(10));
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    page.update(cx, |page, _cx| {
        page.search_results_per_page = 3;
    });

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });

    cx.run_until_parked();

    // 次ページ
    page.update(cx, |page, cx| {
        page.search_next_page(cx);
        assert_eq!(page.search_page, 1);
        assert_eq!(page.filtered_entries.len(), 3);
    });

    // もう一度次ページ
    page.update(cx, |page, cx| {
        page.search_next_page(cx);
        assert_eq!(page.search_page, 2);
        assert_eq!(page.filtered_entries.len(), 3);
    });

    // 最後のページ (残り1件)
    page.update(cx, |page, cx| {
        page.search_next_page(cx);
        assert_eq!(page.search_page, 3);
        assert_eq!(page.filtered_entries.len(), 1);
    });

    // 最終ページ超過しない
    page.update(cx, |page, cx| {
        page.search_next_page(cx);
        assert_eq!(page.search_page, 3); // 変わらない
    });

    // 前ページ
    page.update(cx, |page, cx| {
        page.search_prev_page(cx);
        assert_eq!(page.search_page, 2);
        assert_eq!(page.filtered_entries.len(), 3);
    });
}

#[gpui::test]
async fn test_pagination_reset_on_new_search(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new()
        .with_results("test", make_many_results(10))
        .with_results("other", make_many_results(5));
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    page.update(cx, |page, _cx| {
        page.search_results_per_page = 3;
    });

    // 最初の検索
    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });
    cx.run_until_parked();

    // ページ移動
    page.update(cx, |page, cx| {
        page.search_next_page(cx);
        assert_eq!(page.search_page, 1);
    });

    // 新しい検索→ページリセット
    cx.update_window_entity(&page, |page, window, cx| {
        page.search_query = "other".to_string();
        page.trigger_search(window, cx);
    });
    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert_eq!(page.search_page, 0);
        assert_eq!(page.search_total_results, 5);
    });
}

#[gpui::test]
async fn test_pagination_single_page(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", make_many_results(3));
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    page.update(cx, |page, _cx| {
        page.search_results_per_page = 10;
    });

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });
    cx.run_until_parked();

    page.read_with(cx, |page, _| {
        assert_eq!(page.search_total_pages(), 1);
        assert_eq!(page.filtered_entries.len(), 3);
    });
}

#[gpui::test]
async fn test_prev_page_at_zero(cx: &mut TestAppContext) {
    let mock = MockSearchProvider::new().with_results("test", make_many_results(10));
    let (page, cx) = build_explorer(cx, "/tmp", mock);

    page.update(cx, |page, _cx| {
        page.search_results_per_page = 3;
    });

    cx.update_window_entity(&page, |page, window, cx| {
        page.open_search(window, cx);
        page.search_query = "test".to_string();
        page.trigger_search(window, cx);
    });
    cx.run_until_parked();

    // 0ページで前に行こうとしても0のまま
    page.update(cx, |page, cx| {
        page.search_prev_page(cx);
        assert_eq!(page.search_page, 0);
    });
}
