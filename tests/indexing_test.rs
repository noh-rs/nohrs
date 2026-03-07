use anyhow::Result;
use nohrs::core::types::SearchQuery;
use nohrs::services::search::indexer::IndexManager;
use nohrs::services::search::SearchBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_indexing_workflow() -> Result<()> {
    let temp_root = tempdir()?;
    let content_root = temp_root.path().join("home");
    let index_root = temp_root.path().join("index");

    fs::create_dir_all(&content_root)?;
    fs::create_dir_all(&index_root)?;

    // Create a test file
    let test_file = content_root.join("test.txt");
    fs::write(&test_file, "Hello world content")?;

    // Create manager with custom paths
    let manager = IndexManager::new_with_path(index_root.clone(), content_root.clone())?;

    // Initial indexing
    manager.index_home(None).expect("Initial indexing failed");
    let searcher = manager.index().reader()?.searcher();
    assert_eq!(
        searcher.num_docs(),
        2,
        "Should have 2 documents indexed (dir + file)"
    );

    // Verify search finds content
    let results = manager.search(&SearchQuery::new("Hello".into()))?;
    assert!(!results.is_empty(), "Should find 'Hello'");
    assert_eq!(results[0].path, test_file);

    // Update file
    fs::write(&test_file, "Updated content here")?;
    manager.update_file(&test_file)?;

    let results_updated = manager.search(&SearchQuery::new("Updated".into()))?;
    assert!(!results_updated.is_empty(), "Should find 'Updated'");

    let results_old = manager.search(&SearchQuery::new("Hello".into()))?;
    assert!(
        results_old.is_empty(),
        "Should NOT find 'Hello' after update"
    );

    // Remove file
    manager.remove_file(&test_file)?;

    // Verify removal
    let searcher_after_remove = manager.index().reader()?.searcher();
    assert_eq!(
        searcher_after_remove.num_docs(),
        1,
        "Should have 1 doc after removal"
    );

    Ok(())
}
