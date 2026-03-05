use crate::common::search::test_index_manager;
use anyhow::Result;
use nohrs::services::search::SearchBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_index_create_and_filename_search() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    fs::write(content_root.join("hello.txt"), "some content")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    let results = manager.search("hello")?;
    assert!(!results.is_empty(), "Should find 'hello' via filename");
    Ok(())
}

#[test]
fn test_index_create_and_content_search() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    fs::write(content_root.join("test.txt"), "Hello world content")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    let results = manager.search("world")?;
    assert!(!results.is_empty(), "Should find 'world' in file content");

    // line_number と line_content の検証
    let file_match = results
        .iter()
        .find(|r| r.line_number > 0)
        .expect("Should have a content match with line_number > 0");
    assert!(file_match.line_content.contains("world"));
    Ok(())
}

#[test]
fn test_update_file_reindexes() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    let file_path = content_root.join("test.txt");
    fs::write(&file_path, "Hello world content")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    // Update file content
    fs::write(&file_path, "Updated content here")?;
    manager.update_file(&file_path)?;

    let results_new = manager.search("Updated")?;
    assert!(!results_new.is_empty(), "Should find 'Updated' after update");

    let results_old = manager.search("Hello")?;
    // Hello should not be found in content anymore (though it may still be in path)
    let content_matches: Vec<_> = results_old
        .iter()
        .filter(|r| r.line_number > 0 && r.line_content.contains("Hello"))
        .collect();
    assert!(
        content_matches.is_empty(),
        "Should NOT find 'Hello' in content after update"
    );
    Ok(())
}

#[test]
fn test_remove_file() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    let file_path = content_root.join("removeme.txt");
    fs::write(&file_path, "remove this")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    let before = manager.search("removeme")?;
    assert!(!before.is_empty(), "File should be found before removal");

    manager.remove_file(&file_path)?;

    let after = manager.search("removeme")?;
    assert!(after.is_empty(), "File should not be found after removal");
    Ok(())
}

#[test]
fn test_index_japanese_content() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    fs::write(content_root.join("日本語.txt"), "こんにちは世界")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    // ファイル名検索
    let results = manager.search("日本語")?;
    assert!(
        !results.is_empty(),
        "Should find file with Japanese filename"
    );
    Ok(())
}

#[test]
fn test_index_large_file_count() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;

    for i in 0..120 {
        fs::write(
            content_root.join(format!("file_{:03}.txt", i)),
            format!("Content for file {}", i),
        )?;
    }

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    let reader = manager.index().reader()?;
    let doc_count = reader.searcher().num_docs();
    // 120 files + 1 directory (content_root itself) + sub-entries
    assert!(doc_count >= 120, "Should index at least 120 files, got {}", doc_count);
    Ok(())
}

#[test]
fn test_index_empty_directory() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("empty");
    fs::create_dir_all(&content_root)?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    let reader = manager.index().reader()?;
    let doc_count = reader.searcher().num_docs();
    // Only the root directory itself
    assert!(doc_count <= 1, "Empty dir should have at most 1 doc (the dir itself)");
    Ok(())
}
