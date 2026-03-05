use crate::common::search::test_index_manager;
use anyhow::Result;
use nohrs::services::search::SearchBackend;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_index_manager_home_scope_uses_index() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;
    fs::write(content_root.join("findme.txt"), "searchable content")?;

    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(None)?;

    // SearchBackend::search on IndexManager = Home scope behavior
    let results = manager.search("findme")?;
    assert!(!results.is_empty(), "Home scope search via index should work");
    Ok(())
}

#[test]
fn test_ripgrep_root_scope_search() -> Result<()> {
    use nohrs::services::search::ripgrep::RipgrepBackend;

    let tmp = tempdir()?;
    fs::write(tmp.path().join("rootfile.txt"), "root search content")?;

    let backend = RipgrepBackend::new(tmp.path().to_path_buf());
    let results = backend.search("root")?;
    assert!(!results.is_empty(), "Root scope search via ripgrep should work");
    Ok(())
}

#[test]
fn test_progress_tracking() -> Result<()> {
    let tmp = tempdir()?;
    let content_root = tmp.path().join("content");
    fs::create_dir_all(&content_root)?;

    // Create enough files to trigger progress updates
    for i in 0..200 {
        fs::write(
            content_root.join(format!("file_{:03}.txt", i)),
            format!("Content {}", i),
        )?;
    }

    let (progress_tx, progress_rx) = tokio::sync::watch::channel(0.0f32);
    let (manager, _index_dir) = test_index_manager(&content_root);
    manager.index_home(Some(progress_tx))?;

    // After indexing completes, progress should be 1.0
    let final_progress = *progress_rx.borrow();
    assert!(
        (final_progress - 1.0).abs() < f32::EPSILON,
        "Progress should be 1.0 after indexing, got {}",
        final_progress
    );
    Ok(())
}
