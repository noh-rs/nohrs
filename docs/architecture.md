# Architecture

> Status: Draft (P1 で確定する設計)
> Related: [`ROADMAP.md`](./ROADMAP.md), [ADR 0003 (cargo-workspace-layer-split)](./adr/0003-cargo-workspace-layer-split.md)

本書は nohrs の Cargo workspace 構成、crate 間の依存方向、レイヤーごとの責務、主要なデータフローを定めます。

---

## 1. Workspace Layout

```text
nohrs/
├── Cargo.toml                # workspace root (見出しのみ、main package なし)
├── rust-toolchain.toml
├── crates/
│   ├── nohrs/                # main binary (P1)
│   ├── nohrs-core/           # errors / config / telemetry (P1)
│   ├── nohrs-models/         # FileEntry など pure data types (P1)
│   ├── nohrs-services/       # fs / search / syntax (P1)
│   ├── nohrs-ui/             # gpui components / theme / app shell (P1)
│   ├── nohrs-pages/          # explorer / settings / git / s3 (P1)
│   ├── nohrs-store/          # SQLite + redb + MetadataStore trait (P2)
│   ├── nohrs-launcher/       # launcher (P3)
│   ├── nohrs-plugin-host/    # wasmtime + WIT host (P4)
│   └── plugins/              # コアプラグイン群 (P4 以降に追加)
│       ├── nohrs-plugin-git/
│       └── ...
├── docker/                   # dev / ci 用 Dockerfile (P1)
├── flake.nix                 # Nix devshell (P1)
├── docs/                     # 本書を含む設計ドキュメント群
├── web/                      # tanstack start + vite+ (P1、Cargo workspace から exclude)
└── script/                   # 開発用シェルスクリプト
```

### Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
  "crates/nohrs",
  "crates/nohrs-core",
  "crates/nohrs-models",
  "crates/nohrs-services",
  "crates/nohrs-ui",
  "crates/nohrs-pages",
]
# P2 以降:
#   "crates/nohrs-store",
#   "crates/nohrs-launcher",
#   "crates/nohrs-plugin-host",
#   "crates/plugins/*"

exclude = ["web"]

[workspace.package]
version      = "0.0.1"
edition      = "2021"
rust-version = "1.83"
license      = "MIT"
repository   = "https://github.com/noh-rs/nohrs"
homepage     = "https://nohrs.app"
authors      = ["nohrs contributors"]

[workspace.dependencies]
anyhow            = "1"
thiserror         = "1"
tracing           = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env-filter"] }
serde             = { version = "1", features = ["derive"] }
serde_json        = "1"
time              = { version = "0.3", features = ["formatting", "macros"] }
# (P2 以降に追加)
# rusqlite, redb, postage, async-channel, ureq, wasmtime, wasmtime-wasi, tokio (plugin-host のみ), wit-bindgen, ...

[workspace.lints.rust]
unsafe_code  = "deny"
missing_docs = "warn"     # P6 で deny に格上げ

[workspace.lints.clippy]
unwrap_used  = "warn"
expect_used  = "warn"
```

各 crate は `version.workspace = true`、`license.workspace = true`、`dependencies.anyhow.workspace = true` のように継承します。

---

## 2. レイヤーと責務

```text
                 ┌──────────────┐
                 │  nohrs (bin) │   GUI エントリポイント + CLI
                 └──────┬───────┘
                        │
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
  ┌──────────┐   ┌──────────┐    ┌──────────┐
  │  pages   │   │ launcher │    │ plugin-  │  …各 view / feature
  │          │   │   (P3)   │    │ host(P4) │
  └────┬─────┘   └────┬─────┘    └────┬─────┘
       │              │               │
       └──────┬───────┴──────┬────────┘
              ▼              ▼
        ┌──────────┐   ┌──────────┐
        │    ui    │   │ services │   描画 / ビジネスロジック
        └────┬─────┘   └────┬─────┘
             │              ▼
             │        ┌──────────┐
             │        │  store   │   永続化 (P2)
             │        │   (P2)   │
             │        └────┬─────┘
             │             │
             └──────┬──────┘
                    ▼
              ┌──────────┐
              │  models  │   pure data
              └────┬─────┘
                   ▼
              ┌──────────┐
              │   core   │   errors / config / telemetry
              └──────────┘
```

| crate | 主な責務 | 依存 |
|-------|---------|------|
| **nohrs (bin)** | GUI エントリ、CLI サブコマンド、起動シーケンス | 全 crate |
| **nohrs-pages** | explorer / settings / git / s3 などのページ | `ui`, `services`, `models`, `core`, (P2) `store` |
| **nohrs-launcher** (P3) | ランチャー window、Command trait、結果リスト | `ui`, `services`, `core`, `store`, (P4) `plugin-host` |
| **nohrs-plugin-host** (P4) | wasmtime + wasmtime-wasi + WIT host、permission ガード、専用 tokio runtime (隔離) | `core`, `store`, `services`, `models` |
| **nohrs-ui** | gpui コンポーネント、テーマ、ウィンドウ管理 | `core`, `models` |
| **nohrs-services** | fs listing、search、syntax highlighting | `core`, `models`, (P2) `store` |
| **nohrs-store** (P2) | SQLite (rusqlite) + redb (plugin KV) | `core`, `models` |
| **nohrs-models** | FileEntry など pure data types | `core` |
| **nohrs-core** | errors / config / telemetry / resource policy | — |

### 依存ルール

- **下方向のみ参照可** (上記図の上から下へ)。逆方向参照は禁止
- `models` は他の crate に依存しない (pure data)
- `core` も依存はほぼなし (errors と config のみ)
- 横方向の参照 (例: `pages` から `launcher`) は **避ける**。必要なら `services` か `store` で trait 定義して両者から依存

---

## 3. 主要データフロー

### 3.1 explorer のファイル listing

```text
User clicks folder
  ↓
ExplorerPage::on_navigate (pages)
  ↓ cx.background_spawn
fs::list_dir(path) (services::fs)
  ↓ tokio-free std::fs + rayon
Vec<FileEntry> (models)
  ↓ channel (async-channel)
ExplorerPage::on_listing_complete (pages)
  ↓ cx.update
ExplorerView render (ui)
```

### 3.2 検索 (P3 V2 以降)

```text
User types in launcher
  ↓
Launcher::on_query_change (launcher)
  ↓ debounced 50ms
SearchService::search(query) (services::search)
  ↓
SQLite FTS5 query (store) → Vec<SearchHit>
  ↓
nucleo で fuzzy 再ランキング + boost
  ↓
Vec<LauncherItem> (launcher)
  ↓ cx.notify
LauncherView render (ui)
```

### 3.3 plugin command 実行 (P4)

```text
User selects plugin command in launcher
  ↓
Launcher::dispatch(command_id, args) (launcher)
  ↓
PluginHost::run_command(plugin_id, command_id, args, ctx) (plugin-host, sync な公開 API)
  ↓ permission check
PermissionGuard::check_capability(...)?
  ↓ 専用 tokio runtime で block_on
wasmtime::component::Instance::call_async(...) (wasmtime-wasi)
  ↓ returns CommandResult variant
Launcher render view-node or instant result
```

---

## 4. クレート命名規約

- **本体 crate**: `nohrs-<role>` (例: `nohrs-core`, `nohrs-services`)
- **コアプラグイン**: `nohrs-plugin-<name>` (例: `nohrs-plugin-git`、`crates/plugins/` 配下)
- **公開ライブラリ (Future Work)**: 別リポジトリで独立、prefix は文脈に応じて (例: `noh-rs/gpui-wasmtime`)

`nohrs-` prefix を統一する理由: crates.io 公開時の名前衝突回避と、依存ツリーで一目で nohrs 由来と分かるため。

---

## 5. Lints と禁則

| Lint | レベル | 例外 |
|------|--------|------|
| `unsafe_code` | `deny` (workspace 全体) | 一切認めない (P1 で `file_list.rs` の unsafe を除去) |
| `clippy::unwrap_used` | `warn` | テストコードのみ許容 (`#[allow]`) |
| `clippy::expect_used` | `warn` | 同上 |
| `missing_docs` | `warn` (P1 から)、`deny` (P7 から) | `cfg(test)` モジュールは除外 |
| `disallowed-methods` | clippy.toml で `std::fs::read` 等を禁止し `services::fs` 経由を強制 | 必要なら crate ローカルで `#[allow]` |
| `bans` (cargo-deny) | `tokio` を deny (P2 以降)。`nohrs-plugin-host` 経由のみ `wrappers` で許可 | plugin 実行層 (P4) |

---

## 6. ファイル命名規約 (CLAUDE.md より)

- `mod.rs` は **使わない**。`src/foo.rs` + `src/foo/...` のフラット構成
- 新規 crate は `[lib] path = "src/<crate-name>.rs"` で root を明示
- `src/lib.rs` のような generic 名は避け、`src/nohrs_core.rs` のように記述

---

## 7. 移行計画 (P1)

現状の単一 crate `nohrs` (約 5000 行) を 6 crate に分割するロードマップ:

1. `crates/nohrs-core/` を切り出し (`src/core/` → `crates/nohrs-core/src/nohrs_core.rs`)
2. `crates/nohrs-models/` を切り出し
3. `crates/nohrs-services/` を切り出し
4. `crates/nohrs-ui/` を切り出し
5. `crates/nohrs-pages/` を切り出し
6. `crates/nohrs/` (binary) を整理、`src/gui/main.rs` を `crates/nohrs/src/main.rs` へ
7. workspace inheritance に移行
8. `cargo fmt && cargo clippy && cargo test` が green

各ステップで PR を分けることが望ましい。
