use super::*;

#[gpui::test]
async fn test_sort_entries_applied_on_reload(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("charlie.txt"), "c").unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("bravo.txt"), "b").unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.update(cx, |page, _cx| {
        page.reload();
    });

    page.read_with(cx, |page, _| {
        // Default sort: Name ascending
        assert_eq!(page.filtered_entries[0].name, "alpha.txt");
        assert_eq!(page.filtered_entries[1].name, "bravo.txt");
        assert_eq!(page.filtered_entries[2].name, "charlie.txt");
    });
}

#[gpui::test]
async fn test_sort_key_toggle_direction(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("bravo.txt"), "b").unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.update(cx, |page, _cx| {
        page.reload();
        // Name is default key, toggle to desc
        page.set_sort_key(SortKey::Name);
        assert!(!page.sort_asc);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.filtered_entries[0].name, "bravo.txt");
        assert_eq!(page.filtered_entries[1].name, "alpha.txt");
    });
}
