# Configuration

> Status: Draft (P1 で最小実装、P2 で拡張)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/persistence.md`](./persistence.md)

本書はユーザー設定ファイル `config.toml` の配置・スキーマ・ロード戦略・hot reload を定めます。

---

## 1. ファイル配置 (XDG 準拠)

| データ種別 | 場所 |
|-----------|------|
| **config** | `$XDG_CONFIG_HOME/nohrs/config.toml` (Linux/macOS: `~/.config/nohrs/config.toml`) |
| **SQLite DB** | `$XDG_DATA_HOME/nohrs/db.sqlite` |
| **Tantivy index** | `$XDG_DATA_HOME/nohrs/index/` |
| **redb (plugin KV)** | `$XDG_DATA_HOME/nohrs/plugin-kv.redb` |
| **ログ / キャッシュ** | `$XDG_CACHE_HOME/nohrs/` |
| **plugin インストール先** | `$XDG_DATA_HOME/nohrs/plugins/<plugin-id>/` |
| **window position など高頻度更新** | SQLite `key_value` テーブル (config.toml ではない) |

XDG 環境変数 `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME` / `$XDG_CACHE_HOME` を直接解決し、未設定時は `~/.config` / `~/.local/share` / `~/.cache` にフォールバックして上記を組み立てる。`dirs::config_dir()` / `data_dir()` / `cache_dir()` は **使わない**: macOS では `~/Library/Application Support` 等の OS ネイティブパスを返してしまい、本プロジェクトが全プラットフォームで意図する XDG スタイルの dotfile パス (`~/.config` など) と矛盾するため。

---

## 2. P1 最小スキーマ

```toml
schema_version = 1

[theme]
mode = "system"   # "light" | "dark" | "system"
accent = "blue"   # 名前 or hex (P5 で完全カスタマイズ)

[ui]
default_sort = "name"      # "name" | "modified" | "size" | "kind"
show_hidden = false
icon_pack = "default"

[keybindings]
# P3 で本格化。P1 では空、デフォルトは built-in。

[plugins]
# P4 で本格運用。P1 では空。
# core = ["git", "calculator"]
# community = ["user/repo", "https://..."]
```

### P2 以降の拡張

```toml
[indexing]
mode = "auto"  # "auto" | "always-on" | "manual"

[indexing.exclude]
paths = []
globs = []

[search]
backend = "auto"  # "sqlite-fts" | "tantivy" | "ripgrep" | "auto"

[launcher]
hotkey = "Cmd+Shift+Space"
position_remember = true
```

---

## 3. ロード戦略 (4 層 override)

| 優先度 | 層 | 例 |
|-------|----|----|
| 最低 | デフォルト値 (コード内 `Default::default()`) | — |
| 中 | 設定ファイル | `~/.config/nohrs/config.toml` |
| 高 | 環境変数 | `NOHRS_THEME=dark` |
| 最高 | CLI 引数 | `--theme dark` |

実装は `serde` + 手動マージで P1 は十分 (`figment` 等の依存追加は不要)。

```rust
// 擬似コード
let defaults = Config::default();
let from_file = Config::load_toml(path)?;
let from_env  = Config::from_env();
let from_cli  = Config::from_clap(args);

let merged = defaults
    .merged_with(from_file)
    .merged_with(from_env)
    .merged_with(from_cli);
```

---

## 4. JSON Schema 生成

`schemars` crate で Rust 構造体から JSON Schema を自動生成し、`docs/config.schema.json` を生成。

```bash
cargo run --bin nohrs -- config schema > docs/config.schema.json
```

config.toml の冒頭に:

```toml
#:schema https://nohrs.app/schema/config.schema.json
```

を埋め込み、VS Code (Even Better TOML 拡張) 等でオートコンプリート対応。

---

## 5. Hot Reload

| 項目 | hot reload | 実装フェーズ | 備考 |
|------|-----------|-------------|------|
| `theme.mode` | ✅ Yes | **P1** | `notify` でファイル監視、即時反映 |
| `theme.accent` | ✅ Yes | **P1** | 同上 |
| `ui.default_sort` | ✅ Yes | **P1** | 開いているビューに伝播 |
| `ui.show_hidden` | ✅ Yes | **P1** | 開いているビューに伝播 |
| `ui.icon_pack` | ✅ Yes | **P1** | アイコンキャッシュをクリア |
| `keybindings.*` | ❌ No (再起動) | P3 で hot reload に格上げ検討 | キーマップは入力ハンドラに焼き込まれているため |
| `plugins.enabled` | ❌ No (再起動) | P5 で動的 enable/disable に格上げ検討 | wasm host のライフサイクル安定化が前提 |
| `schema_version` | ❌ No (再起動、マイグレーション処理走る) | 永続 | 変更は版アップに伴う |

**実装メモ**: P1 で `notify` を最小利用 (`nohrs-core::config::watcher`)。検索系の watcher は P3 で復活するが、config 監視は別パスで先行。

---

## 6. バリデーション

| 種類 | 動作 |
|------|------|
| **構文エラー (parse 失敗)** | 起動時に **panic せず** デフォルト config で起動。エラーメッセージを UI のステータスバーに表示し、ファイルパスを案内 |
| **未知の key** | warn ログ + 無視 (forward-compat) |
| **未知の `schema_version`** | 起動時に「nohrs の新しいバージョンが必要かもしれません」を表示、デフォルト config にフォールバック |
| **値の範囲外** | warn ログ + デフォルト値を使う |

---

## 7. マイグレーション

`schema_version` を bump するときは:

1. 旧 schema の serde 構造体を別 module (`legacy_v1`) に残す
2. ロード時に `schema_version` を peek して該当 module で deserialize
3. `Migration::v1_to_v2()` で新 schema に変換、`config.toml` を上書き保存
4. 上書き前にバックアップ (`config.toml.bak-<timestamp>`) を作成

---

## 8. CLI サブコマンド (P1 から)

```bash
nohrs config show          # 現在の merged config を JSON で表示
nohrs config edit          # $EDITOR で config.toml を開く
nohrs config validate      # schema 検証
nohrs config schema        # JSON Schema を stdout に
nohrs config path          # config.toml のフルパスを表示
nohrs config reset         # config を初期化 (backup を取る)
```
