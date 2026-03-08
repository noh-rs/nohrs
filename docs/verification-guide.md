# 検索機能改善 動作確認ガイド

本ドキュメントは `claude/fix-search-issues-62eKv` ブランチで実装した全修正・新機能の動作確認手順をまとめたものです。

## 自動テスト

### テスト実行コマンド

```bash
# 全テスト一括実行
cargo test

# 個別テスト実行
cargo test test_debounce              # デバウンステスト (4件)
cargo test test_pagination            # ページネーションテスト (6件)
cargo test test_search_flow           # 検索フローテスト (6件)
cargo test test_search_state          # 検索状態テスト (5件)
cargo test test_search_scope          # 検索スコープテスト (3件)
cargo test config::tests              # 設定ファイルテスト (5件)
cargo test ripgrep_test               # Ripgrep バックエンドテスト (5件)
cargo test indexing_test              # インデックステスト (8件)
cargo test search_engine_test         # 検索エンジンテスト (3件)
```

### テストカバレッジ一覧

| カテゴリ | ファイル | テスト数 | 確認内容 |
|---------|---------|---------|---------|
| Config | `src/core/config.rs` | 5 | TOML読み込み、デフォルト値、パス展開 |
| Debounce | `tests/debounce_test.rs` | 4 | タイマー発火、キャンセル、空入力、Enter バイパス |
| Pagination | `tests/pagination_test.rs` | 6 | ページ遷移、境界、リセット |
| Search Flow | `tests/search_flow_test.rs` | 6 | 基本フロー、エラー、復帰 |
| Search State | `tests/search_state_test.rs` | 5 | ライフサイクル、展開/折畳み |
| Search Scope | `tests/search_scope_test.rs` | 3 | スコープ切替、オプショントグル |
| Ripgrep | `tests/service_tests/ripgrep_test.rs` | 5 | 正規表現、大文字小文字、バイナリ除外 |
| Indexer | `tests/service_tests/indexing_test.rs` | 8 | インデックス作成・更新・削除・日本語 |
| Engine | `tests/service_tests/search_engine_test.rs` | 3 | スコープ振り分け、プログレス |

---

## GUI 手動確認

GUI 起動コマンド:

```bash
cargo run --features gui
```

### 1. 設定ファイル (`~/.nohrs/nohrs.toml`)

**確認手順:**

1. `~/.nohrs/nohrs.toml` を作成:
   ```toml
   [index]
   directories = ["~/Documents", "~/Projects"]
   exclude_patterns = ["node_modules", ".git", "target", "*.log"]
   max_file_size = 5242880  # 5MB

   [search]
   debounce_ms = 500
   max_results_per_page = 50
   ```
2. アプリを起動
3. Home スコープで検索し、`~/Documents` と `~/Projects` 両方から結果が返ることを確認
4. `node_modules/` 配下のファイルが結果に含まれないことを確認
5. 設定ファイルを削除してアプリを再起動 → デフォルト (`~/Documents` のみ) で動作すること

**確認ポイント:**
- [ ] 複数ディレクトリがインデックス対象になる
- [ ] 除外パターンが適用される
- [ ] 設定ファイルなしでもデフォルトで起動する
- [ ] 不正な TOML でもクラッシュしない (ログに警告が出る)

---

### 2. デバウンス付きオートサーチ

**確認手順:**

1. `Cmd+F` で検索バーを表示
2. 検索欄に文字を **ゆっくり** 入力 (例: `hello`) → 入力停止後 300ms で自動検索が実行される
3. 検索欄に文字を **素早く** 入力 → 入力中は検索されず、停止後にのみ 1 回検索される
4. `Enter` キーを押す → デバウンスをバイパスして即座に検索実行

**確認ポイント:**
- [ ] 入力停止後に自動で検索が開始される
- [ ] 連続入力中は中間の検索がスキップされる
- [ ] Enter キーで即座に検索される
- [ ] 入力欄を空にすると検索結果がクリアされ元のファイル一覧に戻る

---

### 3. 検索結果のページネーション

**確認手順:**

1. 大量の結果が返るクエリで検索 (例: `import`, `use`)
2. ステータス行に `N matches (1/M)` のようなページ情報が表示されることを確認
3. ▶ ボタンで次ページに遷移
4. ◀ ボタンで前ページに戻る
5. 最初のページで ◀ が無効 (グレー) であること
6. 最後のページで ▶ が無効であること
7. 新しいクエリで検索するとページが 1 にリセットされること

**確認ポイント:**
- [ ] ページ番号が正しく表示される
- [ ] ▶ ◀ ボタンでページ遷移できる
- [ ] 端のページでボタンが無効化される
- [ ] 結果が少ない場合はページネーションが表示されない
- [ ] 新規検索でページリセットされる

---

### 4. ローディングインジケーター

**確認手順:**

1. Root スコープ (全ディスク検索) に切り替え
2. クエリを入力して検索実行
3. 検索中にステータス行に **"Searching..."** がアクセントカラーで表示されることを確認
4. 検索完了後にマッチ件数表示に切り替わること

**確認ポイント:**
- [ ] 検索中は "Searching..." 表示
- [ ] アクセントカラーで強調される
- [ ] 検索完了後は件数表示に切り替わる

---

### 5. P0-P2 修正の回帰確認

以下の既存機能が正常に動作することを確認する。

#### 5.1 UI スレッドブロック解消 (4.1.1)

- [ ] 大量ファイルの検索中に UI が固まらない
- [ ] スクロール・リサイズ等が検索中も可能

#### 5.2 検索オプション (4.1.2)

| オプション | 確認内容 |
|-----------|---------|
| Aa (大文字小文字) | ON: `Hello` と `hello` を区別する / OFF: 区別しない |
| ab (全単語一致) | ON: `test` で `testing` にマッチしない / OFF: マッチする |
| .* (正規表現) | ON: `fn\s+\w+` がマッチ / OFF: リテラル検索 |
| スコープ (Home/Root) | Home: `~/Documents` 以下 / Root: `/` 以下 |
| タイプ (All/Filename/Content) | 検索対象の切替 |

#### 5.3 特殊文字のエスケープ (4.1.2, 4.4.3)

- [ ] `[brackets]` をリテラル検索してもクラッシュしない
- [ ] `NOT found` を検索しても Tantivy の Boolean 演算にならない
- [ ] `.` `*` `(` `)` 等をリテラルとして検索できる

#### 5.4 プレビュー (4.3.5)

- [ ] ファイル選択時にプレビューがスムーズに表示される
- [ ] 大きなファイルのプレビューで UI が固まらない
- [ ] 検索結果からファイルを選択するとマッチ行にスクロールする

#### 5.5 基本操作

- [ ] ディレクトリ移動・戻る・進む
- [ ] ソート切替 (名前/サイズ/更新日/種類)
- [ ] ViewMode 切替 (List / Grid)
- [ ] カラムリサイズ
- [ ] 検索バーの開閉 (`Cmd+F` / `Escape`)

---

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | `toml` 依存追加 |
| `src/core/config.rs` | **新規** — AppConfig 設定システム |
| `src/core/mod.rs` | config モジュール公開 |
| `src/services/search/indexer.rs` | 複数ディレクトリ、除外パターン、設定読込 |
| `src/services/search/engine.rs` | 設定ベースの監視ディレクトリ |
| `src/pages/explorer/mod.rs` | デバウンス、ページネーション、ローディング |
| `src/pages/explorer/view/listing/search_bar.rs` | UI: ローディング・ページネーション表示 |
| `src/pages/explorer/view/listing/mod.rs` | search_bar に window 引数追加 |
| `src/pages/explorer/tests/debounce_test.rs` | **新規** — デバウンステスト |
| `src/pages/explorer/tests/pagination_test.rs` | **新規** — ページネーションテスト |
| `src/pages/explorer/tests.rs` | テストモジュール登録 |
