use super::*;

#[gpui::test]
async fn test_search_scope_switch(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, cx| {
        assert_eq!(page.search_scope, SearchScope::Home);
        page.set_search_scope(SearchScope::Root, cx);
        assert_eq!(page.search_scope, SearchScope::Root);
        page.set_search_scope(SearchScope::Home, cx);
        assert_eq!(page.search_scope, SearchScope::Home);
    });
}

#[gpui::test]
async fn test_search_type_switch(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, _cx| {
        assert_eq!(page.search_type, SearchType::All);
        page.search_type = SearchType::Filename;
        assert_eq!(page.search_type, SearchType::Filename);
        page.search_type = SearchType::Content;
        assert_eq!(page.search_type, SearchType::Content);
        page.search_type = SearchType::All;
        assert_eq!(page.search_type, SearchType::All);
    });
}

#[gpui::test]
async fn test_match_option_toggles(cx: &mut TestAppContext) {
    let (page, cx) = build_explorer_default(cx, "/tmp");

    page.update(cx, |page, cx| {
        // match_case
        assert!(!page.match_case);
        page.toggle_match_case(cx);
        assert!(page.match_case);
        page.toggle_match_case(cx);
        assert!(!page.match_case);

        // match_whole_word
        assert!(!page.match_whole_word);
        page.toggle_match_whole_word(cx);
        assert!(page.match_whole_word);
        page.toggle_match_whole_word(cx);
        assert!(!page.match_whole_word);

        // use_regex
        assert!(!page.use_regex);
        page.toggle_use_regex(cx);
        assert!(page.use_regex);
        page.toggle_use_regex(cx);
        assert!(!page.use_regex);
    });
}
