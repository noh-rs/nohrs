# nohrs テスト仕様書

全 82 テストの目的・フロー・検証仕様をまとめたドキュメント。

---

## 目次

1. [テスト基盤](#テスト基盤)
2. [Phase 3: ExplorerPage エンティティテスト](#phase-3-explorerpage-エンティティテスト)
3. [Phase 4: 検索 UI 結合テスト](#phase-4-検索-ui-結合テスト)
   - [search_flow_test](#search_flow_test)
   - [search_state_test](#search_state_test)
   - [search_scope_test](#search_scope_test)
   - [navigation_test](#navigation_test)
   - [sort_test (explorer)](#sort_test-explorer)
4. [Core テスト](#core-テスト)
   - [search_utils_test](#search_utils_test)
   - [sort_test (core)](#sort_test-core)
   - [text_test](#text_test)
5. [Service テスト](#service-テスト)
   - [indexing_test (service)](#indexing_test-service)
   - [ripgrep_test](#ripgrep_test)
   - [search_engine_test](#search_engine_test)
   - [watcher_test (service)](#watcher_test-service)
6. [統合テスト](#統合テスト)
   - [indexing_test (integration)](#indexing_test-integration)
   - [watcher_test (integration)](#watcher_test-integration)

---

## テスト基盤

### ファイル構成

```
src/pages/explorer/
├── tests.rs                        # Phase 3/4 共有基盤 + Phase 3 テスト本体
└── tests/
    ├── navigation_test.rs          # ディレクトリ移動・履歴
    ├── sort_test.rs                # ソート適用
    ├── search_flow_test.rs         # 検索フロー（実行・結果・クリア）
    ├── search_state_test.rs        # 検索状態ライフサイクル
    └── search_scope_test.rs        # 検索スコープ・タイプ・オプション

tests/
├── core_tests/
│   ├── search_utils_test.rs        # group_results / results_to_entries
│   ├── sort_test.rs                # sort_entries / get_extension
│   └── text_test.rs                # truncate_middle / find_query_match_ranges
├── service_tests/
│   ├── indexing_test.rs            # IndexManager
│   ├── ripgrep_test.rs             # RipgrepBackend
│   ├── search_engine_test.rs       # SearchEngine 統合
│   └── watcher_test.rs             # FileWatcher
├── indexing_test.rs                # インデックス E2E ワークフロー
└── watcher_test.rs                 # ファイル監視 E2E
```

### MockSearchProvider

`tests.rs` に定義されたモック検索プロバイダー。全 Phase 3/4 テストで共有。

| メソッド | 役割 |
|---|---|
| `new()` | 空の結果・正常モードで初期化 |
| `with_results(query, results)` | 特定クエリに対する固定結果を登録 |
| `with_error_mode()` | `search_blocking` が常にエラーを返すモード |
| `call_count` | `Arc<AtomicUsize>` で検索呼び出し回数を追跡 |

### test_data モジュール

| 関数 | 返すデータ |
|---|---|
| `single_match()` | 1ファイル1行マッチ (`/tmp/test/hello.rs:5`) |
| `multi_match_same_file()` | 1ファイル2行マッチ (`/tmp/test/lib.rs:1,10`) |
| `multi_file_matches()` | 3ファイル各1行マッチ (`a.rs`, `b.rs`, `c/d.rs`) |

### build_explorer ヘルパー

**目的**: GPUI ウィンドウ付きの `ExplorerPage` エンティティを生成する。

**フロー**:
1. `gpui_component::init(cx)` で Theme 等のグローバル状態を初期化（重複呼び出し防止付き）
2. `cx.add_window` で `Root` をルートビューとしてウィンドウ生成
3. `ExplorerPage` を `Entity` として生成し、`Root::new(explorer.into(), ...)` でラップ
4. `run_until_parked()` で初回レンダリング完了を待機（`ensure_loaded` → `reload` 実行）
5. `(Entity<ExplorerPage>, &mut VisualTestContext)` を返す

**設計上の注意点**:
- `gpui_component` の InputState 等が `Root::read()` を要求するため、`add_window_view` ではなく `Root` ラップが必須
- 初回レンダリングで `ensure_loaded()` が発火し、`cwd` のファイル一覧が自動ロードされる
- `cx.notify()` が再レンダリングを引き起こすため、検索関連のアサーションは `update_window_entity` 内で行う必要がある

---

## Phase 3: ExplorerPage エンティティテスト

`src/pages/explorer/tests.rs` に定義。ExplorerPage の基本的なエンティティ操作を検証する。

---

### test_initial_state

**目的**: ExplorerPage 生成直後のデフォルト状態を検証する。

**フロー**:
1. 空の tempdir を作成
2. `build_explorer_default` でページ生成
3. `read_with` で全フィールドのデフォルト値を検証

**検証仕様**:
| フィールド | 期待値 | 理由 |
|---|---|---|
| `sort_key` | `SortKey::Name` | デフォルトはファイル名ソート |
| `sort_asc` | `true` | 昇順がデフォルト |
| `view_mode` | `ViewMode::List` | リスト表示がデフォルト |
| `search_visible` | `false` | 検索バーは非表示 |
| `search_query` | `""` | 検索クエリは空 |
| `search_results` | `None` | 検索未実行 |
| `entries` | `[]` | 空ディレクトリなのでエントリなし |
| `history` | `[]` | 移動履歴なし |

---

### test_reload_loads_entries

**目的**: ファイルが存在するディレクトリでエントリが自動ロードされることを検証する。

**フロー**:
1. tempdir に `hello.txt`, `world.txt` を作成
2. `build_explorer_default` でページ生成（初回レンダリングで自動ロード）
3. `entries.len() == 2` を検証

**検証仕様**:
- `ensure_loaded()` → `reload()` → `list_dir_sync()` の連鎖が正しく動作する
- ファイルシステム上の実ファイルがエントリとして読み込まれる

---

### test_sort_key_change

**目的**: ソートキー変更時のトグル動作と新キー切替動作を検証する。

**フロー**:
1. tempdir に `alpha.txt`, `beta.txt` を作成
2. ページ生成後 `reload()`
3. `set_sort_key(SortKey::Name)` — 同一キー → 方向トグル
4. `set_sort_key(SortKey::Size)` — 別キー → 昇順リセット

**検証仕様**:
| 操作 | sort_key | sort_asc | 理由 |
|---|---|---|---|
| 初期状態 | `Name` | `true` | デフォルト |
| `set_sort_key(Name)` | `Name` | `false` | 同一キーで方向反転 |
| `set_sort_key(Size)` | `Size` | `true` | 新キーで昇順リセット |

---

### test_select_entry

**目的**: エントリ選択状態の読み書きを検証する。

**フロー**:
1. ページ生成
2. `selected_index` が `None` であることを確認
3. `Some(3)` をセットし反映を確認

**検証仕様**:
- `selected_index` は `Option<usize>` 型で初期値 `None`
- 任意のインデックスを設定可能

---

### test_view_mode_toggle

**目的**: List / Grid 表示モードの切替を検証する。

**フロー**:
1. 初期状態 `ViewMode::List` を確認
2. `set_view_mode(Grid)` → Grid に変更
3. `set_view_mode(List)` → List に復帰

**検証仕様**:
- `set_view_mode` で即座にモードが切り替わる
- 双方向に切替可能

---

### test_column_resize

**目的**: カラムリサイズのドラッグ操作フローを検証する。

**フロー**:
1. `start_column_resize(0, point(100, 0))` でリサイズ開始
2. `resizing_column` が `Some` になることを確認
3. `update_column_resize(point(150, 0))` で右に 50px ドラッグ
4. `col_name_width` が初期値より大きくなることを確認
5. `stop_column_resize()` で終了
6. `resizing_column` が `None` に戻ることを確認

**検証仕様**:
- カラム 0（Name）のリサイズが正しく動作する
- start → update → stop のライフサイクルが正常

---

### test_column_resize_min_width

**目的**: カラム幅の最小値制約を検証する。

**フロー**:
1. `start_column_resize(0, point(400, 0))` でリサイズ開始
2. `update_column_resize(point(0, 0))` で左端までドラッグ（-400px）
3. `col_name_width >= 80.0` を検証

**検証仕様**:
- カラム幅は最小 80px を下回らない
- 極端なドラッグでもクラッシュしない

---

## Phase 4: 検索 UI 結合テスト

### search_flow_test

`src/pages/explorer/tests/search_flow_test.rs` — 検索の実行から結果表示までのフローを検証する。

---

#### test_search_flow_basic

**目的**: 基本的な検索フロー（検索実行→結果取得→状態更新）を検証する。

**フロー**:
1. `multi_file_matches()` を返す MockSearchProvider を作成
2. `open_search` → `search_query = "hello"` → `trigger_search`
3. `update_window_entity` 内で同期的にアサーション

**検証仕様**:
| 項目 | 期待値 |
|---|---|
| `search_results` | `Some(...)` — 3ファイル分のグループ |
| `filtered_entries` | 空でない |
| `is_performing_search` | `false`（同期完了） |
| `call_count` | `1`（1回だけ呼ばれた） |

---

#### test_search_no_results

**目的**: 検索結果が0件の場合の状態を検証する。

**フロー**:
1. 結果未登録の MockSearchProvider を使用
2. `"nonexistent"` で検索実行

**検証仕様**:
| 項目 | 期待値 |
|---|---|
| `search_results` | `Some(vec![])` — 空配列（検索は実行済み） |
| `filtered_entries` | `[]` — 空 |

**仕様ポイント**: `search_results` が `None`（未検索）ではなく `Some(vec![])`（結果0件）になる。

---

#### test_search_error_handling

**目的**: 検索バックエンドがエラーを返した場合のフォールバック動作を検証する。

**フロー**:
1. `with_error_mode()` の MockSearchProvider を使用
2. 検索実行

**検証仕様**:
| 項目 | 期待値 |
|---|---|
| `search_results` | `Some(vec![])` — エラー時も空結果をセット |
| `filtered_entries` | `[]` |
| `is_performing_search` | `false` — フラグはクリア |

**仕様ポイント**: エラーでもパニックせず、空結果としてグレースフルに処理する。

---

#### test_search_clear_restores_listing

**目的**: 検索クローズ時に元のファイル一覧が復元されることを検証する。

**フロー**:
1. tempdir に 2 ファイル作成 → 自動ロードで `entries.len() == 2`
2. 検索実行 → `search_results` がセットされる
3. `close_search` 呼び出し
4. 一覧が元に戻ることを検証

**検証仕様**:
| close_search 後 | 期待値 |
|---|---|
| `search_results` | `None` |
| `search_query` | `""` |
| `search_visible` | `false` |
| `filtered_entries.len()` | `2`（元の一覧に復帰） |

---

#### test_search_empty_query_clears_results

**目的**: 空クエリで `trigger_search` した場合に検索結果がクリアされることを検証する。

**フロー**:
1. tempdir に 1 ファイル作成
2. `"test"` で検索 → 結果あり
3. `search_query.clear()` → `trigger_search` 再実行
4. 結果クリア＆一覧復帰を検証

**検証仕様**:
- 空クエリの `trigger_search` は即座に `search_results = None` をセット
- `filtered_entries` が元の一覧に復帰する

---

#### test_search_result_activate_dir

**目的**: 検索結果（またはファイル一覧）のディレクトリエントリをアクティベートした際にディレクトリ移動が発生することを検証する。

**フロー**:
1. tempdir に `subdir` を作成
2. `filtered_entries` から `kind == "dir"` のエントリを検索
3. `activate_entry` 呼び出し
4. `cwd` が `subdir` に変更されることを検証

**検証仕様**:
- ディレクトリエントリの activate は `change_dir` と同等の動作をする

---

### search_state_test

`src/pages/explorer/tests/search_state_test.rs` — 検索状態の遷移とライフサイクルを検証する。

---

#### test_search_state_lifecycle

**目的**: 検索機能の全状態遷移を一貫して検証する。

**フロー（状態遷移図）**:
```
初期状態（search_visible=false, results=None, query=""）
    ↓ open_search
検索バー表示（search_visible=true, results=None）
    ↓ trigger_search("test")
検索完了（results=Some(...), is_performing_search=false）
    ↓ close_search
初期状態に復帰（search_visible=false, results=None, query=""）
```

**検証仕様**:
- 各遷移ポイントで全状態フィールドが期待値と一致する
- close_search で完全に初期状態に戻る

---

#### test_dir_change_clears_search

**目的**: ディレクトリ移動時に検索状態がリセットされることを検証する。

**フロー**:
1. tempdir に `subdir` を作成（中にファイルあり）
2. 検索実行 → `search_visible=true`, `search_results=Some(...)`
3. `change_dir(subdir)` 呼び出し
4. 検索状態リセット＆新ディレクトリのエントリロードを検証

**検証仕様**:
| change_dir 後 | 期待値 |
|---|---|
| `search_visible` | `false` |
| `search_results` | `None` |
| `search_query` | `""` |
| `cwd` | subdir のパス |
| `entries` | 空でない（新ディレクトリの内容） |

---

#### test_search_expanded_files_state

**目的**: 検索結果のファイル展開/折りたたみで `item_sizes` が変化することを検証する。

**フロー**:
1. 同一ファイルに複数マッチを返すモックで検索実行
2. `expanded_search_files` にファイルパスを追加 → `update_item_sizes()`
3. 展開時の `item_sizes` を記録
4. `expanded_search_files` からファイルパスを削除 → `update_item_sizes()`
5. 折りたたみ時の `item_sizes` を記録
6. 展開時 > 折りたたみ時（高さ）を検証

**検証仕様**:
- ファイル展開時はスニペット行分だけ高さが増加する
- 展開状態は `HashSet<String>` で管理される

---

#### test_toggle_search

**目的**: `toggle_search` の ON/OFF 切替動作を検証する。

**フロー**:
1. 初期状態 `search_visible == false`
2. `toggle_search` → `true`
3. `toggle_search` → `false`

**検証仕様**:
- トグルが正確に反転動作する

---

#### test_open_search_idempotent

**目的**: `open_search` の冪等性を検証する。

**フロー**:
1. `open_search` → `search_visible == true`
2. `open_search` 再呼び出し → `search_visible == true`（変化なし）

**検証仕様**:
- 既に開いている状態で再度開いても副作用がない

---

### search_scope_test

`src/pages/explorer/tests/search_scope_test.rs` — 検索スコープ・タイプ・マッチオプションの状態管理を検証する。

---

#### test_search_scope_switch

**目的**: 検索スコープ（Home / Root）の切替を検証する。

**フロー**:
1. 初期値 `SearchScope::Home` を確認
2. `set_search_scope(Root)` → Root に変更
3. `set_search_scope(Home)` → Home に復帰

**検証仕様**:
- `search_scope` は `Home` がデフォルト
- 任意のスコープに切替可能

---

#### test_search_type_switch

**目的**: 検索タイプ（All / Filename / Content）の切替を検証する。

**フロー**:
1. 初期値 `SearchType::All` を確認
2. `Filename` → `Content` → `All` と順に切替

**検証仕様**:
- `search_type` は `All` がデフォルト
- 3つのタイプに自由に切替可能

---

#### test_match_option_toggles

**目的**: 検索マッチオプション（大文字小文字・単語単位・正規表現）のトグル動作を検証する。

**フロー**:
各オプションについて：
1. 初期値 `false` を確認
2. トグル → `true`
3. トグル → `false`

**検証仕様**:
| オプション | メソッド | 初期値 |
|---|---|---|
| `match_case` | `toggle_match_case` | `false` |
| `match_whole_word` | `toggle_match_whole_word` | `false` |
| `use_regex` | `toggle_use_regex` | `false` |

---

### navigation_test

`src/pages/explorer/tests/navigation_test.rs` — ディレクトリ移動と履歴管理を検証する。

---

#### test_navigate_to_updates_entries

**目的**: `change_dir` でディレクトリを移動した際にエントリ一覧が更新されることを検証する。

**フロー**:
1. tempdir に `subdir/file_in_sub.txt` と `root_file.txt` を作成
2. ルートでは `entries.len() == 2`（subdir + root_file）
3. `change_dir(subdir)` 実行
4. `cwd` と `entries` が subdir の内容に更新されることを検証

**検証仕様**:
| 移動後 | 期待値 |
|---|---|
| `cwd` | subdir のパス |
| `entries.len()` | `1`（file_in_sub.txt のみ） |

---

#### test_navigate_adds_to_history

**目的**: ディレクトリ移動で履歴が蓄積されることを検証する。

**フロー**:
1. tempdir に `dir_a`, `dir_b` を作成
2. `change_dir(dir_a)` → 履歴に1件追加
3. `change_dir(dir_b)` → 履歴に2件以上

**検証仕様**:
- 移動ごとに `history` にエントリが追加される
- `history.len() >= 2` で少なくとも2回分の履歴がある

---

#### test_go_back_and_forward

**目的**: 履歴の戻る/進む操作を検証する。

**フロー**:
```
start → change_dir(A) → change_dir(B)
                                ↓
                         cwd == B
                    go_back ↓
                         cwd == A
                    go_forward ↓
                         cwd == B
```

**検証仕様**:
- `go_back()` で1つ前のディレクトリに戻る
- `go_forward()` で進む方向に移動する
- 各操作後の `cwd` が正しいパスになっている

---

### sort_test (explorer)

`src/pages/explorer/tests/sort_test.rs` — ExplorerPage でのソート適用を検証する。

---

#### test_sort_entries_applied_on_reload

**目的**: `reload` 後にデフォルトソート（Name 昇順）が適用されることを検証する。

**フロー**:
1. tempdir に `charlie.txt`, `alpha.txt`, `bravo.txt` を作成（アルファベット順ではない）
2. `reload()` 実行
3. `filtered_entries` がアルファベット順になっていることを検証

**検証仕様**:
- `filtered_entries[0].name == "alpha.txt"`
- `filtered_entries[1].name == "bravo.txt"`
- `filtered_entries[2].name == "charlie.txt"`

---

#### test_sort_key_toggle_direction

**目的**: ソートキートグルで降順になった結果が `filtered_entries` に反映されることを検証する。

**フロー**:
1. tempdir に `alpha.txt`, `bravo.txt` を作成
2. `reload()` → `set_sort_key(Name)` でトグル（降順に）
3. `filtered_entries` が逆順になっていることを検証

**検証仕様**:
- `filtered_entries[0].name == "bravo.txt"`（降順なので B が先）
- `filtered_entries[1].name == "alpha.txt"`

---

## Core テスト

`tests/core_tests/` — ビジネスロジックの純粋関数テスト。

### search_utils_test

`tests/core_tests/search_utils_test.rs` — `group_results` と `results_to_entries` のユーティリティ関数を検証する。

---

#### test_group_results_same_file_grouped

**目的**: 同一ファイルの複数マッチが1グループにまとまることを検証する。

**検証仕様**:
- 入力: 同じ path の `SearchResult` 2件
- 出力: `grouped.len() == 1`, `grouped[0].matches.len() == 2`

---

#### test_group_results_different_files

**目的**: 異なるファイルのマッチが別グループになることを検証する。

**検証仕様**:
- 入力: 異なる path の `SearchResult` 2件
- 出力: `grouped.len() == 2`

---

#### test_group_results_line_number_zero_skipped

**目的**: `line_number == 0` のエントリ（ファイル名マッチ等）が空マッチとして処理されることを検証する。

**検証仕様**:
- `line_number == 0` → `matches` は空（スニペット表示なし）

---

#### test_group_results_sorted_by_path

**目的**: グループ化後の結果がパスのアルファベット順でソートされることを検証する。

**検証仕様**:
- 入力: `z.rs`, `a.rs` の順
- 出力: `a.rs` が先

---

#### test_group_results_empty

**目的**: 空入力に対して空出力を返すことを検証する。

---

#### test_group_results_folder_extraction

**目的**: フルパスからフォルダ名とファイル名が正しく抽出されることを検証する。

**検証仕様**:
- `/home/user/project/src/main.rs` → `folder == "src"`, `filename == "main.rs"`

---

#### test_results_to_entries_real_file

**目的**: `SearchResult` から実ファイルのメタデータ（サイズ・更新日時等）を含む `FileEntry` に変換できることを検証する。

**フロー**:
1. tempdir に実ファイルを作成
2. `results_to_entries` で変換
3. `size > 0`, `modified` が空でないことを検証

---

#### test_results_to_entries_nonexistent_file_no_panic

**目的**: 存在しないファイルパスの SearchResult を渡してもパニックしないことを検証する。

**検証仕様**:
- メタデータ取得失敗時は `size == 0` 等のフォールバック値が使われる
- パニックは発生しない

---

### sort_test (core)

`tests/core_tests/sort_test.rs` — `sort_entries` と `get_extension` の純粋ロジックを検証する。

---

#### test_sort_entries_name_asc / test_sort_entries_name_desc

**目的**: 名前ソートの昇順/降順を検証する。

**検証仕様**:
- 昇順: `alpha` → `bravo` → `charlie`
- 降順: `charlie` → `bravo` → `alpha`

---

#### test_sort_entries_size

**目的**: ファイルサイズによるソートを検証する。

---

#### test_sort_entries_modified

**目的**: 更新日時によるソートを検証する。

---

#### test_sort_entries_type

**目的**: ファイル拡張子によるソートを検証する。

---

#### test_sort_entries_directories_always_first

**目的**: ソートキーに関わらずディレクトリが常にファイルより前に来ることを検証する。

**検証仕様**:
- `kind == "dir"` のエントリは常に先頭に配置される
- ファイル同士、ディレクトリ同士はソートキーに従う

---

#### test_sort_entries_directories_first_desc

**目的**: 降順ソートでもディレクトリが先頭に来ることを検証する。

**検証仕様**:
- ディレクトリは先頭のまま、ディレクトリ同士は降順ソート

---

#### test_sort_entries_empty

**目的**: 空配列のソートが安全であることを検証する。

---

#### test_get_extension_*

`get_extension` ユーティリティ関数の各ケースを検証する。

| テスト | 入力 | 期待値 | 用途 |
|---|---|---|---|
| `_file` | `"main.rs"` (file) | `"rs"` | 拡張子抽出 |
| `_dir` | `"src"` (dir) | `"0_dir"` | ディレクトリ優先ソート用 |
| `_no_ext` | `"Makefile"` (file) | `"zzz_noext"` | 拡張子なしファイルを末尾に |
| `_other_kind` | kind=`"symlink"` | `"symlink"` | 未知のkindはそのまま返す |

---

### text_test

`tests/core_tests/text_test.rs` — テキスト処理ユーティリティを検証する。

---

#### truncate_middle テスト群

`truncate_middle(text, max_len)` — ファイル名等を中央省略して短縮する関数。

| テスト | 入力 | max_len | 期待値 | 仕様 |
|---|---|---|---|---|
| `_short_text` | `"hi.txt"` | 20 | `"hi.txt"` | max 未満はそのまま |
| `_exact_length` | 10文字 | 10 | そのまま | ちょうどはそのまま |
| `_with_extension` | 長いファイル名 | 15 | `"long...name.txt"` 形式 | 拡張子を保持して中央省略 |
| `_without_extension` | 拡張子なし長い名前 | 10 | `"abcd...xyz"` 形式 | 拡張子なしでも中央省略 |
| `_multibyte` | 日本語ファイル名 | 10 | マルチバイト対応省略 | 文字境界を正しく処理 |
| `_very_short_max` | `"abcdef"` | 3 | `"abc"` | 最小ケース |

---

#### find_query_match_ranges テスト群

`find_query_match_ranges(text, query)` — テキスト中のクエリマッチ範囲（バイトオフセット）を返す関数。

| テスト | 入力 | 期待動作 |
|---|---|---|
| `_basic` | `("hello world", "world")` | `[(6,11)]` |
| `_case_insensitive` | `("Hello World", "hello")` | `[(0,5)]` — 大文字小文字無視 |
| `_multiple_matches` | `("ab ab ab", "ab")` | `[(0,2),(3,5),(6,8)]` — 全マッチ |
| `_empty_query` | `("text", "")` | `[]` — 空クエリは結果なし |
| `_multibyte` | `("日本語テスト", "テスト")` | バイトオフセットが正しい |
| `_no_match` | `("hello", "xyz")` | `[]` |
| `_partial_overlap` | `("aaa", "aa")` | `[(0,2)]` — 非重複マッチ |
| `_query_longer_than_text` | `("hi", "hello world")` | `[]` |

---

## Service テスト

`tests/service_tests/` — 外部リソースアクセス層のテスト。

### indexing_test (service)

`tests/service_tests/indexing_test.rs` — `IndexManager` の単体テスト (7テスト)。

---

#### test_index_create_and_filename_search

**目的**: ファイルをインデックスに追加し、ファイル名で検索できることを検証する。

**フロー**:
1. tempdir にファイル作成
2. `IndexManager::new()` → `index_directory()` でインデックス構築
3. ファイル名で検索 → 結果にそのファイルが含まれることを検証

---

#### test_index_create_and_content_search

**目的**: ファイル内容でインデックス検索し、行番号と行内容が正しく返されることを検証する。

**検証仕様**:
- 検索結果の `line_number` と `line_content` がファイル内容と一致する

---

#### test_update_file_reindexes

**目的**: ファイル更新後に再インデックスが正しく動作することを検証する。

**フロー**:
1. ファイル作成 → インデックス
2. ファイル内容変更 → `update_file()` で再インデックス
3. 旧内容で検索 → ヒットしない
4. 新内容で検索 → ヒットする

---

#### test_remove_file

**目的**: ファイル削除時にインデックスから除外されることを検証する。

**フロー**:
1. ファイル作成 → インデックス → 検索ヒット確認
2. `remove_file()` → 検索ヒットしないことを確認

---

#### test_index_japanese_content

**目的**: 日本語ファイル名・内容のインデックス/検索が正しく動作することを検証する。

---

#### test_index_large_file_count

**目的**: 大量ファイル（120件）のインデックスが正常に完了することを検証する。

**検証仕様**:
- ドキュメント数が120以上（メタデータ含む可能性があるため `>=`）

---

#### test_index_empty_directory

**目的**: 空ディレクトリのインデックスが正常に完了することを検証する。

---

### ripgrep_test

`tests/service_tests/ripgrep_test.rs` — `RipgrepBackend` のテスト (5テスト)。

---

#### test_ripgrep_basic_search

**目的**: ripgrep による基本的なテキスト検索を検証する。

**フロー**:
1. tempdir にファイル作成
2. `RipgrepBackend::search()` でテキスト検索
3. 結果にマッチ行が含まれることを検証

---

#### test_ripgrep_case_sensitive

**目的**: 大文字小文字を区別した検索の動作を検証する。

---

#### test_ripgrep_binary_skipped

**目的**: バイナリファイル（null バイト含有）が検索結果から除外されることを検証する。

---

#### test_ripgrep_line_number_and_content

**目的**: 複数行ファイルで行番号と行内容が正しく抽出されることを検証する。

**検証仕様**:
- マッチ行の `line_number` が実際のファイル中の行位置と一致
- `line_content` がその行の内容と一致

---

#### test_ripgrep_gitignore_respected

**目的**: `.gitignore` に記載されたパターンのファイルが検索対象から除外されることを検証する。

**フロー**:
1. tempdir に `.gitignore` (`*.log`) と `test.log`, `test.txt` を作成
2. `git init` でリポジトリ初期化
3. 検索実行 → `test.log` は結果に含まれないことを検証

---

### search_engine_test

`tests/service_tests/search_engine_test.rs` — 検索エンジン統合テスト (3テスト)。

---

#### test_index_manager_home_scope_uses_index

**目的**: `SearchScope::Home` での検索がインデックスバックエンドを使用することを検証する。

---

#### test_ripgrep_root_scope_search

**目的**: `SearchScope::Root` での検索が ripgrep バックエンドを使用することを検証する。

---

#### test_progress_tracking

**目的**: インデックス構築の進捗追跡が正しく動作することを検証する。

**検証仕様**:
- `tokio::sync::watch` チャネルで進捗を受信
- インデックス完了後に進捗値が `1.0` に到達する

---

### watcher_test (service)

`tests/service_tests/watcher_test.rs` — `FileWatcher` のテスト (4テスト)。

---

#### test_watcher_detects_file_creation

**目的**: 新規ファイル作成イベントを検知することを検証する。

**フロー**:
1. tempdir を監視開始
2. ファイルを作成
3. イベント受信 → 作成されたファイルのパスが含まれることを検証

---

#### test_watcher_detects_file_modification

**目的**: 既存ファイルの変更イベントを検知することを検証する。

---

#### test_watcher_detects_file_deletion

**目的**: ファイル削除イベントを検知することを検証する。

---

#### test_watcher_debounce_batching

**目的**: 短時間に連続する変更がデバウンスされてバッチにまとまることを検証する。

**検証仕様**:
- 急速な連続変更が1つのイベントバッチとして届く
- デバウンス間隔内の変更がまとめられる

---

## 統合テスト

### indexing_test (integration)

`tests/indexing_test.rs` — インデックスの E2E ワークフローテスト (1テスト)。

#### test_indexing_workflow

**目的**: インデックスの作成→検索→更新→削除の全ワークフローを一貫して検証する。

**フロー**:
```
1. tempdir にファイル群作成
2. IndexManager でインデックス構築
3. 検索 → ヒット確認
4. ファイル更新 → 再インデックス → 旧内容ヒットなし・新内容ヒット
5. ファイル削除 → インデックスから除去 → ヒットなし
6. 各段階でドキュメント数を検証
```

---

### watcher_test (integration)

`tests/watcher_test.rs` — ファイル監視の E2E テスト (1テスト)。

#### test_watcher_detects_changes

**目的**: ファイル監視の基本動作を検証する最小限の統合テスト。

**フロー**:
1. tempdir を `FileWatcher` で監視開始
2. ファイル作成
3. イベント受信 → ファイルパスが含まれることを検証
