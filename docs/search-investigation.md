# 検索機能 調査レポート

## 1. アーキテクチャ概要

```
┌──────────────────┐     ┌───────────────────────┐     ┌────────────────────┐
│  UI Layer        │     │  Service Layer        │     │  Backend Layer     │
│                  │     │                       │     │                    │
│ search_bar.rs    │────▶│ SearchProvider trait   │────▶│ IndexManager       │
│ (検索バー描画)     │     │ SearchService         │     │ (Tantivy/Home)     │
│                  │     │                       │     │                    │
│ row.rs           │     │                       │────▶│ RipgrepBackend     │
│ (結果行描画)       │     │                       │     │ (grep/Root)        │
│                  │     │                       │     │                    │
│ PreviewEditor    │     │                       │     │ FileWatcher        │
│ (プレビュー)       │     │                       │     │ (インデックス更新)   │
└──────────────────┘     └───────────────────────┘     └────────────────────┘
```

### コンポーネント一覧

| ファイル | 役割 |
|---------|------|
| `src/services/search/mod.rs` | `SearchProvider` トレイト定義、`SearchService` 実装 |
| `src/services/search/engine.rs` | `SearchEngine` — Scope に応じたバックエンド振り分け |
| `src/services/search/indexer.rs` | `IndexManager` — Tantivy インデックス管理・検索 |
| `src/services/search/ripgrep.rs` | `RipgrepBackend` — grep crate による全文検索 |
| `src/services/search/watcher.rs` | `FileWatcher` — notify によるファイル変更監視 |
| `src/services/search/backend.rs` | `SearchBackend` トレイト定義 |
| `src/core/search_utils.rs` | `group_results` / `results_to_entries` 純粋関数 |
| `src/core/types.rs` | `SearchFileResult`, `SearchMatch`, `SearchType` 型 |
| `src/core/text.rs` | `find_query_match_ranges` — ハイライト用マッチ範囲計算 |
| `src/pages/explorer/mod.rs` | `ExplorerPage` — 検索の状態管理・`trigger_search` |
| `src/pages/explorer/search.rs` | 再エクスポートのみ |
| `src/pages/explorer/view/listing/search_bar.rs` | 検索バー UI |
| `src/pages/explorer/view/listing/row.rs` | ファイル行 + スニペット描画 |
| `src/pages/explorer/view/mod.rs` | `find_query_highlights` ラッパー |
| `src/pages/explorer/view/preview/editor.rs` | `PreviewEditor` プレビュー表示 |

---

## 2. 検索フロー詳細

### 2.1 検索の開始

1. `Cmd+F` / `Ctrl+F` キーバインド → `toggle_search()` → `open_search()` (`view/mod.rs:28-37`)
2. `search_visible = true` に設定、検索入力欄にフォーカス移動 (`mod.rs:488-497`)

### 2.2 クエリ入力時の挙動

`search_bar.rs` の `render()` 関数内で **毎フレーム** `InputState` のテキストと `page.search_query` を比較:

```rust
// search_bar.rs:11-19
let current_text = page.search_input.read(cx).text().to_string();
if current_text != page.search_query {
    page.search_query = current_text;
    page.search_results = None;
    page.apply_filter();
}
```

入力のたびに `search_results` がクリアされ、`apply_filter()` が呼ばれる（通常のファイル一覧に戻る）。

### 2.3 検索の実行

Enter キー押下 or 「Search」ボタンクリック → `trigger_search()`:

```rust
// mod.rs:145-178
fn trigger_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.is_performing_search = true;
    let results = self.search_service.search_blocking(&self.search_query, self.search_scope);
    // ... 同期的に結果を処理 ...
    self.is_performing_search = false;
}
```

### 2.4 バックエンド振り分け

`SearchEngine::search()` (`engine.rs:104-117`) で `SearchScope` に応じて振り分け:

- **`SearchScope::Home`** → `IndexManager` (Tantivy インデックス検索)
- **`SearchScope::Root`** → `RipgrepBackend` (grep crate でリアルタイム検索)

### 2.5 Tantivy インデックス検索 (Home)

`indexer.rs:320-396`:
1. `QueryParser` で `filename` と `content` フィールドを対象にクエリをパース
2. `TopDocs::with_limit(50)` で上位50件取得
3. ファイルマッチごとに `find_all_match_lines()` を呼び、**ファイルを再度読み直して** 行レベルのマッチを取得

### 2.6 Ripgrep 検索 (Root)

`ripgrep.rs:55-110`:
1. `WalkBuilder` で `max_depth(10)`, `hidden(true)`, `git_ignore(true)` のウォーカーを作成
2. `grep::searcher` でファイル内検索
3. 最大100件で打ち切り

### 2.7 結果の加工

1. `group_results()` — `Vec<SearchResult>` をファイル単位に集約 (`core/search_utils.rs:6-48`)
2. `results_to_entries()` — `SearchFileResult` → `FileEntryDto` 変換 (`core/search_utils.rs:50-80`)

### 2.8 結果の表示

- `filtered_entries` にファイル一覧として設定
- `search_results` にグループ化された結果を保持
- `row.rs` で各行を描画、展開/折りたたみでスニペット表示
- `find_query_highlights()` でファイル名・スニペット内のマッチ箇所をハイライト

### 2.9 プレビューとの連携

ファイル選択時に `open_preview()` でプレビューエディタを生成し、検索クエリに基づいたハイライト・スクロールを試みる。

### 2.10 インデックスの初期化・更新

- 起動時: `spawn_blocking` で `index_home()` を実行（`~/Documents` 配下）
- ファイル変更時: `FileWatcher` → `mpsc::channel` → `process_changes()` でインデックス更新
- デバウンス: `notify_debouncer_mini` で 2秒間隔

---

## 3. 検索オプション

| オプション | UI | フィールド | 実際の利用状況 |
|-----------|-----|-----------|-------------|
| Scope (Home/Root) | スコープボタン | `search_scope` | 使用されている |
| Type (All/Filename/Content) | タイプボタン | `search_type` | **UI のみ、検索ロジックで未使用** |
| Match Case (Aa) | トグルボタン | `match_case` | **UI のみ、検索ロジックで未使用** |
| Match Whole Word (ab) | トグルボタン | `match_whole_word` | **UI のみ、検索ロジックで未使用** |
| Use Regex (.*) | トグルボタン | `use_regex` | **UI のみ、検索ロジックで未使用** |

---

## 4. 発見された問題点

### 4.1 致命的なバグ (Critical)

#### 4.1.1 `trigger_search` が UI スレッドをブロックする

**場所**: `mod.rs:145-178`

`trigger_search()` は `search_service.search_blocking()` を **GPUI のメインスレッド上で同期的に** 呼び出している。`SearchService::search_blocking()` 内部では `tokio::task::block_in_place` + `handle.block_on` を使用しているが、これは GPUI のイベントループをブロックし、UI がフリーズする。

特に `SearchScope::Root` の場合、ルートディレクトリからの再帰検索となるため、数十秒〜数分間 UI が完全に応答不能になる可能性がある。

```rust
// mod.rs:156-158 — UI スレッドをブロックする呼び出し
let results = self
    .search_service
    .search_blocking(&self.search_query, self.search_scope);
```

**推奨修正**: `cx.spawn()` / `cx.background_executor()` を使用して非同期化する。

#### 4.1.2 検索オプションが無視される

**場所**: `mod.rs:145-178`, `engine.rs:104-117`

`match_case`, `match_whole_word`, `use_regex`, `search_type` の各オプションが `trigger_search()` から `search_service.search_blocking()` に一切渡されていない。UI には表示されるがロジックに反映されない。

`SearchProvider::search_blocking` のシグネチャが `query` と `scope` しか受け取らない:
```rust
fn search_blocking(&self, query: &str, scope: SearchScope) -> Result<Vec<SearchResult>>;
```

**推奨修正**: 検索オプションを含む `SearchQuery` 構造体を定義し、シグネチャを変更する。

#### 4.1.3 `find_all_match_lines` での二重ファイル読み込み

**場所**: `indexer.rs:399-420`

Tantivy でファイルがマッチした後、行レベルのマッチを得るために `find_all_match_lines()` で **同じファイルを再度読み込んでいる**。これは `spawn_blocking` 内で行われるため、Tantivy 検索の上位50件全てのファイルに対して `fs::read_to_string` が追加発生する。

さらに、Tantivy のクエリはトークナイザを通過するため、`find_all_match_lines` のナイーブな `contains()` マッチと結果が一致しない場合がある（例: Tantivy がステミングや正規化を適用した場合）。

#### 4.1.4 `SearchMatch` の `match_start` / `match_end` が常に 0

**場所**: `core/search_utils.rs:35-41`

```rust
entry.matches.push(SearchMatch {
    line_number: res.line_number,
    line_content: res.line_content,
    match_start: 0,  // 常に 0
    match_end: 0,     // 常に 0
});
```

`SearchMatch` にはマッチ位置を示す `match_start` / `match_end` フィールドがあるが、常に 0 がセットされている。行内のどこがマッチしたかの情報が失われており、正確なハイライト表示に使用できない。現在は `find_query_highlights` で再計算しているが、バックエンドが既に持っている情報を捨てている。

### 4.2 パフォーマンス問題 (Performance)

#### 4.2.1 `results_to_entries` での同期ファイルシステムアクセス

**場所**: `core/search_utils.rs:50-80`

`results_to_entries()` は各検索結果ファイルに対して `std::fs::metadata()` を呼び出してサイズ・更新日時を取得している。これは **UI スレッド上の `trigger_search` から同期的に呼ばれる** ため、検索結果が多い場合にパフォーマンスが劣化する。

```rust
let meta = std::fs::metadata(&res.path).ok(); // 検索結果ごとに stat(2) 呼び出し
```

#### 4.2.2 `render` 内での毎フレームの文字列比較・状態変更

**場所**: `search_bar.rs:11-19`

検索バーの `render()` 関数内で毎フレーム `InputState::text()` を取得して `search_query` と比較し、異なれば `search_results = None` + `apply_filter()` を呼んでいる。`apply_filter()` 内では `entries.clone()` + ソートが走る。

これは render 内での副作用であり、GPUI のベストプラクティスに反する。`InputState` の変更はイベントハンドラで処理すべき。

#### 4.2.3 Tantivy 検索結果の上限が 50 件固定

**場所**: `indexer.rs:341`

```rust
let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(50))?;
```

上位50件しか取得しないため、大量のファイルにマッチするクエリでは結果が不完全になる。さらに、50件それぞれに対して `find_all_match_lines()` でファイルを再読み込みするため、最大50回のファイル I/O が追加発生する。

#### 4.2.4 Ripgrep バックエンドのシングルスレッド走査

**場所**: `ripgrep.rs:55-110`

`WalkBuilder` の並列検索機能 (`build_parallel()`) を使用しておらず、シングルスレッドで逐次走査している。`/` からの検索では極めて遅くなる。

#### 4.2.5 `update_item_sizes` の O(n×m) 計算

**場所**: `mod.rs:272-302`

`update_item_sizes()` で全ファイルの行高さを再計算する際、展開済みファイルごとに `search_results` を線形検索している:

```rust
self.search_results
    .as_ref()
    .and_then(|results| {
        results.iter().find(|r| r.path == entry.path)  // O(n)
    })
```

`filtered_entries × search_results` の O(n×m) になり得る。HashMap でのルックアップに置き換えるべき。

#### 4.2.6 `row.rs` での同様の O(n) 検索

**場所**: `row.rs:48-56`, `row.rs:60-73`

各行の描画でも `search_results` を線形検索してスニペット情報を取得している。仮想リストの描画では表示行ごとに呼ばれるため、スクロール時にも繰り返し実行される。

#### 4.2.7 インデックス初期化時のファイルカウント二重走査

**場所**: `indexer.rs:115-131`

プログレス追跡のために、インデックス作成前に全ファイルを一度走査してカウントし、その後もう一度走査してインデックスを作成している。合計で 2回 ファイルツリーを歩く。

### 4.3 設計上の問題 (Design Issues)

#### 4.3.1 `ExplorerPage` の全フィールドが `pub`

**場所**: `mod.rs:29-78`

CLAUDE.md の可視性ルールでは「Page struct のフィールドは原則 private」「submodule からは `pub(super)`」とあるが、全フィールドが `pub` になっている。

#### 4.3.2 `RipgrepBackend` の検索ルートが `/` 固定

**場所**: `engine.rs:21`

```rust
let ripgrep_backend = Arc::new(RipgrepBackend::new(std::path::PathBuf::from("/")));
```

`SearchScope::Root` 選択時にルートディレクトリ全体を検索する。意図的な設計かもしれないが、`max_depth(10)` との組み合わせでも非常に広い範囲を走査する。

#### 4.3.3 Tantivy インデックスの対象が `~/Documents` 限定

**場所**: `indexer.rs:22-23`

```rust
let documents_dir = home_dir.join("Documents");
```

`SearchScope::Home` は `~/Documents` のみを対象としている。ホームディレクトリ全体や任意のディレクトリをインデックス対象にする柔軟性がない。

#### 4.3.4 `PreviewEditor` の主要機能が全て未実装

**場所**: `view/preview/editor.rs:33-55`

以下の3メソッドが全て TODO + no-op:
- `set_highlights()` — ハイライト API が未公開
- `scroll_to()` — スクロール API が未公開
- `set_search_query()` — 検索クエリ API が未公開

検索結果からのプレビュー連携が事実上機能していない。`open_preview()` 内の `scroll_to` 呼び出しやハイライト設定は全て無効。

#### 4.3.5 `open_preview` 内の同期ファイル読み込み

**場所**: `mod.rs:519-647`

`open_preview()` は `std::fs::metadata()` + `std::fs::read()` を UI スレッド上で同期的に実行している。2MB 制限はあるが、ネットワークファイルシステム上のファイルなどではブロックし得る。

#### 4.3.6 `search_bar.rs:render()` 内での副作用

**場所**: `search_bar.rs:10-19`

`render()` 関数内で `page.search_query` への代入、`page.search_results` のクリア、`page.apply_filter()` の呼び出しといった状態変更を行っている。GPUI では render は副作用のない純粋な描画関数であるべき。

#### 4.3.7 `process_changes` がディレクトリの更新を無視

**場所**: `indexer.rs:279-313`

`process_changes()` はファイルの変更/削除のみ処理し、ディレクトリの作成・削除は `index_single_file` しか呼ばない。新しいディレクトリが作成されてもインデックスの `is_directory` エントリは追加されない。

#### 4.3.8 検索クエリがそのまま正規表現として渡される

**場所**: `ripgrep.rs:57`

```rust
let matcher = RegexMatcher::new(query_str).context("Invalid regex")?;
```

ユーザー入力がそのまま正規表現としてコンパイルされるため、`[`, `(`, `.` 等の特殊文字を含むクエリでエラーになる。`use_regex` フラグが false の場合はエスケープすべき。

### 4.4 潜在的なバグ (Potential Bugs)

#### 4.4.1 `open_preview` での行番号の off-by-one

**場所**: `mod.rs:586-610`

`scroll_to_line()` (`mod.rs:237-270`) では `line.saturating_sub(1)` で 1-based → 0-based 変換しているが、`open_preview()` 内のスクロール処理では `first_match.line_number` をそのまま 0-based インデックスとして使っている (`if i == target_line`)。

`find_all_match_lines` は 1-based の行番号を返す (`idx + 1`) が、`open_preview` では `target_line` として直接使い、`lines().enumerate()` の 0-based `i` と比較しているため、**1行分ずれる**。

ただし `scroll_to` 自体が no-op なので現時点では顕在化しない。

#### 4.4.2 `group_results` でのファイル名分解の問題

**場所**: `core/search_utils.rs:64-69`

`results_to_entries()` で生成される `name` フィールドが、`folder` が空でない場合 `"{folder}/{filename}"` となるが、`folder` はフルパスの親ディレクトリである。これにより `name` がほぼフルパスと同じになり、表示上の意味が薄い。

#### 4.4.3 Tantivy クエリ構文とユーザー入力の不整合

**場所**: `indexer.rs:335-339`

Tantivy の `QueryParser` はデフォルトで AND/OR/NOT 等の演算子やフレーズ検索 (`"..."`) を解釈する。ユーザーがこれらの文字列を含む一般的な検索語を入力した場合、予期しないクエリが構築される可能性がある。

例: `NOT found` → `found` を含まないドキュメントを検索してしまう。

#### 4.4.4 `is_performing_search` が意味をなしていない

**場所**: `mod.rs:153, 174`

`is_performing_search` が `true` にセットされた直後に同期的な `search_blocking()` が呼ばれ、完了後すぐに `false` に戻る。UI の再描画は `cx.notify()` 後なので、ユーザーに「検索中」の状態が表示されることはない。

---

## 5. テストカバレッジ

### 現在のテスト

| テストファイル | テスト数 | カバー範囲 |
|-------------|---------|-----------|
| `search_flow_test.rs` | 5 | 基本検索フロー、空結果、エラー処理、クリア復帰、ディレクトリ活性化 |
| `search_state_test.rs` | 5 | ライフサイクル、ディレクトリ移動時クリア、展開状態、トグル、冪等性 |
| `search_scope_test.rs` | 3 | スコープ切替、タイプ切替、マッチオプショントグル |

### 不足しているテスト

- `RipgrepBackend` の単体テスト
- `IndexManager` の単体テスト（インデックス作成・更新・検索）
- `find_all_match_lines` のテスト
- `group_results` / `results_to_entries` の単体テスト
- 検索オプション（case/word/regex）が反映されるかのテスト（そもそも未実装）
- 正規表現特殊文字を含むクエリのテスト
- 大量結果時のパフォーマンステスト

---

## 6. 修正優先度

### P0 (即座に修正すべき)

1. **UI スレッドブロック**: `trigger_search` を非同期化
2. **正規表現エスケープ**: `use_regex` フラグに応じてクエリをエスケープ

### P1 (次スプリントで修正すべき)

3. **検索オプションの実装**: `match_case`, `match_whole_word`, `use_regex`, `search_type` を実際にバックエンドに渡す
4. **`results_to_entries` の非同期化**: `std::fs::metadata` を `tokio::fs::metadata` に置き換え
5. **`render` 内の副作用排除**: `search_bar.rs` の状態変更をイベントハンドラに移動
6. **`PreviewEditor` の機能実装**: `scroll_to`, `set_search_query`, `set_highlights` の実装

### P2 (改善すべき)

7. **`find_all_match_lines` の二重読み込み排除**: Tantivy に `content` を `STORED` にして直接取得、または行情報をインデックスに持たせる
8. **O(n×m) ルックアップの改善**: `update_item_sizes` / `row.rs` で HashMap を使用
9. **`SearchMatch` の `match_start/end` 設定**: バックエンドでマッチ位置を計算
10. **Ripgrep の並列化**: `build_parallel()` の使用
11. **Tantivy クエリのサニタイズ**: ユーザー入力のエスケープ or `TermQuery` の使用
12. **フィールド可視性の修正**: `pub` → `pub(super)` or private
13. **`open_preview` の非同期化**: ファイル読み込みを `background_executor` で実行

### P3 (将来的な改善)

14. インデックス対象ディレクトリの設定可能化
15. インクリメンタル検索 / デバウンス付きオートサーチ
16. 検索結果のページネーション
17. `is_performing_search` を活用したローディングインジケーター
