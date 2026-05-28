# Search — SQLite + Tantivy ハイブリッド

> Status: Draft (P3 で V2、P4 で V3)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/persistence.md`](./persistence.md), [`docs/launcher.md`](./launcher.md), [ADR 0001 (sqlite-tantivy-hybrid-search)](./adr/0001-sqlite-tantivy-hybrid-search.md)

本書は nohrs の検索アーキテクチャを定めます。ベースは設計 Gist (`https://gist.github.com/syuya2036/b47fb2fca58f9877e12255d34560f1a8`) の方針 (SQLite + Tantivy ハイブリッド) に従い、段階的に V1 → V2 → V3 と進めます。

旧 `SEARCH_IMPLEMENTATION.md` の Spotlight 一本化方針は **棄却** されました ([ADR 0001](./adr/0001-sqlite-tantivy-hybrid-search.md) 参照)。

---

## 1. アーキテクチャ概観

```text
                  ┌───────────────────────────────┐
                  │    SearchService (services)   │
                  └───┬───────────────────────┬───┘
                      │                       │
                      ▼                       ▼
              ┌──────────────┐         ┌──────────────┐
              │ SQLite (FTS5) │        │   Tantivy    │   (P4 V3 以降)
              │  metadata     │        │  full-text   │
              │  trigram      │        │  ngrams      │
              └──────┬────────┘        └──────┬───────┘
                     │                        │
                     ▼                        ▼
              ┌──────────────────────────────────┐
              │     notify-debouncer-mini        │
              │  (file watcher with debounce)    │
              └──────────────────────────────────┘
```

| 担当 | 役割 |
|------|------|
| **SQLite** | ファイルメタデータ (path, mtime, size, inode, hash)、削除追跡、状態管理、差分検出。FTS5 で trigram 全文検索 (V2) |
| **Tantivy (V3)** | 全文検索インデックスの本命、BM25 ランキング、コード対応 ngrams、identifier 分解 (camelCase / snake_case) |
| **notify-debouncer-mini** | ファイルシステム変更検出 (debounce 500ms) |

---

## 2. 段階移行

| 版 | Phase | 内容 |
|----|-------|------|
| **V1 (ripgrep)** | (現状: macOS 以外 / 暫定) | オンデマンド検索、永続インデックスなし。`ignore` + `grep` で walk + 一致 |
| **V2 (SQLite FTS5)** | **P3** | trigram tokenization、増分更新、SQLite で完結 |
| **V3 (SQLite + Tantivy)** | **P4** | BM25 + code-aware ngrams、identifier 分解、plugin から WIT 経由で使えるように |

V1 は完全に消えず、**非 macOS かつ index 未構築時のフォールバック** として残す。

---

## 3. インデックス対象とスコープ

### 3.1 対象

| パス | 動作 |
|------|------|
| `$HOME` 配下 | デフォルトで indexing 対象 |

### 3.2 デフォルト除外

| 種類 | パターン |
|------|---------|
| Build artifacts | `node_modules/`, `target/`, `dist/`, `build/` |
| Cache | `.venv/`, `__pycache__/`, `*.pyc` |
| VCS metadata | `.git/`, `.svn/` |
| OS | `.DS_Store`, `Thumbs.db` |
| Hidden | dotfiles はデフォルト除外 (設定で opt-in) |

### 3.3 `.gitignore` 尊重

`ignore` crate を使い、リポジトリの `.gitignore` / `.ignore` / `.rgignore` を尊重。

### 3.4 ユーザー設定除外

`~/.config/nohrs/config.toml`:

```toml
[indexing.exclude]
paths = ["/Users/me/HugeArchive"]
globs = ["*.iso", "*.mov"]
```

### 3.5 サイズ・種別

| 観点 | 上限 / 動作 |
|------|------------|
| 1 ファイル全文インデックス | **10 MB まで**、超過はパス・メタデータのみ |
| バイナリ判定 | `infer` crate or BOM チェック。バイナリは全文 index せずメタのみ |
| シンボリックリンク | デフォルト follow しない、設定で follow 可 |

---

## 4. コンテンツ抽出

| ファイル種別 | P3 (V2) | P4 (V3) | P5+ |
|-------------|---------|---------|-----|
| プレーンテキスト | ✅ そのまま | ✅ | ✅ |
| コード (.rs, .py, .js, ...) | ✅ そのまま (identifier 分解なし) | ✅ camelCase / snake_case 分解 + ngrams | ✅ |
| PDF | ❌ | ❌ | ✅ (`pdf-extract` crate) |
| Office (.docx, .xlsx) | ❌ | ❌ | ✅ (P9+) |
| 画像 (OCR) | ❌ | ❌ | ✅ (P9+) |
| アーカイブ (.zip, .tar.gz) | ❌ (中身までは見ない) | ❌ | ❌ |

---

## 5. クエリ構文

| 構文 | 例 | サポート |
|------|-----|---------|
| 通常テキスト | `hello world` | V1〜 |
| フレーズ | `"hello world"` | V2 から |
| Boolean | `cat AND dog`, `cat OR dog`, `cat -fish` | V2 から |
| フィールド指定 | `ext:rs todo`, `path:src/`, `name:lib*` | V2 から (FTS5 trigger + WHERE) |
| 正規表現 | `regex:fn\\s+\\w+` | V3 から (Tantivy regex query) |
| ファジー | `foo~` | V3 から (Tantivy fuzzy query) |

---

## 6. 増分更新

| 観点 | 仕様 |
|------|------|
| 初回フル indexing | 起動時にバックグラウンドで実行、ステータスバーに progress 表示 |
| ファイル変更検出 | `notify-debouncer-mini` で 500ms debounce、change event を SQLite `files.mtime_ns` と比較し変化があれば re-index |
| 削除検出 | watcher の delete event + 定期的な orphan scan (起動時 1 回 + 24h ごと) |
| concurrent indexing | rayon で並列、CPU の半分 (最大 4 thread) まで |
| index 整合性 | 起動時に lazy check (`files.content_hash` と Tantivy doc id の対応) |

---

## 7. リソース制限 (重要)

PC のリソースを過度に消費しないよう、適応的に throttle します。

### 7.1 マトリクス

| 状況 | 並列度 | thread priority |
|------|--------|----------------|
| 通常 (AC 電源 + idle + nohrs 非 frontmost) | `min(CPU/2, 4)` | UTILITY |
| バッテリー駆動 | `min(CPU/4, 2)` | UTILITY |
| LowPowerMode (macOS) / power-saver (Linux) | **indexing 停止** | — |
| nohrs が frontmost + UI 操作中 | **一時停止** (UI 操作完了 1 秒後に再開) | — |
| OS idle 検出 (>120s 無入力) | "burst mode" (CPU 数全部使う) | BACKGROUND |

### 7.2 実装メモ

| 機能 | 推奨 crate / API |
|------|----------------|
| 電源状態 | `battery` crate (`battery::State`) + macOS は `IOPSGetTimeRemainingEstimate` |
| LowPowerMode 検出 | macOS: `NSProcessInfo.isLowPowerModeEnabled`、Linux: GNOME `org.freedesktop.PowerProfiles` D-Bus |
| Idle 時間 | macOS: `CGEventSourceSecondsSinceLastEventType` |
| Thread QoS | macOS: `pthread_set_qos_class_self_np` (QoS class)、Linux: `setpriority`/`nice` (スケジューリング優先度)、必要なら `ioprio_set` で IO 優先度。§7.1 の thread priority 列に対応 |
| アプリ前面状態 | GPUI の `window.is_active()` |

`crates/nohrs-core/src/resource_policy.rs` に `ResourcePolicy::current() -> ResourcePolicy` を提供、cfg で OS 別実装。

### 7.3 I/O / メモリ バックプレッシャ

| カテゴリ | 制限 |
|---------|------|
| scanner → indexer channel | `bounded(64)` で scanner が waiting |
| indexing パイプラインバッファ合計 | 100 MB 上限 |
| Tantivy index ディスク使用量 | 1 GB で警告、5 GB で indexing 一時停止 (ユーザー通知) |
| 1 値サイズ | (KV 系) 1 MB 上限 |

### 7.4 ユーザー制御

| 設定 | 効果 |
|------|------|
| `[indexing] mode = "auto"` (default) | 上記マトリクスに従う |
| `[indexing] mode = "always-on"` | throttle 無し (古い PC では非推奨) |
| `[indexing] mode = "manual"` | UI ボタンで明示起動するまで走らない |

ステータスバー (`src/ui/components/layout/footer.rs`) に `Indexing: 1,234 / 5,678 files` + ON/OFF/Pause トグル。

---

## 8. 検索 UI

| 接点 | スコープ |
|------|---------|
| **ランチャー (`Cmd+Shift+Space`)** | グローバル全文検索 (全 indexed scope) |
| **Explorer 内検索バー (`Cmd+F`)** | active pane の current dir 配下のみ scope |

### 検索結果から遷移

| 操作 | 動作 |
|------|------|
| `Enter` | reveal in explorer (該当ファイルを explorer で開いてハイライト) |
| `Cmd+Enter` | default editor で開く |
| `Cmd+Shift+C` | パスをクリップボードにコピー |

---

## 9. plugin への公開 (P4)

WIT `search` interface (P4 で追加):

```wit
interface search {
  record search-hit {
    path: string,
    score: f32,
    snippet: option<string>,
  }
  search-files: func(query: string, limit: u32) -> list<search-hit>;
  search-content: func(query: string, limit: u32) -> list<search-hit>;
}
```

plugin の permission `read_paths` の範囲内でのみヒット返す (host 側で post-filter)。詳細は [`docs/plugin-api.md`](./plugin-api.md)。
