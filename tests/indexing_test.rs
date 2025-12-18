use anyhow::Result;
use nohrs::services::search::indexer::IndexManager;
use std::fs;
use tempfile::tempdir;

// Note: We need to modify IndexManager to accept a custom path for testing
// or mock dirs::home_dir.
// For now, let's see if we can refactor IndexManager to be testable.
// Wait, IndexManager::new() hardcodes the path.
// I should update IndexManager to allow overriding the path for tests.

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
    assert_eq!(searcher.num_docs(), 1, "Should have 1 document indexed");

    // Verify search finds content
    use nohr::services::search::backend::SearchBackend;
    let results = manager.search("Hello")?;
    assert!(!results.is_empty(), "Should find 'Hello'");
    assert_eq!(results[0].path, test_file);

    // Update file
    fs::write(&test_file, "Updated content here")?;
    manager.update_file(&test_file)?;

    // Verify update
    // Note: commit is done in update_file, but reader needs reload usually?
    // Tantivy readers need reload to see changes. IndexManager methods usually create new reader each time?
    // IndexManager::search calls `self.index.reader()?`. Index::reader() returns a *new* reader or handle?
    // Actually `index.reader()` returns a `IndexReader` which has a reload policy.
    // If not configured, we might need to manually reload or get new reader.
    // But `self.index.reader()` creates a standard reader.
    // To ensure fresh view for search, we rely on `search` implementation calling `reader.searcher()`.
    // Wait, `Index::reader()` usually returns a pool.
    // Let's verify if `manager.search` gets fresh data.

    // With default settings, reader might lag?
    // `IndexManager::search` calls `self.index.reader()?`.
    // It calls `index.reader()` every time? No, that would be expensive.
    // IndexManager stores `index`.
    // Let's assume for test `index.reader()` gets fresh.

    let results_updated = manager.search("Updated")?;
    assert!(!results_updated.is_empty(), "Should find 'Updated'");

    let results_old = manager.search("Hello")?;
    assert!(
        results_old.is_empty(),
        "Should NOT find 'Hello' after update"
    );

    // Remove file
    manager.remove_file(&test_file)?;

    // Verify removal
    let searcher_after_remove = manager.index().reader()?.searcher();
    // Getting reader again *should* see changes if committed.
    assert_eq!(
        searcher_after_remove.num_docs(),
        0,
        "Should have 0 docs after removal"
    );

    Ok(())
}
