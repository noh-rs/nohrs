# CLAUDE.md - nohrs

## Project Overview

macOS Finder を置き換えるファイルエクスプローラ。Rust + GPUI + Tokio。

- リポジトリ: https://github.com/noh-rs/nohrs
- GUI: `gpui` 0.2 / `gpui-component` 0.3（`--features gui` で有効化）
- 非同期: `tokio`（ファイルシステム走査、プレビュー読み込み等の重い I/O）
- ファイル監視: `notify` 6
- 構文ハイライト: `syntect` 5
- Markdown: `comrak` 0.22

## AI Agent Behavior

### 基本姿勢

- 実装前にテストを書く。テストが通らない状態で作業を終えない
- 単一ファイルを肥大化させない。分割基準に従う
- 既存挙動を壊さない。リファクタ時は内部構造のみ変更する
- 推測で実装しない。不明点はソースを読んで確認する

### 開発ループ

```
1. #[gpui::test] テストを書く
2. cargo test で失敗確認
3. 実装
4. cargo test で全テスト通過確認
5. cargo run --features gui で動作確認が必要な場合はユーザーに依頼
```

### テストの書き方

- `TestAppContext` を使う。OS環境・GPU不要
- 非同期処理を発火したら `cx.run_until_parked()` してからアサーション
- ウィンドウが必要なテストは `cx.add_window_view()` で `VisualTestContext` を取得
- マウスイベントのシミュレート前に `cx.draw()` で要素ツリーを描画する
- `simulate_keystrokes` / `simulate_input` は内部で `run_until_parked()` を呼ぶので追加不要
- イベント検証は `cx.events()` または `entity.next_event()` を使う
- プラットフォーム操作（ファイルダイアログ等）は `TestPlatform` がシミュレートする。直接モックを書かない

### コマンド

```bash
cargo test                     # 全テスト
cargo test test_name           # 特定テスト
SEED=42 cargo test test_name   # シード指定で再現
cargo run --features gui       # GUI起動（AIが直接実行する場面はない）
```

## Code Style

- `unwrap()` はテストコードのみ許可。プロダクションコードでは `Result` を返す
- エンティティ間の通信は `EventEmitter` + `cx.subscribe()`。直接参照を保持しない
- グローバル状態は `Global` トレイト経由。`static mut` 禁止
- `std::thread::spawn` 禁止。`cx.background_executor()` を使う
- `std::sync::Mutex` 禁止。GPUI のエンティティモデルが排他制御を担う
- テストで `std::thread::sleep` 禁止。`cx.run_until_parked()` を使う
- `Rc<RefCell<T>>` の多用禁止。`Entity<T>` に置き換える
- `unsafe` 禁止（FFI 境界を除く）

## Architecture - ディレクトリ構成

```
src/
├── lib.rs
├── core/           # エラー型、テレメトリ等の基盤
├── models/         # Entity<T> で管理されるビジネスロジック
├── services/       # ファイルシステム等の外部リソースアクセス
├── ui/             # 共有UIコンポーネント
├── gui/            # GUI起動・アプリケーション初期化
└── pages/          # ページ実装（以下の分割ルールに従う）
```

## Page Module Rules（最重要）

### ディレクトリ化の基準

`src/pages/<page>.rs` が **500行を超える、または render 周辺が増えてきたら**必ずディレクトリへ移行する。

### ディレクトリ構成例（Explorer）

```
src/pages/explorer/
├── mod.rs          # ExplorerPage 本体・new / focus_handle・pub(super) 状態
├── types.rs        # enum / struct 等の型
├── entries.rs      # 一覧・ディレクトリ移動・ソート・ロード
├── search.rs       # 検索関連
├── event.rs        # click / resize / activate 等のイベント処理
└── view/           # 描画（view.rs 1枚にしない）
    ├── mod.rs      # Render 骨組み + render_root
    ├── util.rs     # path_parts / truncate_middle 等の小物
    ├── header.rs   # render_header / breadcrumb / view mode toggle
    ├── sidebar.rs  # render_sidebar / shortcuts
    ├── listing/
    │   ├── mod.rs  # render_listing の分岐のみ
    │   ├── list.rs # render_list_view / virtual list 呼び出し
    │   ├── row.rs  # render_file_row_static（Closure 隔離）
    │   └── grid.rs # render_grid_view / render_grid_item
    ├── search_bar.rs
    └── preview/
        ├── mod.rs          # render_preview の骨組み
        ├── highlight.rs    # クエリハイライト処理
        └── virtual_scroll.rs
```

### 可視性ルール

- Page struct のフィールドは原則 **private**
- ページ配下の submodule から参照が必要なものは **`pub(super)`** を許可
- **`pub`** は原則禁止（ページ外へ状態を漏らさない）
- `explorer/mod.rs` は submodule を `pub mod` で宣言し、外部に必要な型だけ `pub use`
- view 配下の関数は原則 `pub(super)`

### view 分割ルール

**view/mod.rs は「画面の骨組み」だけ：**
- OK: `render_root` / `child(...)` の接着、最低限のイベント束ね
- NG: 行レンダリング、ハイライト処理、仮想スクロール計算、検索UIの細部、文字列処理

**view が 300行を超えそうなら view/ ディレクトリ化して feature 単位に分割する。**

### 関数サイズ・分割基準

- `render_*` 関数は **80〜120行**を上限とする
- 超えた場合は以下を別関数・別ファイルへ分離：
  - 計算（range算出、表示用データ整形）
  - 文字列処理（truncate、検索、highlight）
  - UI部品生成（ボタン群、行、セル、パネル単位）
- **1ファイルは原則400行以内**。超えたら分割を優先する

### Closure 肥大化への対処（GPUI 特有）

`v_virtual_list` や `WeakEntity` を使うと closure が肥大しやすい。

- closure が **30行を超えたら** static renderer に切り出す
- 行レンダリングは `listing/row.rs` に隔離
- `list.rs` は virtual list の枠と `render_file_row_static(...)` 呼び出しだけにする

```rust
// listing/list.rs -- 枠だけ
v_virtual_list(..., |view, range, ...| {
    render_file_row_static(...)
})

// listing/row.rs -- 行の中身
pub(super) fn render_file_row_static(...) -> impl IntoElement { ... }
```

### "純粋ロジック" を view から追い出す

view は「Element を組み立てる」責務のみ。

- 文字列処理 → `view/*/highlight.rs`、`view/util.rs`
- 仮想スクロール計算 → `view/preview/virtual_scroll.rs`
- 表示用データ構築（match_snippets、display_name、icon判定等） → `view/listing/*` 内の helper

UI要素を返さない処理が増えたら、必ず専用ファイルへ切り出す。

## Refactoring Verification

リファクタ後は以下をすべて確認する。

1. `cargo test` が通る
2. `ExplorerPage` を import しているテストの参照パスと可視性が正しい
3. GUI起動（`cargo run --features gui`）で以下が同一挙動で動く：
   - ディレクトリ移動・一覧ロード
   - 検索（ファイル名・内容）とハイライト
   - ソート
   - プレビュー
   - カラムリサイズ
   - ViewMode 切替（Grid / List）

GUI の動作確認はユーザーに依頼する。AIが直接確認できるのは `cargo test` まで。

## Commit Rules

- 1コミット1目的。テスト追加と実装は同一コミットでよい
- コミットメッセージは日本語可。変更の意図を書く
- テストが通らない状態でコミットしない

## Do NOT

- `render()` にビジネスロジックを書かない
- 1ファイルに400行以上詰め込まない
- `Box<dyn Any>` でダウンキャストしない。GPUI の Entity 型システムを使う
- プラットフォーム API を直接呼ばない。コンテキスト経由でアクセスする
- テストなしで機能追加しない
