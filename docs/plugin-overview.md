# Plugin System — Overview

> Status: Draft (P4 で実装)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/plugin-api.md`](./plugin-api.md), [`docs/plugin-permissions.md`](./plugin-permissions.md), [`docs/plugin-distribution.md`](./plugin-distribution.md), [`docs/plugin-templates.md`](./plugin-templates.md), [ADR 0005 (wit-bindgen-component-model)](./adr/0005-wit-bindgen-component-model.md)

本書はプラグインシステム全体の方針 (ランタイム選択 / ライフサイクル / コアとコミュニティの分離 / バージョニング) を定めます。詳細は関連 docs を参照してください。

---

## 1. 全体アーキテクチャ

```text
                    ┌────────────────────────────────┐
                    │       nohrs (host process)      │
                    │                                 │
                    │  ┌──────────────────────────┐  │
                    │  │  nohrs-plugin-host       │  │
                    │  │  - wasmtime (sync API)   │  │
                    │  │  - WIT host imports      │  │
                    │  │  - permission guard      │  │
                    │  └────┬─────────┬───────────┘  │
                    │       │         │              │
                    └───────┼─────────┼──────────────┘
                            │         │
              wasm component│         │wasm component
                            ▼         ▼
                  ┌────────────┐  ┌────────────┐
                  │  plugin A  │  │  plugin B  │   (各 plugin は別 Store + Instance)
                  │  (Rust)    │  │   (TS)     │
                  └────────────┘  └────────────┘
```

| 要素 | 採用 |
|------|------|
| WASM runtime | **wasmtime 30+** (sync API) |
| 仕様 | **WASI Preview 2 + Component Model** |
| bindgen | **wit-bindgen** (Extism は経由しない) |
| 通信モデル | sync (host → plugin, plugin → host とも) |
| 並列性 | plugin ごとに別 wasmtime `Store` + `Instance`、host 側で `cx.background_spawn` で並列実行 |

理由:
- **Extism MVP → wit-bindgen 移行** ではなく、wit-bindgen 一直線で開始 (移行コストの二重投資を回避)
- 2026 年現在、Component Model は実用段階 (wasmtime 30+ で stable)
- ユーザー要件 "wasm preview2 の仕様" と整合

---

## 2. ライフサイクル

| 状態 | 説明 | 遷移条件 |
|------|------|---------|
| **Discovered** | manifest だけ読み込んだ状態 | 初回起動 / install 時 |
| **Loaded** | WASM モジュールが instantiate された | 必要時 lazy / `activation.mode = "eager"` 設定で起動時 |
| **Active** | host call (run_command 等) が実行中 | host call 開始時 |
| **Idle** | Loaded だが使われていない | host call 完了後 |
| **Suspended** | メモリ解放のため module を破棄、manifest は残る | 60 秒間 idle で auto suspend (設定可) |
| **Disabled** | ユーザーが disable | ユーザー操作 |
| **Uninstalled** | 完全削除 | ユーザー操作 |

### 方針

| 観点 | 仕様 |
|------|------|
| デフォルト activation | **lazy** (使われるまで Loaded しない、起動高速化) |
| eager activation | manifest で `[activation] mode = "eager"` を宣言したものだけ起動時 Load |
| activation events | "onCommand:" / "onFileType:" / "onPathChange" を contributes に宣言、イベント発火で Load |
| auto suspend | デフォルト 60 秒 idle で suspend、設定で `0` (suspend しない) や `300` 等変更可 |
| 異常終了 (wasmtime trap) | エラーログ + 該当 plugin を **24h 自動 disable** (`auto_disabled_until` カラム) |

---

## 3. コア vs コミュニティ

### 3.1 コアプラグイン (`crates/plugins/`)

- WIT を経由せず **Rust ネイティブ実装**
- 密な権限が必要な機能 (低レベル file system 等)
- 重い計算
- nohrs リポジトリに同梱 (`crates/plugins/nohrs-plugin-<name>`)
- インストール: config.toml に列挙
  ```toml
  [plugins]
  core = ["git", "calculator"]
  ```
- permission チェック完全 bypass (host 信頼コード)

### 3.2 コミュニティプラグイン

- **WASM Component Model 経由**
- ユーザー責任で permission を許可
- 軽量計算が中心、重い処理は host service 経由
- インストール先: `$XDG_DATA_HOME/nohrs/plugins/<plugin-id>/`
  ```toml
  [plugins]
  community = ["syuya2036/nohrs-plugin-example", "https://example.com/myplugin.git"]
  ```
- permission 同意必須 (詳細は [`docs/plugin-permissions.md`](./plugin-permissions.md))

### 3.3 共通 interface

| API | コア plugin | コミュニティ plugin |
|-----|------------|---------------------|
| Rust ネイティブ access | ✅ 全て | ❌ |
| WIT 経由 host imports | optional | ✅ 唯一の経路 |
| permission チェック | ❌ なし | ✅ user consent 必須 |
| 配布 | nohrs リポジトリ同梱 | 個別 install |
| update | nohrs アップデートに同期 | 個別 update |

**重要**: コア plugin も launcher の `Command` trait や explorer の `Decorator` trait を実装する形で API を統一。WIT を経由しないだけで、抽象化された interface は共通 → 将来 "コアからコミュニティへの移行" (= WIT 化して公開) が技術的に可能。

---

## 4. マニフェスト形式

`plugin.toml` (TOML、`config.toml` / `Cargo.toml` と統一):

```toml
[plugin]
id          = "syuya2036/nohrs-plugin-example"   # or "core/example"
name        = "Example"
version     = "0.1.0"
description = "Example plugin doing nothing useful."
authors     = ["Syuya"]
license     = "MIT"
homepage    = "https://github.com/syuya2036/nohrs-plugin-example"

[engine]
nohrs_version           = ">=0.5"
component_model_version = "0.1"   # WIT world version

[activation]
mode   = "lazy"   # "lazy" | "eager"
events = ["onCommand:example.foo", "onFileType:rs"]

[permissions]
read_paths  = ["$HOME/Documents/**"]
write_paths = []
network     = []
process     = []
clipboard   = "none"        # "none" | "read" | "write" | "read-write"
notification = true
host_apis   = ["kv", "cache", "launcher.contribute"]

[[commands]]
id       = "example.foo"
title    = "Foo"
subtitle = "Does foo"
category = "developer-tools"

[contributes]
context_menu = []
decorations  = []
preview      = []
```

詳細スキーマは [`docs/plugin-permissions.md`](./plugin-permissions.md) と [`docs/plugin-api.md`](./plugin-api.md) 参照。

---

## 5. バージョニング

| 観点 | 仕様 |
|------|------|
| plugin version | SemVer 必須 |
| nohrs version 範囲 | `engine.nohrs_version` で minimal 指定 |
| WIT world version | plugin は特定 WIT world (`nohrs:plugin@0.1.0` 等) に依存、host 側で複数 world バージョンをサポート (例: `0.1.x` と `0.2.x` 両対応) |
| breaking change | WIT world は major bump で互換性切断、各 plugin が複数 world バージョンを export 可能 |
| update 通知 | community plugin は週次で GitHub API で latest release / tag チェック、新版あれば設定ページに通知バッジ |

---

## 6. plugin に提供される API (要点)

詳細は [`docs/plugin-api.md`](./plugin-api.md)。

| 種類 | 例 |
|------|------|
| **host imports** (plugin → host) | `logging`, `kv`, `cache`, `metadata`, `fs`, `network`, `process`, `clipboard`, `notification`, `launcher`, `explorer`, `search` (`read_paths` 範囲内のファイル/全文検索) |
| **plugin exports** (host → plugin) | `commands`, `decorations`, `previews`, `events` |
| **UI モデル** | "データを返させて、描画はホストが行う" 原則。構造化リスト + markdown のハイブリッド |

---

## 7. permission モデル (要点)

詳細は [`docs/plugin-permissions.md`](./plugin-permissions.md)。

- インストール時 1 回プロンプト (実行時都度プロンプトはしない)
- ユーザーは個別 permission を customize 可
- 不許可の permission を呼ぶと plugin に `NotPermitted` を返す (trap させない)
- 2 層サンドボックス: WASI capability + host function ガード
- 危険操作の hard ban (`~/.ssh/` などのシステムパス、shell expansion 等)

---

## 8. 配布・インストール (要点)

詳細は [`docs/plugin-distribution.md`](./plugin-distribution.md)。

| ソース | 例 |
|--------|-----|
| GitHub `user/repo` | `syuya2036/nohrs-plugin-example` |
| 任意 URL (git) | `https://gitlab.com/.../.git` |
| 任意 URL (zip/tar.gz) | `https://example.com/plugin.zip` |
| ローカルパス | `file:///Users/.../my-plugin` (dev mode) |

- 整合性: `plugin.toml` の `[verify] sha256` + Plugin Store cache の二重照合
- update: 週次バックグラウンド check、明示ユーザー操作で適用 (default 自動 update オフ)
- permission diff: 増えた permission のみ再プロンプト

---

## 9. 言語別テンプレ (要点)

詳細は [`docs/plugin-templates.md`](./plugin-templates.md)。

| 言語 | バインディング | P4 提供 |
|------|--------------|---------|
| Rust | wit-bindgen | ✅ |
| TypeScript | jco (componentize-js) | ✅ |
| Python | componentize-py | ✅ |
| Go | tinygo (P5+) | — |

AI agent 開発支援 (`.factory/skills/`, MCP server `@nohrs/mcp-plugin-dev`) は各テンプレに同梱。

---

## 10. 開発体験目標

- 「new → build → install → 動作確認」が **30 分以内**
- AI agent (Claude Code 等) で WIT を参照しながら plugin 開発可能 (MCP 提供)
- breaking change は WIT world のメジャー bump タイミングに集約
