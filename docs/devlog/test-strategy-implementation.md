# テスト戦略 Phase 1–4 実装メモ

日付: 2026-03-05

## 概要

`docs/testing/test-strategy.md` に従い、Phase 1–4 のテスト基盤を構築した。

---

## Phase 1: 純粋ロジックの feature gate 解除と単体テスト

### 型の移動 (1-1)

`src/pages/explorer/types.rs` から GUI 非依存の型を `src/core/types.rs` に移動。

| 型 | 移動先 |
|---|---|
| `SortKey` | `src/core/types.rs` |
| `SearchType` | `src/core/types.rs` |
| `SearchMatch` | `src/core/types.rs` |
| `SearchFileResult` | `src/core/types.rs` |

移動元には `pub use` による再エクスポートを配置し、既存コードの互換性を維持。
`ViewMode`, `ResizingColumn`, `LastClickInfo` は GUI 依存のため `pages/explorer/types.rs` に残留。

### 純粋ロジック関数の移動 (1-2)

| 関数 | 移動元 → 移動先 |
|---|---|
| `sort_entries`, `get_extension` | `pages/explorer/entries.rs` → `src/core/sort.rs` |
| `group_results`, `results_to_entries` | `pages/explorer/search.rs` → `src/core/search_utils.rs` |
| `truncate_middle` | `pages/explorer/view/listing/mod.rs` → `src/core/text.rs` |

移動元には再エクスポートを配置。

### find_query_highlights のロジック分離 (1-3)

`find_query_match_ranges()` を `src/core/text.rs` に新規作成（純粋ロジック）。
`find_query_highlights()` は `pages/explorer/view/mod.rs` に残し、`HighlightStyle` 付与の薄いラッパーとして実装。

### 単体テスト (1-4)

`tests/core_tests.rs` + `tests/core_tests/` に配置:

- `sort_test.rs`: 9 テスト
- `search_utils_test.rs`: 7 テスト
- `text_test.rs`: 10 テスト

**合計: 26 テスト** (`cargo test` で実行可能、GUI feature 不要)

---

## Phase 2: 検索サービスの統合テスト強化

`tests/service_tests.rs` + `tests/service_tests/` に配置:

- `indexing_test.rs`: 7 テスト（インデックス作成・検索・更新・削除・日本語・大量ファイル・空ディレクトリ）
- `ripgrep_test.rs`: 5 テスト（基本検索・大文字小文字区別・バイナリスキップ・行番号・gitignore）
- `search_engine_test.rs`: 3 テスト（インデックス検索・ripgrep 検索・進捗トラッキング）
- `watcher_test.rs`: 4 テスト（ファイル作成・変更・削除・デバウンス）

**合計: 19 テスト** (`cargo test` で実行可能、GUI feature 不要)

### テストヘルパー

`tests/common/` に共通ヘルパーを配置:
- `fixtures.rs`: `TestFileTree` — テスト用ファイルツリー生成
- `search.rs`: `test_index_manager()` — tempdir ベースの IndexManager ファクトリ

---

## Phase 3: UI ロジックテスト（ExplorerPage エンティティテスト）

`src/pages/explorer/tests.rs` に `#[cfg(test)]` モジュールとして配置。

### SearchProvider トレイト抽出 (Phase 4-0 を先行実施)

テスト可能にするため、`Arc<SearchService>` を `Arc<dyn SearchProvider>` に変更。

```rust
// src/services/search/mod.rs
pub trait SearchProvider: Send + Sync {
    fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>>;
}
```

テスト側に `MockSearchProvider` を実装し、テストデータのマッピングで返却値を制御可能にした。

### テストヘルパー

`build_explorer()` / `build_explorer_default()` を実装。
`TestAppContext::add_window_view()` を使い、ウィンドウコンテキスト付きで `ExplorerPage` を生成。

**重要な発見**: `gpui` 0.2.2 の `TestAppContext` は `#[cfg(any(test, feature = "test-support"))]` で gated されており、`Cargo.toml` の `[dev-dependencies]` に `gpui = { version = "0.2", features = ["test-support"] }` を追加する必要があった。

### エンティティテスト

`tests.rs` 直下に 7 テスト:
- `test_initial_state` — 初期状態の検証
- `test_reload_loads_entries` — ファイル一覧読み込み
- `test_sort_key_change` — ソートキー切替と方向トグル
- `test_select_entry` — 選択状態
- `test_view_mode_toggle` — List/Grid 切替
- `test_column_resize` — カラムリサイズ
- `test_column_resize_min_width` — 最小幅制約

### ナビゲーションテスト

`tests/navigation_test.rs` に 3 テスト:
- `test_navigate_to_updates_entries` — ディレクトリ変更で entries 更新
- `test_navigate_adds_to_history` — 履歴スタック追加
- `test_go_back_and_forward` — 戻る/進むナビゲーション

### ソートテスト

`tests/sort_test.rs` に 2 テスト:
- `test_sort_entries_applied_on_reload` — 名前昇順ソート
- `test_sort_key_toggle_direction` — トグルで降順切替

---

## Phase 4: 検索 UI の結合テスト

### 検索フローテスト

`tests/search_flow_test.rs` に 6 テスト:
- `test_search_flow_basic` — 基本検索フロー（open → query → trigger → verify results）
- `test_search_no_results` — 結果なし検索
- `test_search_error_handling` — エラー時のフォールバック
- `test_search_clear_restores_listing` — 検索クローズで元一覧に復帰
- `test_search_empty_query_clears_results` — 空クエリで結果クリア
- `test_search_result_activate_dir` — ディレクトリエントリのアクティベーション

### 検索スコープ/タイプテスト

`tests/search_scope_test.rs` に 3 テスト:
- `test_search_scope_switch` — Home/Root スコープ切替
- `test_search_type_switch` — Filename/Content/All タイプ切替
- `test_match_option_toggles` — match_case/match_whole_word/use_regex トグル

### 検索状態遷移テスト

`tests/search_state_test.rs` に 5 テスト:
- `test_search_state_lifecycle` — 完全ライフサイクル（初期→表示→検索→クローズ→初期）
- `test_dir_change_clears_search` — ディレクトリ移動で検索クリア
- `test_search_expanded_files_state` — ファイル展開/折りたたみ状態
- `test_toggle_search` — 検索バートグル
- `test_open_search_idempotent` — 冪等な検索バー表示

**Phase 3 + 4 合計: 26 テスト** (`cargo test --features gui` で実行可能)

---

## 発見・修正したバグ

### 1. RipgrepBackend バイナリ検出漏れ

**場所**: `src/services/search/ripgrep.rs`

**問題**: `Searcher::new()` がデフォルトのバイナリ検出なしで使用されており、バイナリファイルの内容が検索結果に含まれる可能性があった。

**修正**:
```rust
// Before
let mut searcher = Searcher::new();

// After
let mut searcher = SearcherBuilder::new()
    .binary_detection(BinaryDetection::quit(b'\x00'))
    .line_number(true)
    .build();
```

### 2. close_search() の重複 apply_filter() 呼び出し

**場所**: `src/pages/explorer/mod.rs` (close_search メソッド)

**問題**: `apply_filter()` が 2 回連続で呼ばれていた（525行目と526行目）。2回目の呼び出しは冗長。

**修正**: 重複呼び出しを削除。

### 3. apply_filter() の冗長クローン

**場所**: `src/pages/explorer/mod.rs` (apply_filter メソッド)

**問題**: `search_query` が空で `search_results` が `None` の場合、`filtered_entries` が2回クローンされていた（319行目で `entries.clone()` → 339行目で上書き）。

**修正**: メソッドを再構造化し、`search_results` の有無で分岐するシンプルなロジックに変更。

```rust
fn apply_filter(&mut self) {
    if self.search_results.is_some() {
        self.update_item_sizes();
        return;
    }
    self.filtered_entries = self.entries.clone();
    entries::sort_entries(&mut self.filtered_entries, self.sort_key, self.sort_asc);
    self.update_item_sizes();
}
```

### 4. truncate_middle のエッジケース

**場所**: `src/core/text.rs`

**問題**: `max_len < 4` の場合にオーバーフロー（saturating_sub 未使用）の可能性があった。

**修正**: `saturating_sub` を使用し、`max_len < 4` の場合は先頭からの切り詰めにフォールバック。

### 5. PreviewEditor のコンパイルエラー（pre-existing）

**場所**: `src/pages/explorer/view/preview/editor.rs`

**問題**: `scroll_to` は `gpui-component` 0.3.1 で `pub(crate)` のためアクセス不可。`set_search_query` は `InputState` に存在しない。

**修正**: 両メソッドを no-op にスタブ化（TODO コメント付き）。

---

## テスト実行環境の制約

### `cargo test`（feature gate なし）
- **56 テスト** が全てパス
- core (26) + service (19) + legacy (11) テスト

### `cargo test --features gui`
- `cargo check --features gui --tests` でコンパイル確認済み
- リンクには `xkbcommon` 等のシステムライブラリが必要（CI の ubuntu-latest では `apt install libxkbcommon-dev` が必要）
- **26 テスト** が追加される（Phase 3 + 4）

### CI 設定推奨

```yaml
test-core:
  runs-on: ubuntu-latest
  steps:
    - run: cargo test

test-gui:
  runs-on: ubuntu-latest
  needs: test-core
  steps:
    - run: sudo apt-get install -y libxkbcommon-dev libxkbcommon-x11-dev
    - run: cargo test --features gui
```

---

## ファイル変更一覧

### 新規作成
- `src/core/types.rs` — GUI 非依存の型定義
- `src/core/sort.rs` — ソートロジック
- `src/core/search_utils.rs` — 検索結果グルーピング・変換
- `src/core/text.rs` — 文字列処理（truncate_middle, find_query_match_ranges）
- `src/pages/explorer/tests.rs` — Phase 3/4 テストルート + MockSearchProvider
- `src/pages/explorer/tests/navigation_test.rs`
- `src/pages/explorer/tests/sort_test.rs`
- `src/pages/explorer/tests/search_flow_test.rs`
- `src/pages/explorer/tests/search_scope_test.rs`
- `src/pages/explorer/tests/search_state_test.rs`
- `tests/core_tests.rs` + `tests/core_tests/` (Phase 1 テスト)
- `tests/service_tests.rs` + `tests/service_tests/` (Phase 2 テスト)
- `tests/common/` (テストヘルパー)

### 変更
- `src/core/mod.rs` — 新モジュール宣言追加
- `src/pages/explorer/mod.rs` — `Arc<dyn SearchProvider>` への変更、バグ修正、`#[cfg(test)] mod tests`
- `src/pages/explorer/types.rs` — 再エクスポート + `ViewMode` に `Debug` derive 追加
- `src/pages/explorer/entries.rs` — 再エクスポート
- `src/pages/explorer/search.rs` — 再エクスポート
- `src/pages/explorer/view/mod.rs` — `find_query_highlights` を薄いラッパーに変更
- `src/pages/explorer/view/listing/mod.rs` — `truncate_middle` を再エクスポート
- `src/services/search/mod.rs` — `SearchProvider` トレイト追加
- `src/services/search/ripgrep.rs` — バイナリ検出修正
- `Cargo.toml` — `[dev-dependencies]` に `gpui` test-support 追加

---

## 未実装・今後の課題

1. **`preview_test.rs`**: test-strategy.md に記載があるが、PreviewEditor のスタブ化により現時点では有意義なテストが書けない。PreviewEditor の API が安定してから追加する。
2. **キーストロークベースの UI イベントテスト** (Phase 3-2, 4-5): `simulate_keystrokes` を使ったキーバインド検証は、ExplorerPage のキーバインド実装が確立してから追加する。
3. **旧テストファイルの整理**: `tests/indexing_test.rs` と `tests/watcher_test.rs` は `tests/service_tests/` に統合済みだが、旧ファイルもまだ残っている。
