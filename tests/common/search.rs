use nohrs::services::search::indexer::IndexManager;
use std::path::Path;
use tempfile::TempDir;

/// テスト用 IndexManager を tempdir ベースで生成
pub fn test_index_manager(content_dir: &Path) -> (IndexManager, TempDir) {
    let index_dir = TempDir::new().unwrap();
    let manager = IndexManager::new_with_path(index_dir.path().to_path_buf(), content_dir.to_path_buf())
        .unwrap();
    (manager, index_dir)
}
