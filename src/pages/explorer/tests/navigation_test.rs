use super::*;

#[gpui::test]
async fn test_navigate_to_updates_entries(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("subdir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("file_in_sub.txt"), "content").unwrap();
    std::fs::write(tmp.path().join("root_file.txt"), "root").unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    page.update(cx, |page, _cx| {
        page.reload();
        assert_eq!(page.entries.len(), 2); // subdir + root_file
    });

    let sub_str = sub.to_str().unwrap().to_string();
    cx.update_window_entity(&page, |page, window, cx| {
        page.change_dir(sub_str, window, cx);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.cwd, sub.to_str().unwrap());
        assert_eq!(page.entries.len(), 1); // file_in_sub
    });
}

#[gpui::test]
async fn test_navigate_adds_to_history(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let sub_a = tmp.path().join("dir_a");
    let sub_b = tmp.path().join("dir_b");
    std::fs::create_dir_all(&sub_a).unwrap();
    std::fs::create_dir_all(&sub_b).unwrap();

    let (page, cx) = build_explorer_default(cx, tmp.path().to_str().unwrap());

    let dir_a = sub_a.to_str().unwrap().to_string();
    cx.update_window_entity(&page, |page, window, cx| {
        page.change_dir(dir_a, window, cx);
    });

    page.read_with(cx, |page, _| {
        assert!(!page.history.is_empty());
        assert_eq!(page.cwd, sub_a.to_str().unwrap());
    });

    let dir_b = sub_b.to_str().unwrap().to_string();
    cx.update_window_entity(&page, |page, window, cx| {
        page.change_dir(dir_b, window, cx);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.cwd, sub_b.to_str().unwrap());
        assert!(page.history.len() >= 2);
    });
}

#[gpui::test]
async fn test_go_back_and_forward(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let sub_a = tmp.path().join("dir_a");
    let sub_b = tmp.path().join("dir_b");
    std::fs::create_dir_all(&sub_a).unwrap();
    std::fs::create_dir_all(&sub_b).unwrap();

    let start_cwd = tmp.path().to_str().unwrap().to_string();
    let (page, cx) = build_explorer_default(cx, &start_cwd);

    // Navigate: start -> A -> B
    let dir_a = sub_a.to_str().unwrap().to_string();
    cx.update_window_entity(&page, |page, window, cx| {
        page.change_dir(dir_a, window, cx);
    });
    let dir_b = sub_b.to_str().unwrap().to_string();
    cx.update_window_entity(&page, |page, window, cx| {
        page.change_dir(dir_b, window, cx);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.cwd, sub_b.to_str().unwrap());
    });

    // Go back to A
    cx.update_window_entity(&page, |page, window, cx| {
        page.go_back(window, cx);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.cwd, sub_a.to_str().unwrap());
    });

    // Go forward to B
    cx.update_window_entity(&page, |page, window, cx| {
        page.go_forward(window, cx);
    });

    page.read_with(cx, |page, _| {
        assert_eq!(page.cwd, sub_b.to_str().unwrap());
    });
}
