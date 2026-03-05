use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// テスト用ファイルツリーを生成
pub struct TestFileTree {
    pub dir: TempDir,
}

impl TestFileTree {
    /// 基本ツリー: dir_a/, dir_b/, file1.txt, file2.rs, image.png
    pub fn basic() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("dir_a")).unwrap();
        fs::create_dir_all(root.join("dir_b")).unwrap();
        fs::write(root.join("file1.txt"), "Hello world").unwrap();
        fs::write(root.join("file2.rs"), "fn main() {}").unwrap();
        fs::write(root.join("image.png"), &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        Self { dir }
    }

    /// 検索テスト用: 複数ファイルに検索可能なコンテンツを配置
    pub fn for_search() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    println!(\"hello world\");\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn search() {\n    // search implementation\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("readme.txt"),
            "This is a readme file.\nIt contains search terms.\n",
        )
        .unwrap();

        Self { dir }
    }

    /// 大量ファイル: N個のファイルを生成
    pub fn large(count: usize) -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for i in 0..count {
            fs::write(root.join(format!("file_{:04}.txt", i)), format!("Content {}", i)).unwrap();
        }

        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}
