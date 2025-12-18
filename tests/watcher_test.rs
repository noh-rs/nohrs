use anyhow::Result;
use nohrs::services::search::watcher::FileWatcher;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;
use tokio::time::timeout;

#[tokio::test]
async fn test_watcher_detects_changes() -> Result<()> {
    let temp_root = tempdir()?;
    let watch_root = temp_root.path().to_path_buf();

    let (tx, mut rx) = mpsc::channel(100);

    // Short timeout for fast tests
    let _watcher = FileWatcher::new(watch_root.clone(), tx, Duration::from_millis(100))?;

    // Create new file
    let file_path = watch_root.join("new_file.txt");
    fs::write(&file_path, "Initial content")?;

    // Wait for event (with safety timeout)
    let event = timeout(Duration::from_secs(2), rx.recv()).await;

    assert!(event.is_ok(), "Timed out waiting for watcher event");
    let paths = event.unwrap().expect("Channel closed unexpected");

    // Notify might coalesce events or send multiple.
    // We expect at least our file path.
    let found = paths
        .iter()
        .any(|p| p.file_name() == Some(std::ffi::OsStr::new("new_file.txt")));
    assert!(found, "Should detect new_file.txt creation/write");

    Ok(())
}
