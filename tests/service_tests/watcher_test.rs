use anyhow::Result;
use nohrs::services::search::watcher::FileWatcher;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;
use tokio::time::timeout;

#[tokio::test]
async fn test_watcher_detects_file_creation() -> Result<()> {
    let tmp = tempdir()?;
    let watch_root = tmp.path().to_path_buf();
    let (tx, mut rx) = mpsc::channel(100);

    let _watcher = FileWatcher::new(watch_root.clone(), tx, Duration::from_millis(100))?;

    let file_path = watch_root.join("new_file.txt");
    fs::write(&file_path, "Initial content")?;

    let event = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(event.is_ok(), "Timed out waiting for creation event");
    let paths = event.unwrap().expect("Channel closed unexpectedly");
    let found = paths
        .iter()
        .any(|p| p.file_name() == Some(std::ffi::OsStr::new("new_file.txt")));
    assert!(found, "Should detect new_file.txt creation");
    Ok(())
}

#[tokio::test]
async fn test_watcher_detects_file_modification() -> Result<()> {
    let tmp = tempdir()?;
    let watch_root = tmp.path().to_path_buf();
    let file_path = watch_root.join("existing.txt");
    fs::write(&file_path, "Initial")?;

    let (tx, mut rx) = mpsc::channel(100);
    let _watcher = FileWatcher::new(watch_root.clone(), tx, Duration::from_millis(100))?;

    // Modify existing file
    fs::write(&file_path, "Modified content")?;

    let event = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(event.is_ok(), "Timed out waiting for modification event");
    let paths = event.unwrap().expect("Channel closed unexpectedly");
    let found = paths
        .iter()
        .any(|p| p.file_name() == Some(std::ffi::OsStr::new("existing.txt")));
    assert!(found, "Should detect existing.txt modification");
    Ok(())
}

#[tokio::test]
async fn test_watcher_detects_file_deletion() -> Result<()> {
    let tmp = tempdir()?;
    let watch_root = tmp.path().to_path_buf();
    let file_path = watch_root.join("todelete.txt");
    fs::write(&file_path, "will be deleted")?;

    let (tx, mut rx) = mpsc::channel(100);
    let _watcher = FileWatcher::new(watch_root.clone(), tx, Duration::from_millis(100))?;

    // Delete the file
    fs::remove_file(&file_path)?;

    let event = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(event.is_ok(), "Timed out waiting for deletion event");
    let paths = event.unwrap().expect("Channel closed unexpectedly");
    let found = paths
        .iter()
        .any(|p| p.file_name() == Some(std::ffi::OsStr::new("todelete.txt")));
    assert!(found, "Should detect todelete.txt deletion");
    Ok(())
}

#[tokio::test]
async fn test_watcher_debounce_batching() -> Result<()> {
    let tmp = tempdir()?;
    let watch_root = tmp.path().to_path_buf();

    let (tx, mut rx) = mpsc::channel(100);
    let _watcher = FileWatcher::new(watch_root.clone(), tx, Duration::from_millis(300))?;

    // Rapid-fire file changes
    for i in 0..5 {
        fs::write(watch_root.join(format!("rapid_{}.txt", i)), format!("content {}", i))?;
    }

    // With debounce, we should get fewer batches than individual changes
    let event = timeout(Duration::from_secs(3), rx.recv()).await;
    assert!(event.is_ok(), "Should receive at least one batched event");

    // The batch should contain some of our files
    let paths = event.unwrap().expect("Channel closed unexpectedly");
    assert!(!paths.is_empty(), "Batch should not be empty");
    Ok(())
}
