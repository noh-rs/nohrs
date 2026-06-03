# Configuration

> Status: P1 最小実装 + P2 拡張 (indexing / search / launcher 実装、keybindings / plugins 予約)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/persistence.md`](./persistence.md)

本書はユーザー設定ファイル `config.toml` の配置・スキーマ・ロード戦略・hot reload を定めます。

---

## 1. ファイル配置 (XDG 準拠)

| データ種別 | 場所 |
|-----------|------|
| **config** | `$XDG_CONFIG_HOME/nohrs/config.toml` (Linux/macOS: `~/.config/nohrs/config.toml`) |
| **SQLite DB** (メタデータ・履歴) | `$XDG_DATA_HOME/nohrs/db.sqlite` |
| **redb (ホスト KV, P2)** | `$XDG_DATA_HOME/nohrs/state.redb` |
| **Tantivy index** | `$XDG_DATA_HOME/nohrs/index/` |
| **redb (plugin KV, P4)** | `$XDG_DATA_HOME/nohrs/plugin-kv.redb` |
| **ログ / キャッシュ** | `$XDG_CACHE_HOME/nohrs/` |
| **plugin インストール先** | `$XDG_DATA_HOME/nohrs/plugins/<plugin-id>/` |
| **window position・タブ/セッション復元など高頻度更新** | redb `state.redb` (config.toml ではない、詳細は [`docs/persistence.md`](./persistence.md)) |

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
# 草案 (P3 で本格化)。`action = "chord"` の自由マップ。空ならデフォルトは built-in。
# 任意のアクション名を forward-compat のため warn なしで受理 (値は string のみ)。
# quit = "ctrl-q"

[plugins]
# 型予約のみ (P4 で本格運用)。リストは parse/格納されるが host はまだロードしない。
# core = ["git", "calculator"]
# community = ["user/repo", "https://..."]
```

`[keybindings]` と `[plugins]` は型として定義済みだが、値はまだ挙動に反映されない (hot reload は再起動扱い、§5)。スキーマには予約済みで、P3/P4 で形を拡張しても古いファイルは壊れない。

> **ランタイム反映状況**: 現状でランタイムに反映されるのは `theme` / `ui` / `diagnostics` のみ。`indexing` / `search` / `launcher` / `keybindings` / `plugins` は **parse + validate されるが、まだ挙動には反映されない** (各サブシステムが配線される後続フェーズで有効化)。schema / template に先行して載せているのは、ファイルとエディタ補完が形を先取りできるようにするため。編集しても今は効果がない点に注意。
>
> **`required` なし**: 全フィールドが `#[serde(default)]`。ローダは `Config::default()` から開始して存在するキーだけ上書きするため、どのセクションを省略しても (空の `config.toml` でも) 受理される。生成スキーマも `required` を持たず、エディタ検証とローダ挙動が一致する。

### P2 拡張 (schema 定義済み / ランタイムは後続フェーズ)

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

[diagnostics.store]
# パフォーマンス解析用のストア操作ログ。デフォルト全 off (本番では無音)。
# 出力は tracing 経由 (target = "nohrs_store::sql" / "nohrs_store::redb")、RUST_LOG で絞り込み可。
# 詳細は docs/persistence.md §5。
log_all_queries = false   # 全 SQL クエリを debug で記録 (verbose)
slow_query_ms   = 0       # >0 のとき、この閾値(ms)超過の SQL クエリを warn で記録 (0 = 無効)
log_redb_ops    = false   # redb の get/put/delete/batch 操作と所要時間を記録
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

`schemars` crate で Rust 構造体 (`Config` 以下) から JSON Schema を自動生成し、`docs/config.schema.json` にコミットする。出力は安定化済み (キーソート + Rust 固有の integer `format` を除去) なので byte-for-byte で再現できる。

```bash
# GUI バイナリ経由 (macOS):
cargo run --bin nohrs -- config schema > docs/config.schema.json

# GUI 非依存 (Linux/CI、上と同一バイト列を出力):
cargo run -p nohrs-core --example schema > docs/config.schema.json
```

**CI で常に最新を保証**: `config schema` ジョブが上記 example でスキーマを再生成し、コミット済みファイルと `git diff --exit-code` する。構造体を変更してスキーマ再生成を忘れると、diff 付きで CI が落ちる。加えて `nohrs-core` のユニットテスト (`committed_schema_is_up_to_date`) が同じ検証をローカル/`cargo test` でも行う。

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
| `diagnostics.store.*` | ❌ No (再起動) | P3 で hot reload に格上げ検討 | ストア接続 open 時に profile フックの登録可否を決めるため (off 時はフック未登録でオーバーヘッド 0) |
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
