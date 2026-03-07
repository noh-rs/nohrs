use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// アプリケーション設定 (~/.nohrs/nohrs.toml)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub index: IndexConfig,
    pub search: SearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IndexConfig {
    /// インデックス対象ディレクトリ一覧 (デフォルト: ["~/Documents"])
    pub directories: Vec<String>,
    /// インデックスから除外するパターン
    pub exclude_patterns: Vec<String>,
    /// ファイルサイズ上限 (バイト, デフォルト: 10MB)
    pub max_file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchConfig {
    /// デバウンス待機時間 (ミリ秒, デフォルト: 300)
    pub debounce_ms: u64,
    /// 1ページあたりの最大結果数 (デフォルト: 200)
    pub max_results_per_page: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            index: IndexConfig::default(),
            search: SearchConfig::default(),
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            directories: vec!["~/Documents".to_string()],
            exclude_patterns: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
            ],
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 300,
            max_results_per_page: 200,
        }
    }
}

impl AppConfig {
    /// 設定ファイルのデフォルトパスを返す
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".nohrs").join("nohrs.toml"))
    }

    /// ファイルから読み込む。ファイルが存在しない場合はデフォルトを返す
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from(&path)
    }

    /// 指定パスから読み込む
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config: {}", path.display()))?;
        Ok(config)
    }

    /// インデックス対象ディレクトリを展開済みの PathBuf リストで返す
    pub fn resolved_index_directories(&self) -> Vec<PathBuf> {
        let home = dirs::home_dir().unwrap_or_default();
        self.index
            .directories
            .iter()
            .map(|d| {
                if d.starts_with("~/") {
                    home.join(&d[2..])
                } else if d == "~" {
                    home.clone()
                } else {
                    PathBuf::from(d)
                }
            })
            .filter(|p| p.exists())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.index.directories, vec!["~/Documents".to_string()]);
        assert_eq!(config.search.debounce_ms, 300);
        assert_eq!(config.search.max_results_per_page, 200);
        assert_eq!(config.index.max_file_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let config = AppConfig::load_from(Path::new("/tmp/nonexistent_nohrs_config.toml")).unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn test_load_partial_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nohrs.toml");
        std::fs::write(
            &path,
            r#"
[index]
directories = ["~/Projects", "~/Code"]

[search]
debounce_ms = 500
"#,
        )
        .unwrap();

        let config = AppConfig::load_from(&path).unwrap();
        assert_eq!(
            config.index.directories,
            vec!["~/Projects".to_string(), "~/Code".to_string()]
        );
        assert_eq!(config.search.debounce_ms, 500);
        // デフォルト値が残る
        assert_eq!(config.search.max_results_per_page, 200);
    }

    #[test]
    fn test_resolved_index_directories_expands_tilde() {
        let config = AppConfig::default();
        let dirs = config.resolved_index_directories();
        // ~/Documents が存在するかはOS依存なので、展開されていることだけ検証
        for dir in &dirs {
            assert!(!dir.to_string_lossy().starts_with("~/"));
        }
    }

    #[test]
    fn test_load_with_exclude_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nohrs.toml");
        std::fs::write(
            &path,
            r#"
[index]
exclude_patterns = ["*.log", "dist"]
"#,
        )
        .unwrap();

        let config = AppConfig::load_from(&path).unwrap();
        assert_eq!(
            config.index.exclude_patterns,
            vec!["*.log".to_string(), "dist".to_string()]
        );
    }
}
