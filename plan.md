# テスト戦略

## 前提

- デスクトップ環境(GPU/ウィンドウシステム)不要で `cargo test` のみで完結する
- CI / LLM エージェントが headless 環境で実行可能
- GPUI の `TestAppContext` を活用し、UI ロジックもヘッドレスでテスト可能にする

---

## テストファイル配置

### 設計方針

- **`tests/`（integration tests）**: feature gate なし（`cargo test`）で実行可能なテスト
- **`src/` 内 `#[cfg(test)]` モジュール**: GPUI エンティティテスト（`cargo test --features gui`）
- ページ追加時はそのページ配下に `tests.rs` を置くパターンを踏襲

### ディレクトリ構成

```
tests/                                  # integration tests（feature gate なし）
├── core/                               # Phase 1: 純粋ロジック単体テスト
│   ├── mod.rs                          # mod 宣言
│   ├── sort_test.rs                    # sort_entries, get_extension
│   ├── search_utils_test.rs            # group_results, results_to_entries
│   └── text_test.rs                    # truncate_middle, find_query_match_ranges
│
├── services/                           # Phase 2: 検索サービス統合テスト
│   ├── mod.rs
│   ├── indexing_test.rs                # ← 既存 tests/indexing_test.rs を移動
│   ├── ripgrep_test.rs                 # RipgrepBackend
│   ├── search_engine_test.rs           # SearchEngine (tempdir ベース)
│   └── watcher_test.rs                 # ← 既存 tests/watcher_test.rs を移動
│
├── helpers/                            # テストユーティリティ（全テスト共通）
│   ├── mod.rs
│   ├── fixtures.rs                     # テスト用ファイルツリー生成
│   └── search.rs                       # SearchService/IndexManager テスト用ファクトリ
│
└── lib.rs                              # tests/ のルート（mod core; mod services; mod helpers;）

src/pages/explorer/
├── tests.rs                            # Phase 3: ExplorerPage エンティティテスト (#[gpui::test])
└── tests/                              # Phase 4: 検索 UI 結合テスト (#[gpui::test])
    ├── mod.rs                          # mod 宣言 + テスト用ヘルパー
    ├── navigation_test.rs              # ディレクトリ移動・履歴
    ├── sort_test.rs                    # ソート切替
    ├── search_flow_test.rs             # 検索 E2E フロー
    ├── search_scope_test.rs            # スコープ/タイプ切替
    ├── search_state_test.rs            # 検索状態遷移
    └── preview_test.rs                 # プレビュー表示
```

### 配置の根拠

| 場所 | 対象 | 理由 |
|------|------|------|
| `tests/core/` | 純粋ロジック | GUI feature 不要。CI の高速パスで回せる |
| `tests/services/` | 検索サービス | tempdir + tokio で完結。GUI 不要 |
| `tests/helpers/` | テスト共通コード | fixture 生成や SearchService ファクトリを集約 |
| `src/pages/explorer/tests.rs` | エンティティ単体 | `pub(super)` フィールドに直接アクセスが必要 |
| `src/pages/explorer/tests/` | UI 結合テスト | テスト数が増えたらディレクトリ化。機能単位で分割 |

### テストヘルパー

```rust
// tests/helpers/fixtures.rs
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// テスト用ファイルツリーを生成
pub struct TestFileTree {
    pub dir: TempDir,
}

impl TestFileTree {
    /// 基本ツリー: dir_a/, dir_b/, file1.txt, file2.rs, image.png
    pub fn basic() -> Self { ... }

    /// 検索テスト用: 複数ファイルに検索可能なコンテンツを配置
    pub fn for_search() -> Self { ... }

    /// 大量ファイル: N個のファイルを生成（性能テスト用）
    pub fn large(count: usize) -> Self { ... }

    pub fn path(&self) -> &Path { self.dir.path() }
}
```

```rust
// tests/helpers/search.rs
use nohrs::services::search::indexer::IndexManager;
use tempfile::TempDir;

/// テスト用 IndexManager を tempdir ベースで生成
pub fn test_index_manager(content_dir: &Path) -> (IndexManager, TempDir) {
    let index_dir = TempDir::new().unwrap();
    let manager = IndexManager::new_with_path(
        index_dir.path().to_path_buf(),
        content_dir.to_path_buf(),
    ).unwrap();
    (manager, index_dir)
}
```

### 将来のページ追加時のパターン

新ページ `src/pages/git/` を追加する場合:

```
src/pages/git/
├── mod.rs
├── tests.rs            # エンティティテスト
└── tests/              # テストが増えたらディレクトリ化
    ├── mod.rs
    ├── commit_test.rs
    └── diff_test.rs
```

`tests/` 側にも対応するサービステストを追加:

```
tests/services/
└── git_service_test.rs
```

---

## Phase 1: 純粋ロジックの feature gate 解除と単体テスト

### 1-1. 型の移動

`src/pages/explorer/types.rs` から GUI 非依存の型を `src/core/types.rs` に移動する。

**移動対象:**
- `SortKey` — GUI 依存なし
- `SearchType` — GUI 依存なし
- `SearchMatch` — GUI 依存なし
- `SearchFileResult` — GUI 依存なし

**残留（pages/explorer/types.rs）:**
- `ViewMode` — pages 内でのみ使用されるため残す
- `ResizingColumn` — `gpui::Pixels`, `gpui::Point` に依存 → 残す
- `LastClickInfo` — UI 固有 → 残す

### 1-2. 純粋ロジック関数の移動

| 関数 | 移動元 | 移動先 | 理由 |
|------|--------|--------|------|
| `sort_entries()` | `pages/explorer/entries.rs` | `src/core/sort.rs` | SortKey + FileEntryDto のみ依存 |
| `get_extension()` | `pages/explorer/entries.rs` | `src/core/sort.rs` | sort_entries の補助関数 |
| `group_results()` | `pages/explorer/search.rs` | `src/core/search_utils.rs` | SearchResult → SearchFileResult の変換ロジック |
| `results_to_entries()` | `pages/explorer/search.rs` | `src/core/search_utils.rs` | SearchFileResult → FileEntryDto の変換ロジック |
| `truncate_middle()` | `pages/explorer/view/listing/mod.rs` | `src/core/text.rs` | 純粋な文字列処理 |

### 1-3. find_query_highlights のロジック分離

現在 `find_query_highlights()` は `gpui::HighlightStyle` を返すため GUI 依存。
**マッチ位置の算出ロジック**と**スタイル適用**を分離する。

```rust
// src/core/text.rs
/// クエリにマッチするバイト範囲を返す（純粋ロジック）
pub fn find_query_match_ranges(text: &str, query: &str) -> Vec<Range<usize>> { ... }

// src/pages/explorer/view/mod.rs（GUI 側）
/// マッチ範囲に HighlightStyle を付与する薄いラッパー
pub fn find_query_highlights(text: &str, query: &str) -> Vec<(Range<usize>, HighlightStyle)> {
    find_query_match_ranges(text, query)
        .into_iter()
        .map(|range| (range, highlight_style()))
        .collect()
}
```

### 1-4. 単体テスト追加

**`tests/core/sort_test.rs`:**
```
- sort_entries: Name 昇順/降順
- sort_entries: Size ソート
- sort_entries: Modified ソート
- sort_entries: Type ソート（拡張子別）
- sort_entries: ディレクトリが常に先頭
- sort_entries: 空リスト
- get_extension: file/dir/unknown 各ケース
```

**`tests/core/search_utils_test.rs`:**
```
- group_results: 同一ファイルの結果がグループ化される
- group_results: 異なるファイルが別グループになる
- group_results: line_number=0 のマッチはスキップされる
- group_results: 結果が path 順にソートされる
- group_results: 空入力
- results_to_entries: tempdir の実在ファイルからメタデータ取得
- results_to_entries: 存在しないファイルでもパニックしない
```

**`tests/core/text_test.rs`:**
```
- truncate_middle: 短いテキスト（そのまま返る）
- truncate_middle: 拡張子付きファイル名の省略
- truncate_middle: 拡張子なしファイル名の省略
- truncate_middle: マルチバイト文字（日本語ファイル名）
- find_query_match_ranges: 基本一致
- find_query_match_ranges: 大文字小文字無視
- find_query_match_ranges: 複数マッチ
- find_query_match_ranges: 空クエリ → 空結果
- find_query_match_ranges: マルチバイト文字でのマッチ
- find_query_match_ranges: 部分一致なし
```

---

## Phase 2: 検索サービスの統合テスト強化

### 2-1. IndexManager テスト拡充

既存の `tests/indexing_test.rs` → `tests/services/indexing_test.rs` に移動・拡充。

```
- インデックス作成 → ファイル名検索 → ヒット確認
- インデックス作成 → 内容検索 → ヒット確認（line_number, line_content）
- update_file: 内容変更後に再検索で新内容がヒット
- remove_file: 削除後に検索でヒットしない
- 日本語ファイル名・日本語内容の検索
- 大量ファイル（100+）のインデックスとクエリ性能（タイムアウト付き）
- 空ディレクトリのインデックス
```

### 2-2. RipgrepBackend テスト

新規 `tests/services/ripgrep_test.rs`。

```
- tempdir にファイル作成 → search() でヒット
- 大文字小文字の区別
- バイナリファイルはスキップされる
- .gitignore パターンの尊重（ignore crate の挙動）
- マッチ行の line_number と line_content が正しい
```

### 2-3. SearchEngine 統合テスト

新規 `tests/services/search_engine_test.rs`。

```
- SearchScope::Home でインデックス検索が使われる
- SearchScope::Root で ripgrep 検索が使われる
- progress_subscription で進捗が 0.0 → 1.0 に遷移する
```

### 2-4. FileWatcher テスト拡充

既存 `tests/watcher_test.rs` → `tests/services/watcher_test.rs` に移動・拡充。

```
- ファイル作成イベントの検知
- ファイル変更イベントの検知
- ファイル削除イベントの検知
- デバウンス: 短時間の連続変更がバッチされる
```

---

## Phase 3: UI ロジックテスト（#[gpui::test]）

`TestAppContext` を使用。GPU/ウィンドウシステム不要。

### 3-1. ExplorerPage エンティティテスト

新規 `src/pages/explorer/tests.rs`（`#[cfg(test)]` モジュール）。

```
- new: 初期状態（cwd, sort_key, view_mode, search_visible）
- navigate_to: ディレクトリ変更で entries が更新される
- navigate_to: 履歴スタックに追加される
- go_back / go_forward: 履歴ナビゲーション
- sort: SortKey 変更で entries が再ソートされる
- toggle_search: search_visible が反転する
- trigger_search: 検索結果が filtered_entries に反映される
- select_entry: selected_index が更新される
- view_mode 切替: List ↔ Grid
- column_resize: 幅の更新と制約
```

### 3-2. UI イベントテスト

`cx.add_window_view()` + `cx.simulate_keystrokes()` を使用。

```
- Cmd+F / Ctrl+F で検索バーが表示される
- Escape で検索バーが閉じる
- ディレクトリ行ダブルクリックでナビゲート
- ファイル行クリックでプレビュー表示
- ソートヘッダクリックでソート変更
```

### 3-3. テストパターン（コード例）

```rust
// src/pages/explorer/tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use tempfile::tempdir;

    /// テスト用 ExplorerPage を生成するヘルパー
    fn build_explorer(cx: &mut TestAppContext, cwd: &str) -> Entity<ExplorerPage> {
        cx.new(|cx| {
            let resizable = cx.new(|cx| ResizableState::new(cx));
            let search_input = cx.new(|cx| InputState::new(cx));
            // テスト用: SearchService は new_test_instance() で tempdir ベース
            let search_service = Arc::new(/* テスト用 SearchService */);
            let focus_handle = cx.focus_handle();
            let mut page = ExplorerPage::new(resizable, search_input, search_service, focus_handle);
            page.cwd = cwd.to_string();
            page
        })
    }

    #[gpui::test]
    async fn test_navigate_to_updates_entries(cx: &mut TestAppContext) {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("hello.txt"), "hi").unwrap();

        let page = build_explorer(cx, tmp.path().to_str().unwrap());

        page.update(cx, |page, cx| {
            page.reload();
        });

        page.read_with(cx, |page, _| {
            assert!(!page.entries.is_empty());
        });
    }

    #[gpui::test]
    async fn test_toggle_search(cx: &mut TestAppContext) {
        let page = build_explorer(cx, "/tmp");

        page.update(cx, |page, window, cx| {
            assert!(!page.search_visible);
            page.toggle_search(window, cx);
            assert!(page.search_visible);
            page.toggle_search(window, cx);
            assert!(!page.search_visible);
        });
    }
}
```

---

## Phase 4: 検索 UI の結合テスト（#[gpui::test]）— 詳細

### 4-0. 前提: テスト可能にするための構造変更

現在の `ExplorerPage::trigger_search()` は `task::block_in_place` + `Handle::block_on` で
SearchService を直接呼んでいる。テスト環境では tokio ランタイムが存在しない場合がある。

**対策: SearchService のトレイト抽出**

```rust
// src/services/search/mod.rs
pub trait SearchProvider: Send + Sync {
    fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>>;
}

// 本番用
impl SearchProvider for SearchService {
    fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>> {
        let handle = Handle::current();
        task::block_in_place(|| handle.block_on(self.search(query.to_string(), scope)))
    }
}
```

```rust
// テスト用モック
pub struct MockSearchProvider {
    /// クエリ → 返却結果のマッピング
    pub results: HashMap<String, Vec<SearchResult>>,
    /// 呼び出し回数カウンタ
    pub call_count: Arc<AtomicUsize>,
}

impl SearchProvider for MockSearchProvider {
    fn search_blocking(&self, query: &str, _scope: SearchScope) -> Result<Vec<SearchResult>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.results.get(query).cloned().unwrap_or_default())
    }
}
```

ExplorerPage の `search_service: Arc<SearchService>` を `search_service: Arc<dyn SearchProvider>` に変更。

### 4-1. 検索フロー E2E テスト

`src/pages/explorer/tests/search_flow_test.rs`

**テストケース一覧:**

```rust
#[gpui::test]
async fn test_search_flow_basic(cx: &mut TestAppContext) {
    // 1. MockSearchProvider に「hello」→ 3件の結果を設定
    // 2. ExplorerPage を生成
    // 3. open_search() で検索バー表示
    // 4. search_query = "hello" を設定
    // 5. trigger_search() を実行
    // 6. cx.run_until_parked()
    // 7. アサーション:
    //    - search_results.is_some()
    //    - search_results の件数 == group_results で期待される件数
    //    - filtered_entries が検索結果に基づいて更新されている
    //    - is_performing_search == false
}

#[gpui::test]
async fn test_search_no_results(cx: &mut TestAppContext) {
    // MockSearchProvider に空結果を設定
    // trigger_search() 後:
    //   - search_results == Some(vec![])
    //   - filtered_entries が空
}

#[gpui::test]
async fn test_search_error_handling(cx: &mut TestAppContext) {
    // MockSearchProvider が Err を返す設定
    // trigger_search() 後:
    //   - search_results == Some(vec![])（エラー時の fallback）
    //   - filtered_entries が空
    //   - is_performing_search == false（ロック解除確認）
}

#[gpui::test]
async fn test_search_clear_restores_listing(cx: &mut TestAppContext) {
    // 1. tempdir にファイルを作成、reload() で entries を読み込み
    // 2. 検索実行 → filtered_entries が検索結果に置換される
    // 3. close_search() を呼ぶ
    // 4. アサーション:
    //    - search_results == None
    //    - search_query == ""
    //    - search_visible == false
    //    - filtered_entries == entries（元の一覧に復帰）
}

#[gpui::test]
async fn test_search_empty_query_clears_results(cx: &mut TestAppContext) {
    // 1. 検索実行（結果あり）
    // 2. search_query を空にして trigger_search()
    // 3. アサーション:
    //    - search_results == None
    //    - filtered_entries が元の entries に一致
}
```

### 4-2. 検索スコープ/タイプ切替テスト

`src/pages/explorer/tests/search_scope_test.rs`

```rust
#[gpui::test]
async fn test_search_scope_switch(cx: &mut TestAppContext) {
    // MockSearchProvider: Home → 結果A, Root → 結果B（異なる結果）
    // 1. scope = Home で検索 → 結果A を確認
    // 2. set_search_scope(Root) → scope が変更されたことを確認
    // 3. 再検索 → 結果B を確認
}

#[gpui::test]
async fn test_search_type_switch(cx: &mut TestAppContext) {
    // 1. search_type = Filename で設定
    // 2. search_type = Content に変更
    // 3. search_type = All に変更
    // 各状態で search_type フィールドが正しいことを確認
}

#[gpui::test]
async fn test_match_option_toggles(cx: &mut TestAppContext) {
    // toggle_match_case / toggle_match_whole_word / toggle_use_regex
    // 各トグルで対応フィールドが反転することを確認
    // 2回トグルで元に戻ることを確認
}
```

### 4-3. 検索状態遷移テスト

`src/pages/explorer/tests/search_state_test.rs`

```rust
#[gpui::test]
async fn test_search_state_lifecycle(cx: &mut TestAppContext) {
    // 検索の完全ライフサイクルを検証
    //
    // [初期] search_visible=false, search_results=None, search_query=""
    //   ↓ open_search()
    // [検索バー表示] search_visible=true, search_results=None
    //   ↓ search_query="test", trigger_search()
    // [検索実行中] is_performing_search=true
    //   ↓ 検索完了
    // [結果表示] search_results=Some(...), filtered_entries=検索結果
    //   ↓ close_search()
    // [初期に復帰] search_visible=false, search_results=None, filtered_entries=元一覧
}

#[gpui::test]
async fn test_dir_change_clears_search(cx: &mut TestAppContext) {
    // 1. 検索実行 → 結果あり
    // 2. change_dir() でディレクトリ移動
    // 3. アサーション:
    //    - search_visible == false
    //    - search_results == None
    //    - search_query == ""
    //    - entries は新ディレクトリの内容
}

#[gpui::test]
async fn test_search_expanded_files_state(cx: &mut TestAppContext) {
    // 1. 検索実行 → 複数ファイルの結果
    // 2. expanded_search_files にファイルパスを追加
    // 3. item_sizes が展開分だけ大きくなることを確認
    // 4. expanded_search_files からパスを除去
    // 5. item_sizes が元に戻ることを確認
}

#[gpui::test]
async fn test_search_with_preview(cx: &mut TestAppContext) {
    // 1. 検索実行 → 結果のファイルを選択
    // 2. open_preview() が呼ばれる
    // 3. preview_editor が Some になる
    // 4. update_editor_search() で検索クエリがエディタに渡される
}
```

### 4-4. 検索結果からの操作テスト

`src/pages/explorer/tests/search_flow_test.rs`（同ファイルに追記）

```rust
#[gpui::test]
async fn test_search_result_select_opens_preview(cx: &mut TestAppContext) {
    // 1. tempdir にテストファイルを作成（内容あり）
    // 2. 検索実行 → filtered_entries にファイルが含まれる
    // 3. selected_index を設定してファイルを選択
    // 4. open_preview() を呼ぶ
    // 5. アサーション:
    //    - preview_path == Some(選択したファイルのパス)
    //    - preview_text == Some(ファイル内容)
    //    - preview_editor.is_some()
}

#[gpui::test]
async fn test_search_result_activate_dir(cx: &mut TestAppContext) {
    // 検索結果にディレクトリが含まれる場合
    // activate_entry() でディレクトリをダブルクリック
    // → change_dir() が呼ばれ、cwd が変わる
    // → 検索がクリアされる
}

#[gpui::test]
async fn test_search_result_scroll_to_match(cx: &mut TestAppContext) {
    // 1. tempdir にファイル作成（複数行、10行目にマッチ）
    // 2. 検索実行 → 結果あり
    // 3. open_preview() でファイルを開く
    // 4. scroll_to_line(10) を呼ぶ
    // 5. preview_editor の scroll 位置が更新されていることを確認
}
```

### 4-5. 検索 UI イベントテスト（ウィンドウベース）

`src/pages/explorer/tests/search_flow_test.rs`（ウィンドウテスト部分）

```rust
#[gpui::test]
async fn test_cmd_f_opens_search_bar(cx: &mut TestAppContext) {
    // cx.add_window_view() で ExplorerPage をウィンドウに配置
    // cx.simulate_keystrokes("cmd-f")
    // → search_visible == true
    // → search_input にフォーカスが移る
}

#[gpui::test]
async fn test_escape_closes_search_bar(cx: &mut TestAppContext) {
    // 検索バーを表示した状態で
    // cx.simulate_keystrokes("escape")
    // → search_visible == false
    // → search_results == None
}

#[gpui::test]
async fn test_search_bar_input_triggers_search(cx: &mut TestAppContext) {
    // ウィンドウベーステスト
    // 1. Cmd+F で検索バー表示
    // 2. cx.simulate_input("hello") で入力
    // 3. Enter キーまたは debounce 後に trigger_search() が呼ばれる
    // 4. MockSearchProvider の call_count が 1 であることを確認
}
```

### 4-6. テストデータ設計

```rust
/// Phase 4 テスト共通のモックデータ生成
mod test_data {
    use nohrs::services::search::SearchResult;
    use std::path::PathBuf;

    /// 単一ファイル・単一マッチ
    pub fn single_match() -> Vec<SearchResult> {
        vec![SearchResult {
            path: PathBuf::from("/tmp/test/hello.rs"),
            line_number: 5,
            line_content: "fn hello() { println!(\"hello\"); }".to_string(),
        }]
    }

    /// 同一ファイルに複数マッチ（group_results の検証用）
    pub fn multi_match_same_file() -> Vec<SearchResult> {
        vec![
            SearchResult {
                path: PathBuf::from("/tmp/test/lib.rs"),
                line_number: 1,
                line_content: "use std::collections::HashMap;".to_string(),
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/lib.rs"),
                line_number: 10,
                line_content: "let map = HashMap::new();".to_string(),
            },
        ]
    }

    /// 複数ファイルにまたがるマッチ
    pub fn multi_file_matches() -> Vec<SearchResult> {
        vec![
            SearchResult {
                path: PathBuf::from("/tmp/test/a.rs"),
                line_number: 3,
                line_content: "fn search() {}".to_string(),
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/b.rs"),
                line_number: 7,
                line_content: "fn search_inner() {}".to_string(),
            },
            SearchResult {
                path: PathBuf::from("/tmp/test/c/d.rs"),
                line_number: 1,
                line_content: "mod search;".to_string(),
            },
        ]
    }
}
```

---

## 実装順序

| 順序 | Phase | 内容 | 依存 |
|------|-------|------|------|
| 1 | 1-1, 1-2 | 型と関数を `src/core/` へ移動 | なし |
| 2 | 1-3 | find_query_highlights の分離 | 1 |
| 3 | 1-4 | 純粋ロジックの単体テスト追加 | 1, 2 |
| 4 | 2-1〜2-4 | 検索サービステスト拡充 | なし（並行可） |
| 5 | 4-0 | SearchProvider トレイト抽出 + MockSearchProvider | 1 |
| 6 | 3-1 | ExplorerPage エンティティテスト | 1, 5 |
| 7 | 3-2 | UI イベントテスト | 6 |
| 8 | 4-1〜4-4 | 検索フロー・スコープ・状態遷移テスト | 5, 6 |
| 9 | 4-5 | 検索 UI イベントテスト（ウィンドウベース） | 8 |

## CI 設定

```yaml
# .github/workflows/test.yml
jobs:
  test-core:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test             # core + services テスト（高速）

  test-gui:
    runs-on: ubuntu-latest
    needs: test-core                # core が通ってから実行
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --features gui   # UI テスト（TestAppContext、GPU不要）
```

2段階に分けることで、GUI feature なしのテストは高速に回り、
GUI feature ありのテストも `TestAppContext` によりヘッドレスで実行可能。
