use anyhow::Result;
use nohr::services::search::indexer::IndexManager;
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

    // Index the "home" directory
    manager.index_home()?;

    // Verify indexing
    let searcher = manager.index().reader()?.searcher();
    assert_eq!(searcher.num_docs(), 1, "Should have 1 document indexed");

    // Clean up is handled by tempdir drop
    Ok(())
}
