# Explorer Essentials — DnD / File Operations / Split View / Tab

> Status: Draft (P1 で骨子、P2 で詳細化・実装)
> Related: [`ROADMAP.md`](./ROADMAP.md)

本書は「現代的なファイラー」として欲しい中核機能 (DnD、ファイル操作、スプリットビュー、タブ) の設計指針を定めます。

---

## 1. ファイル操作

### 1.1 削除

| 操作 | 動作 |
|------|------|
| Delete キー (または右クリック → Delete) | **trash に移動** (既存 `trash` crate を使用) |
| 永久削除 (Shift+Delete / macOS: Cmd+Shift+Delete、正準は §6 参照) (または右クリック → Delete Permanently) | **直接削除**、確認ダイアログ表示 |

理由: Finder / Windows Explorer の慣例に沿う。学習コストゼロ。

### 1.2 コンフリクト解決 (ペースト/ドロップ時)

ターゲットに同名ファイルが存在する場合のダイアログ:

| 選択肢 | 動作 |
|--------|------|
| **Rename** | 自動で `(2)` を付与 (`foo.txt` → `foo (2).txt`、衝突しない番号を選ぶ) |
| **Overwrite** | 上書き |
| **Skip** | このファイルだけスキップ、他は継続 |
| **Apply to all** | 残りの全ファイルに同じ判断を適用 (大量操作時の認知負荷軽減) |

### 1.3 Undo Stack

| 観点 | 仕様 |
|------|------|
| スコープ | **window 単位** (ペインを跨いで 1 stack) |
| `Cmd+Z` | 直近の操作を取り消し |
| `Cmd+Shift+Z` | redo |
| 永続化 | session 中のみ (再起動で消える)。永続化は P9+ |
| 取り消し可能な操作 | rename, copy, move, delete (trash to original), new folder |
| 取り消し不可 | permanent delete, file content edit |

理由: ペイン単位だと「Cmd+Z 効かない」混乱が起きるので window で統一。

### 1.4 進捗 UI

ファイル数千件規模の操作:
- フッターのステータスバーに進捗バー + キャンセルボタン
- 完了後は数秒間「N files moved」を表示し fade out
- エラー (アクセス権限ない等) は集約ダイアログで一覧表示

### 1.5 cross-volume

別ボリュームへの move は **自動で "copy + delete"** に変換 (UNIX `rename(2)` は同ファイルシステム内のみ)。

---

## 2. DnD (P2 必須セット)

| 操作 | P2 必須 | 備考 |
|------|---------|------|
| 同一 pane 内のフォルダ間 move | ✅ | 通常ドラッグで move、`Option` (macOS) 押下で copy |
| 別 pane へ move (split view) | ✅ | 同上 |
| 外部アプリ (Finder, VSCode 等) からの drop 受け入れ | ✅ | NSFilenamesPboardType を解釈 |
| 外部アプリへの drag-out (drop external) | ✅ | NSFilenamesPboardType を提供 |
| 複数選択ドラッグ | ✅ | 選択中の全ファイルを Pboard に積む |
| マウスホバーでフォルダ自動展開 (spring-loaded) | ❌ | P3 |
| Git ステージング drop (`git add`) | ❌ | P9+ |
| S3 upload drop | ❌ | P9+ |

### ドラッグプレビュー

- ドラッグ中はカーソルに **半透明サムネイル** + 件数バッジ
- ドラッグ先がフォルダの場合は frame 強調
- 無効ドロップ先 (たとえば parent への移動) は cursor を no-drop に変更

### cross-volume drop

別ボリューム検出時:
- デフォルトは **copy** (ユーザー意図を確認)
- ステータスバーに「Copying to another volume…」と明示
- progress UI 表示

---

## 3. スプリットビュー

### 3.1 レイアウト

| 観点 | P2 仕様 | 後続 |
|------|---------|------|
| 分割数 | **2-way 固定** (左右 or 上下) | 3+ way は P3 以降 |
| 方向 | 水平・垂直 両方 (`Cmd+\\` で垂直、`Cmd+Shift+\\` で水平)、設定で切替 | — |
| ペイン閉じる | ヘッダの × ボタン。常に最低 1 pane は残る | — |

### 3.2 ナビゲーション

| 観点 | 仕様 |
|------|------|
| pane 間フォーカス遷移 | `Cmd+1` / `Cmd+2` で直接、`Cmd+[` / `Cmd+]` で順送り |
| ナビゲーション独立性 | **独立** (左右で別ディレクトリ表示が前提) |
| 同期モード | 設定で opt-in (`[explorer] synced_panes = true`)、ON 時は両 pane が同じパスを表示 |
| 検索スコープ | `Cmd+F` (in-pane search) は active pane の current dir 配下のみ |

### 3.3 tab との関係

**tab はペイン単位** (各ペインが独自の tab bar を持つ)。

```text
┌──────────────────────────────────────────┐
│ [tab1][tab2][+]   │ [tabA][tabB][+]      │  ← tab bar (pane-local)
├──────────────────┼──────────────────────┤
│                  │                      │
│  Pane 1          │  Pane 2              │
│  (left)          │  (right)             │
│                  │                      │
└──────────────────┴──────────────────────┘
```

---

## 4. タブ

| 観点 | P2 仕様 | 後続 |
|------|---------|------|
| scope | **ペイン単位** | — |
| 復元 | 再起動時に直前の tab 群を復元 (config で disable 可) | — |
| close | `Cmd+W` で active tab を close、最後の tab を close した場合は pane も close (上記 §3 参照) | — |
| reorder | tab bar 上でドラッグ並び替え | — |
| 新規 tab | `Cmd+T` で active pane に新規 tab、デフォルトは home | — |
| **ピン留め** | ❌ P2 未対応、**P3 で実装** | P3 |
| **tab グループ** | ❌ P9+ | Future |

---

## 5. ファイル選択モデル

| 操作 | 動作 |
|------|------|
| クリック | 単一選択 |
| `Shift+クリック` | 範囲選択 |
| `Cmd+クリック` | 個別追加 |
| `Cmd+A` | 全選択 |
| `Esc` | 選択解除 |
| 矢印キー | 1 つ移動、`Shift+矢印` で範囲拡張 |

---

## 6. キーボードショートカット一覧 (P2 必須)

| 操作 | macOS | Linux |
|------|-------|-------|
| 新規 tab | `Cmd+T` | `Ctrl+T` |
| tab 閉じる | `Cmd+W` | `Ctrl+W` |
| pane 垂直分割 | `Cmd+\\` | `Ctrl+\\` |
| pane 水平分割 | `Cmd+Shift+\\` | `Ctrl+Shift+\\` |
| pane 間移動 | `Cmd+1/2`, `Cmd+[`, `Cmd+]` | `Ctrl+1/2`, `Ctrl+[`, `Ctrl+]` |
| 検索 | `Cmd+F` | `Ctrl+F` |
| undo | `Cmd+Z` | `Ctrl+Z` |
| redo | `Cmd+Shift+Z` | `Ctrl+Shift+Z` |
| copy / cut / paste | `Cmd+C/X/V` | `Ctrl+C/X/V` |
| rename | `Enter` (選択中) | `F2` |
| delete (trash) | `Delete` / `Backspace` | `Delete` |
| delete (permanent) | `Cmd+Shift+Delete` | `Shift+Delete` |
| new folder | `Cmd+Shift+N` | `Ctrl+Shift+N` |
| open path | `Cmd+Shift+G` | `Ctrl+L` |

---

## 7. アクセシビリティ

- 全操作がキーボードで完結 (DnD 以外)
- フォーカスインジケータを明示
- スクリーンリーダー対応 (P6 で詳細化)

---

## 8. 実装メモ

- explorer の state を `Vec<ExplorerState>` に変更し、pane / tab 構造を導入
- DnD は gpui の `on_drag` / `on_drop` event を使用
- ファイル操作は `nohrs-services::fs` 経由で、UI から直接 std::fs を呼ばない
- 進捗は `async-channel` で background → UI
- undo stack は `nohrs-pages::explorer::undo` で管理
