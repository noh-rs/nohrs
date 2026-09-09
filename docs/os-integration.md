# OS Integration — Finder 代替のための OS レベル統合

> Status: Draft (P1 で骨子、P2 で詳細化・実装)
> Related: [`ROADMAP.md`](./ROADMAP.md) / [`explorer-essentials.md`](./explorer-essentials.md)
> Source: [#58 のコメント](https://github.com/noh-rs/nohrs/issues/58#issuecomment-4589555237)

本書は nohrs を「Finder を起動しなくても完結するファイラー」にするための **OS レベル統合**の方針を定めます。[`explorer-essentials.md`](./explorer-essentials.md) がファイラー UI (DnD / ファイル操作 / split / tab) に閉じるのに対し、本書はアプリバンドル登録・システムイベント連携・プレビュー連携といった OS 固有のグルーを扱います。

実装は **P2 (Explorer Essentials)** で行う。大半が **アプリバンドル (`.app`) を前提**とするため、最小バンドル生成を P2 に前倒しする (本格パッケージング / `dmg` 配布・notarization は ROADMAP Phase 6)。各項目のフェーズ割当は下表の通り。

## フェーズ割当サマリ

| # | 項目 | macOS 実装 | フェーズ | 備考 |
|---|------|-----------|---------|------|
| 1 | フォルダビューア登録 | `CFBundleDocumentTypes` に `public.folder` (`Viewer`) | **P2** | 「フォルダを開くアプリ候補」になる。`.app` 前提 |
| 2 | デフォルトファイルビューア | `NSFileViewer` を bundle id に設定 | **P2** | 「Reveal in Finder / Show in Finder」系の呼び出し先になる |
| 3 | `file://` URL スキーム受理 | URL → フォルダを開く | **P2** | 最低限の受理。`odoc`/`GURL` と統合 |
| 4 | Apple Event 対応 | `odoc` (Open Documents) / `GURL` (Open URL) | **P2** | ダブルクリック・「このアプリで開く」で正しく動く |
| 5 | 外部アプリとの Drag & Drop | drag-out / drop-in (`NSFilenamesPboardType`) | **P2 (済)** | UI 側は [`explorer-essentials.md` §2](./explorer-essentials.md) で確定済。OS pasteboard 結線のみ本書 |
| 6 | Quick Look 連携 | `QLPreviewPanel` / QuickLook framework (スペースキー) | **P2→P3** | 既存 in-app preview (`nohrs-pages::explorer::preview`) と棲み分け。OS Quick Look パネル呼び出しは後続 |
| 7 | Finder Sync Extension | 右クリックメニュー / バッジ (Dropbox/Git 方式) | **Future** | 任意。別ターゲット (extension) が必要で重い |
| 8 | File Operations | copy/move/delete/rename/new folder, trash, restore | **P2 (済)** | [`explorer-essentials.md` §1](./explorer-essentials.md) で確定済。本書ではタグ等 OS 固有のみ追跡 |
| 9 | LaunchServices 登録 | インストール後 `lsregister` で認識 | **P2** | 1〜4 の登録を OS に反映させる工程 |
| 10 | Finder-less な完結 UI | サイドバー / タブ / デュアルペイン / 検索 / プレビュー / お気に入り | **P2〜P3 (済/進行)** | UI は [`explorer-essentials.md`](./explorer-essentials.md) §3〜§5 + launcher/search で達成 |

## 重要度

コメントの結論として、「Finder の代わりとして使う」レベルに到達するために実質重要なのは次の 5 点:

1. `public.folder` 登録 (#1)
2. `NSFileViewer` 登録 (#2)
3. Apple Event 対応 (#4)
4. Drag & Drop (#5) — UI は P2 済、OS 結線が残課題
5. Quick Look (#6)

ブラウザのファイルピッカー置換は別問題であり **対象外**。

## P2 で確定すべき設計論点 (骨子)

- **`Info.plist` 設計**: `CFBundleDocumentTypes` / `LSItemContentTypes` / `CFBundleURLTypes` / `NSFileViewer` の最終キー構成。
- **Apple Event ハンドラ配線**: GPUI/`objc` 経由でのイベント受信経路と、既存の起動パス (CLI 引数で開く) との統合。
- **`NSFileViewer` を自動設定するか**: インストーラで `defaults write -g NSFileViewer` を行うか、ユーザー任意のオプトインに留めるか (システム全体設定を書き換える副作用の是非)。
- **Quick Look の二択**: OS の `QLPreviewPanel` を呼ぶか、既存の in-app preview を拡張するか。スペースキーの割当 (§explorer-essentials §6 のショートカット表との整合)。
- **`lsregister` 実行タイミング**: 配布物 (dmg / pkg) のどの段で登録するか。

## Linux 等価対応 (並列 TODO)

nohrs は macOS / Linux 両対応のため、上記の macOS 固有機構には Linux 等価が必要 (P2 で詳細化):

| macOS | Linux 等価 |
|-------|-----------|
| `CFBundleDocumentTypes` / `public.folder` | XDG MIME (`inode/directory`) + `.desktop` の `MimeType` |
| `NSFileViewer` / デフォルトファイラー | `xdg-mime default nohrs.desktop inode/directory` |
| Apple Event (`odoc`/`GURL`) | `.desktop` の `Exec=%U` 引数 + D-Bus アクティベーション |
| LaunchServices / `lsregister` | `update-desktop-database` / `desktop-file-install` |
| Quick Look | OS 標準プレビューは無し → in-app preview に一本化 |
| Finder Sync Extension | Nautilus/Dolphin の extension API (任意・Future) |

## 8. 実装メモ

- OS 固有コードは `nohrs` (アプリ層) に閉じ込め、`nohrs-ui` / `nohrs-services` には漏らさない (コンポーネントライブラリの app-decoupled 方針)。
- macOS バインディングは `objc2` 系 crate を想定。`cfg(target_os = "macos")` / `cfg(target_os = "linux")` で分岐。
- DnD の OS pasteboard 結線は gpui の `on_drag` / `on_drop` から `NSFilenamesPboardType` を読み書きする層を追加 (UI ロジックは explorer-essentials §2 のまま)。
