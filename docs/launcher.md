# Launcher

> Status: Draft (P3 で実装)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/search.md`](./search.md), [`docs/plugin-api.md`](./plugin-api.md)

本書は Raycast 風グローバルランチャーの設計を定めます。Launcher × Explorer の "Launcher" 側です。

---

## 1. ウィンドウモデル

| 観点 | 仕様 |
|------|------|
| **モデル** | 別ウィンドウ (フローティング)。explorer とは独立した window |
| **表示位置 (初回)** | 画面中央寄り上 (画面上端から 25%) |
| **表示位置 (2 回目以降)** | 最後にユーザーが置いた位置を記憶し、その位置で起動 |
| **位置の記憶** | SQLite `key_value` テーブル (`launcher.window_position`) に保存。高頻度更新のため config.toml ではない |
| **移動** | 検索バー上部の数十 px (drag handle) をマウスでドラッグして移動可能 |
| **位置リセット** | `Cmd+0` でデフォルト位置に戻す |
| **マルチディスプレイ** | ドラッグで別ディスプレイへ移動可、移動先で位置記憶 |
| **サイズ** | **750 × 500** 固定 (リサイズ不可、検索 UI なので幅固定で十分) |
| **デコレーション** | borderless + 丸角 + blur background (mica / vibrancy) |
| **フォーカス喪失時** | 自動 close (`ESC` でも close)、設定で disable 可 |
| **再 hotkey** | 既に開いている場合は toggle (close)。位置は記憶のまま |
| **起動アニメ** | fade-in 100ms |

---

## 2. 起動キー

| キー | デフォルト | 動作 |
|------|-----------|------|
| **グローバルホットキー** | `Cmd+Shift+Space` | OS グローバル登録、別アプリ前面でも launcher を起動 |
| **アプリ内ショートカット** | `Cmd+K` | explorer focus 時に launcher を起動 |
| **menubar アイコン** | (P5 で検討) | 常駐 process が必要 |

`Cmd+Space` は macOS Spotlight 占有のため奪わない (デフォルト)。設定で上書き可。

### 実装

- crate: **`global-hotkey`** (Carbon 経由、pure Rust、tray-icon 系と同 author)
- macOS: Carbon Hot Keys API、Linux: X11 / Wayland

---

## 3. ホーム画面 (起動直後・入力前)

**Super minimal + placeholder ヒント** を採用:

- 検索バーのみ表示
- placeholder: `Search files or commands…    ? for commands, > to run`
- 結果リストは空 (入力するまで何も表示しない)

理由:
- ノイズゼロ (Spotlight 流の美しさ)
- 学習導線として placeholder にヒントを書く (`?` / `>` の入り口)

---

## 4. アクションフレームワーク

### 4.1 `Command` trait

```rust
// crates/nohrs-launcher/src/command.rs

pub trait Command: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn title(&self) -> SharedString;
    fn subtitle(&self) -> Option<SharedString>;
    fn icon(&self) -> Icon;
    fn keywords(&self) -> &[&'static str];
    fn category(&self) -> Category;
    fn mode(&self) -> Mode;
    fn arguments(&self) -> &[ArgSpec];
    fn default_hotkey(&self) -> Option<KeyChord>;
    fn execute(&self, ctx: &CommandContext, args: &Args) -> CommandResult;
}

inventory::collect!(&'static dyn Command);
```

`inventory` クレートで linker-time にコマンドを集める。各 crate (`nohrs-pages`, `nohrs-services`, `nohrs-launcher` 自体) が自身のコマンドを `inventory::submit!` で宣言。

### 4.2 メタデータ

| フィールド | 説明 |
|-----------|------|
| `id` | グローバルユニーク (`"explorer.open_path"` 等) |
| `title` | 主表示 |
| `subtitle` | 補助情報 (optional) |
| `icon` | 16-24px、SF Symbols 互換 or 自前 SVG |
| `keywords` | 検索マッチ強化用 (例: "calc", "math") |
| `category` | "Productivity", "Developer Tools", "Media", "Cloud", "Theme" |
| `mode` | `Instant` (即実行)、`View` (結果を launcher 内に表示)、`External` (別 window 開く) |
| `arguments` | `Vec<ArgSpec>` (string / path / number / enum)、検索バーに inline 入力 `> command arg1 arg2` |
| `default_hotkey` | 任意のコマンド固有グローバルホットキー |

### 4.3 plugin command (P4)

`plugin.toml` の `[[commands]]` セクションで宣言、WIT 経由 `run_command(id, args, ctx)` で呼び出し。host 側で `Command` trait の adapter 実装で `inventory` レジストリに登録するので、コア plugin と同じレジストリで検索可能。

詳細は [`docs/plugin-api.md`](./plugin-api.md) §commands interface。

---

## 5. 結果リスト

### レイアウト

```text
┌────────────────────────────────────────────────────┐
│ 🔍  [icon]  Title text             [kind]   ⌘K    │
│             Subtitle text                          │
└────────────────────────────────────────────────────┘
```

| 要素 | 説明 |
|------|------|
| icon | 16-24px |
| title | 主表示 (ファイル名 / コマンド名) |
| subtitle | 補助情報 (フルパス / 説明 / 最終更新) |
| kind badge | "File" / "Folder" / "Command" / "Plugin" / "Calc" 等 (右寄せ、subtle) |
| accessory | ショートカット表示 (右端、subtle) |

### Section 分け

- "Recent" (使用履歴)
- "Files" (検索結果のファイル)
- "Commands" (検索結果のコマンド)
- "Calculations" (式の自動評価結果)
- "Plugins" (plugin が提供する候補)

各セクション折りたたみ可。

### ハイライト

マッチした文字を bold + accent カラーでハイライト。

---

## 6. ランキング (nucleo + boost)

| 観点 | 仕様 |
|------|------|
| マッチャー | **`nucleo` crate** (helix-editor 製、高性能 fuzzy matcher) |
| 基本スコア | nucleo の scoring 関数 |
| recency boost | `last_used_at` が新しいほどスコア加算 |
| frequency boost | `use_count` が多いほどスコア加算 |
| context boost | 現在の context (`explorer focused` / `launcher only`) に合致するカテゴリを優先 |
| 永続化 | SQLite `history` テーブル + `command_usage` テーブル (`kind=command` で記録) |

---

## 7. 詳細ペイン

**必要時のみ表示** (デフォルトは結果リストのみで動作軽快)。

| トリガ | 動作 |
|--------|------|
| `Tab` | 詳細ペインを open/close toggle |
| file 選択時 | ファイル preview ペイン (画像/テキスト/メタ) |
| command 選択時 | コマンド説明 + 引数フォーム |

詳細ペインの中身は plugin から push 可能 (`launcher.push-view`)。詳細は [`docs/plugin-api.md`](./plugin-api.md) §`view-node`。

---

## 8. ナビゲーション (push-pop モデル)

Raycast 流のスタック型ナビ:

| 操作 | 動作 |
|------|------|
| コマンド実行 (mode=View) | スタックを push、新しいビューを表示 |
| `ESC` | 1 ステップ pop、最後の pop で launcher close |
| `Cmd+W` | スタッククリアして launcher close |
| 矢印キー | 結果リスト内の移動 (push しない) |
| breadcrumb top-bar | スタックの history を表示 |

---

## 9. 初期コアコマンド (P3 で実装、15-20 個)

| 領域 | コマンド |
|------|---------|
| File operations | `Open Path`, `Reveal in Finder`, `Quick Open File`, `Open Recent` |
| Navigation | `Go to Path`, `Bookmark Path`, `Recent Folders` |
| Quick calc | `Calculator` (入力が数式なら自動)、`Unit Convert` |
| System | `Quit`, `Settings`, `Reload`, `About` |
| Search | `Search Files` (current scope), `Search Content` (FTS5 全文検索 P3 V2) |

---

## 10. アクション (item 単位)

各結果アイテムに `Vec<Action>` を持ち、右クリックメニュー or `Enter` (primary) / `Cmd+Enter` (secondary) で起動:

```rust
struct Action {
    id: String,
    title: String,
    shortcut: Option<KeyChord>,
    icon: Option<Icon>,
}
```

例: ファイル選択時のアクション:
- Open (Enter, default editor)
- Reveal in Explorer (Cmd+Enter)
- Copy Path (Cmd+Shift+C)
- Open in Finder (Cmd+R)

---

## 11. 実装上のポイント

- crate: **`nohrs-launcher`** (P3 で新規)
- window: GPUI で borderless + blur 化、`window.set_window_level(.floating)`
- global hotkey: `global-hotkey` crate
- ranking: `nucleo`
- 位置記憶: `nohrs-store::KvStore` (SQLite `key_value`) 経由

---

## 12. パフォーマンス目標

- グローバルホットキーから launcher window 表示: **<100ms**
- キー入力から結果リスト更新: **<50ms** (debounced)
- 検索結果取得 (全文検索含む): **<500ms** 中央値
