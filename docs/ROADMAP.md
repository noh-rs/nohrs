# Nohrs Roadmap

> Last updated: 2026-05-28

Nohrs は、macOS の Finder を起点に「Launcher × Explorer」を高速・拡張可能・プラグイン可能な形で再構築する OSS プロジェクトです。本書は v0.2.0 から v0.7.0 までの開発計画を示し、各フェーズの目標と参照すべき設計ドキュメントを整理します。

このロードマップは旧 `QUALITY_IMPROVEMENT_PLAN.md` を統合・廃止し、新たな単一の指針として位置付けられます。

---

## ビジョン

> **Launcher × Explorer** — Finder の代替に留まらない、Raycast 風ランチャーと高速エクスプローラを 1 つのアプリで結合した、プラグイン拡張可能なファイル操作環境。

差別化:

- **Launcher first-class**: グローバルホットキーから単独起動できるランチャーを内蔵
- **Explorer first-class**: スプリットビュー / タブ / DnD / バルク操作を備えた現代的なファイラー
- **WASM Component Model プラグイン**: Rust / TypeScript / Python で書ける、サンドボックス + 明示同意の権限モデル
- **Spotlight に依存しない検索**: SQLite + Tantivy のハイブリッドで、依存ゼロ + コードベース対応

---

## 全体方針

- **シリアルな 6 フェーズ** (P1〜P6) で v0.2.0〜v0.7.0 まで進める
- 各フェーズ内では **Core / Web / Quality** の 3 セクションを並行進行可能
- web (`nohrs.app` + `noh.rs`) は **P1 から MVP** を立ち上げる
- すべての breaking change は major bump (`0.x` の `x`) のタイミングに集約
- 各フェーズの完了基準には常に **`cargo fmt --check && cargo clippy -D warnings && cargo test`** が CI で通ることを含む
- `unsafe` ブロックは P1 完了時点でゼロにし、以後ワークスペース全体で `#![deny(unsafe_code)]` 維持

---

## フェーズ早見表

| Phase | Milestone | テーマ | 主な成果物 |
|-------|-----------|--------|-----------|
| **P1** | `v0.2.0` | Foundation | 既存品質改善・workspace 化・開発環境・検証基盤・web MVP・config 最小実装 |
| **P2** | `v0.3.0` | Explorer Essentials | DnD・ファイル操作・スプリットビュー・タブ・SQLite/MetadataStore・tokio 撤去 |
| **P3** | `v0.4.0` | Launcher & Search | Raycast 風ランチャー・SQLite FTS5 (V2) 全文検索 |
| **P4** | `v0.5.0` | Plugin Host | WIT API・WASM Component Model ホスト・3 言語テンプレ・AI agent 開発支援 |
| **P5** | `v0.6.0` | Ecosystem | Plugin Store ページ・コミュニティプラグイン |
| **P6** | `v0.7.0` | Stabilization | 多 OS 戦略決定・パフォーマンス・ドキュメント完成 |
| Future | `v1.0.0` | TBD | UI i18n・Linux/Windows 完全対応・AI agent 統合・クラウド機能 等 |

---

## 参照ドキュメント (15)

ROADMAP 本体には判断の要点のみを記し、詳細は次の設計ドキュメントを参照します。各ドキュメントは骨子を P1 で作成し、対応フェーズで詳細化します。

| ドキュメント | 対応 Phase | 内容 |
|------------|-----------|------|
| [`docs/architecture.md`](./architecture.md) | P1 | Cargo workspace 構成・crate 間依存方向・データフロー |
| [`docs/web.md`](./web.md) | P1 | サイト IA・ルーティング・i18n・ホスティング (CF Pages + Workers) |
| [`docs/testing.md`](./testing.md) | P1 | GPUI `TestAppContext`・llvm-cov→R2/GitHub native パイプライン・静的解析 |
| [`docs/dev-environment.md`](./dev-environment.md) | P1 | docker dev/ci 二系統・nix devshell |
| [`docs/config.md`](./config.md) | P1→P2 | `~/.config/nohrs/config.toml` スキーマ・hot reload タイミング・JSON Schema |
| [`docs/persistence.md`](./persistence.md) | P2 | rusqlite + WAL・redb (plugin KV)・`MetadataStore`/`KvStore` trait・マイグレーション |
| [`docs/async-runtime.md`](./async-runtime.md) | P2 | GPUI executor 統一・`postage`/`async-channel`/`ureq` への置換 |
| [`docs/explorer-essentials.md`](./explorer-essentials.md) | P1骨子→P2 | DnD・ファイル操作・スプリットビュー・タブ |
| [`docs/launcher.md`](./launcher.md) | P3 | フローティング window・グローバルホットキー・アクションフレームワーク |
| [`docs/search.md`](./search.md) | P3→P4 | V1 ripgrep → V2 SQLite FTS5 → V3 Tantivy 統合・リソース制限 |
| [`docs/plugin-overview.md`](./plugin-overview.md) | P4 | wit-bindgen + Component Model・ライフサイクル・コア/コミュニティ分離 |
| [`docs/plugin-api.md`](./plugin-api.md) | P4 | WIT world・host imports/exports・UI レンダリングモデル |
| [`docs/plugin-permissions.md`](./plugin-permissions.md) | P4 | 権限マニフェスト・同意フロー・2 層サンドボックス |
| [`docs/plugin-distribution.md`](./plugin-distribution.md) | P4→P5 | インストール (user/repo, URL, local)・更新・Plugin Store 連携 |
| [`docs/plugin-templates.md`](./plugin-templates.md) | P4 | Rust/TS/Python テンプレ・`nohrs plugin` CLI・AI agent skills/MCP |

加えて [`docs/adr/`](./adr/) に短文の Architecture Decision Records を蓄積します。

---

## Phase 1 — Foundation (v0.2.0)

**ゴール**: OSS として「他人が Issue/PR を出したくなる」最小限の体裁を整え、後続フェーズの土台を固める。

### Core (Rust 本体)

- **緊急修正**: `unsafe` 除去 (`src/ui/components/file_list.rs`)、`apply_filter()` 二重呼び出し修正、`open_preview` の同期 I/O 撤去、`SearchService::new` 失敗時の `panic!` 撤廃、`format_date` 自前実装を `time` crate に置換、`mdfind` 引数注入対策、エラー処理の UI 経路化
- **Cargo workspace 化**: 単一 crate を 6 crate に分割 (`nohrs`, `nohrs-core`, `nohrs-models`, `nohrs-services`, `nohrs-ui`, `nohrs-pages`)。詳細は [`docs/architecture.md`](./architecture.md)
- **エラー設計**: `core::errors::Error` 拡張、サービス層を `crate::core::errors::Result` に統一、`clippy::unwrap_used`/`expect_used` warn 化
- **巨大ファイル分解**: `src/pages/explorer/mod.rs` (688 行) の責務分割、マジックナンバーの `core::config` 集約
- **explorer essentials の骨子確定**: DnD/file ops/split view/tab の方針を [`docs/explorer-essentials.md`](./explorer-essentials.md) に記載 (実装は P2)
- **config 最小実装**: theme / ui セクションのみ。XDG 準拠の `~/.config/nohrs/config.toml`、hot reload は theme/ui だけ対応。詳細は [`docs/config.md`](./config.md)
- **axum 削除** + `#[tokio::main]` → GPUI main 化 (tokio 撤去への第一歩)

### Web

- **`nohrs.app` MVP** (Cloudflare Pages + Workers): `/` ランディング・`/releases` (GitHub API 連携の一覧)・`/blog` 雛形・`/docs` 雛形
- **`noh.rs` リダイレクト Worker**: path 保持 301 + 短縮スキーム (例: `noh.rs/p/<plugin>`)
- **i18n 基盤**: パス前置 (`/en/...` / `/ja/...`)、canonical = `en`
- **Pagefind 検索の組み込み骨格** (docs / blog 用)
- **Cloudflare Web Analytics 組み込み**
- 詳細は [`docs/web.md`](./web.md)

### Quality / Infra

- **CI**: `.github/workflows/ci.yml` (`fmt --check` / `clippy -D warnings` / `cargo test` / `cargo build --features gui`)
- **カバレッジパイプライン**: `cargo llvm-cov` → R2 への HTML アップ (`coverage.nohrs.app/pr/<n>/`) + GitHub Native の lcov 取り込み。詳細は [`docs/testing.md`](./testing.md)
- **GPUI テスト基盤**: `TestAppContext` を用いたテストのお手本コードを `nohrs-pages` 配下に整備
- **静的解析**: `clippy.toml` / `rustfmt.toml` / `deny.toml` / `cargo-machete` / `typos`
- **開発環境**: `docker/dev/` (X11 forwarding)、`docker/ci/` (Xvfb)、`flake.nix` (devshell)。詳細は [`docs/dev-environment.md`](./dev-environment.md)
- **OSS 体裁**: `CONTRIBUTING.md` / `CODE_OF_CONDUCT.md` / `SECURITY.md` / `CHANGELOG.md`、Issue/PR テンプレ、`dependabot.yml`、`Cargo.toml` メタ情報整備
- **README 書き換え**: Planned Features を ROADMAP へ移管。Hero / Demo / Why / Quick Start / Status / Roadmap / Community / License 構成
- **遡及 ADR 起票**: [§遡及 ADR](#遡及-adr-p1-で作成) 参照

### 完了条件

- `unsafe` 0 個
- `cargo fmt --check && cargo clippy -- -D warnings -W clippy::unwrap_used -W clippy::expect_used && cargo test --all-features` が CI で green
- `cargo publish --dry-run` がメタ情報エラーを出さない
- `nohrs.app` が GA、`noh.rs` リダイレクト稼働
- 全 spec doc 15 本の骨子が `docs/` 配下に存在

---

## Phase 2 — Explorer Essentials (v0.3.0)

**ゴール**: エクスプローラの「現代的なファイラー」として最低限欲しい機能を揃える + 永続化基盤と非同期ランタイムの土台を確立。

### Core

- **DnD**: 内部 pane 間・外部アプリ間 (drop in / drop out)・複数選択ドラッグ。Cmd で copy / 通常は move (cross-volume は自動で copy)。詳細は [`docs/explorer-essentials.md`](./explorer-essentials.md)
- **ファイル操作**: copy / cut / paste / rename / delete (trash デフォルト・Shift で permanent) / new folder / undo (window 単位 stack)。conflict は Rename/Overwrite/Skip + "Apply to all"
- **スプリットビュー**: 2-way (水平/垂直)、ペイン独立ナビゲーション、ペイン単位 tab
- **タブ**: 復元 (再起動時)、close、reorder。ピン留めは P3
- **SQLite + MetadataStore trait**: `nohrs-store` crate 新設、rusqlite + bundled + WAL モード。`MetadataStore` / `KvStore` / `HistoryStore` 等の interface segregated trait。詳細は [`docs/persistence.md`](./persistence.md)
- **tokio 完全撤去**: `tokio::sync::*` を `postage` / `async-channel` / `futures::channel::oneshot` に、`tokio::spawn` を `cx.background_spawn` に、`tokio::time::*` を GPUI timer に置換。HTTP は `ureq` 採用。`cargo-deny` で tokio を ban。詳細は [`docs/async-runtime.md`](./async-runtime.md)
- **config 拡張**: keybindings セクション草案 (P3 で本格化)、`schemars` で JSON Schema 自動生成

### Web

- **blog エンジン本格化**: MDX + `<Callout>` / `<Screenshot>` / `<CodeTabs>` カスタムコンポーネント、RSS / Atom feed、giscus (GitHub Discussions) コメント、Satori で OG 画像自動生成
- **docs 拡充**: P2 までに固まった spec doc を web 公開

### Quality

- **`nohrs-store` 単体テスト整備** (in-memory backend で trait テスト)
- **ベンチマーク基盤**: `criterion` で SQLite クエリ・ファイル listing のベース測定
- **`#![deny(unsafe_code)]` を workspace 全体に強制**

### 完了条件

- DnD / file ops / split view / tab が「Finder と同等の操作」を達成
- `cargo tree | grep '^tokio'` が **空**
- SQLite で起動間メタデータが永続化
- `MetadataStore` mock backend でテスト可能

---

## Phase 3 — Launcher & Search (v0.4.0)

**ゴール**: 「Launcher × Explorer」の launcher 側を立ち上げ、検索基盤を SQLite FTS5 (V2) まで進める。

### Core

- **Launcher 本体** (`crates/nohrs-launcher`):
  - 別 window、フローティング、画面中央寄り上、マウスドラッグで移動可能 (位置記憶は SQLite `key_value`)、リサイズ不可
  - グローバルホットキー `Cmd+Shift+Space` (`global-hotkey` crate)、アプリ内 `Cmd+K`
  - 入力前は空欄 + placeholder ヒント
  - 結果リスト: icon / title / subtitle / kind badge / shortcut accessory
  - fuzzy ranking: `nucleo` + recency / frequency / context boost
  - 詳細ペイン (`Tab` で開く / 閉じる)、スタック型ナビゲーション
  - 初期コアコマンド 15-20 個 (Open Path / Reveal in Finder / Quick Open / Recent / Calculator / Settings 等)
  - 詳細は [`docs/launcher.md`](./launcher.md)
- **アクションフレームワーク** (`Command` trait + `inventory` レジストリ): コア crate がそれぞれ自身のコマンドを宣言。P4 で WIT plugin command の adapter 経由で同 trait に統合
- **検索 V2 (SQLite FTS5)**: `nohrs-services` 内の `search` モジュールを再構築。trigram tokenization、増分更新、リソース throttling (バッテリー / `LowPowerMode` / idle 検出 / 前面状態)、`notify-debouncer-mini` で watcher 復活。詳細は [`docs/search.md`](./search.md)
- **検索 UI**: ランチャー (グローバルスコープ) + エクスプローラ内検索バー (`Cmd+F`、現在ディレクトリ scope)

### Web

- **コマンド一覧ページ** (`/docs/commands`): ランチャーで使える core コマンド一覧の自動生成 (build 時に inventory を読み取り)
- **検索デモ動画**: blog 記事として release

### Quality

- launcher / search の integration test 整備
- リソース throttling ロジックのテスト (mock 電源状態)

### 完了条件

- グローバルホットキーから launcher が <100ms で起動
- 検索が `$HOME` に対し `cat` のような短いクエリで <500ms 応答
- バッテリー駆動時に indexer の CPU 使用が 1 thread 以下

---

## Phase 4 — Plugin Host (v0.5.0)

**ゴール**: WASM Component Model ベースのプラグインホストと、3 言語のテンプレを揃える。

### Core

- **`nohrs-plugin-host` crate**: wasmtime 30+ + sync API + Component Model + `wit-bindgen`。詳細は [`docs/plugin-overview.md`](./plugin-overview.md)
- **WIT world `nohrs:plugin@0.1.0`**: imports (`logging` / `kv` / `cache` / `metadata` / `fs` / `network` / `process` / `clipboard` / `notification` / `launcher` / `explorer`)、exports (`commands` / `decorations` / `previews` / `events`)。詳細は [`docs/plugin-api.md`](./plugin-api.md)
- **権限モデル**: install 時 1 回プロンプト、Customize で個別 toggle、危険操作の hard ban、update 時の permission diff 再プロンプト。詳細は [`docs/plugin-permissions.md`](./plugin-permissions.md)
- **redb-backed plugin KV**: ホストの SQLite と分離、plugin_id ごとに table 隔離、1 値 1MB 上限、batch 操作。詳細は [`docs/persistence.md`](./persistence.md)
- **ライフサイクル**: lazy activation がデフォルト、activation events (`onCommand:` / `onFileType:` 等)、60 秒 idle で auto suspend
- **コアプラグイン**: `crates/plugins/nohrs-plugin-*` に初期 1-2 個 (例: git status badge、calculator) を Rust ネイティブで実装。WIT を経由しないが、launcher の `Command` trait や explorer の `Decorator` trait を実装する形で API を統一
- **`nohrs plugin` サブコマンド**: `new` / `build` / `install` / `check`。詳細は [`docs/plugin-templates.md`](./plugin-templates.md)
- **検索 V3 (Tantivy 統合)**: SQLite FTS5 を Tantivy + identifier 分解 + ngrams で BM25 ランキング。code-aware tokenization。詳細は [`docs/search.md`](./search.md)

### Plugin Templates (別リポジトリ)

- **`noh-rs/plugin-template-rust`** (wit-bindgen + cargo + wasm32-wasip2)
- **`noh-rs/plugin-template-typescript`** (jco / componentize-js)
- **`noh-rs/plugin-template-python`** (componentize-py)
- 各テンプレに最小 sample (Hello command + Decoration)、`.factory/skills/` + `CLAUDE.md` / `AGENTS.md` + `.claude/commands/` を同梱
- **MCP server**: `@nohrs/mcp-plugin-dev` を npm で配布 (`nohrs_wit_lookup` / `nohrs_doc_search` / `nohrs_plugin_validate` / `nohrs_example_plugins`)

### Web

- **`/docs/plugin-authoring/`**: 各言語の getting started、WIT API リファレンス自動生成、permission 解説、AI agent 開発支援

### Quality

- WIT world に対する snapshot test (`insta`)
- permission ガードのプロパティテスト (`proptest`)
- wasmtime trap → host エラーパスの統合テスト

### 完了条件

- 3 言語テンプレで「new → build → install → 動作確認」が **30 分以内** で完了
- 初期コア plugin 2 個が動作
- AI agent (Claude Code 等) で WIT を参照しながら plugin 開発できる (MCP 提供)

---

## Phase 5 — Ecosystem (v0.6.0)

**ゴール**: コミュニティプラグインを受け入れる仕組みを web 側に整備し、エコシステムを立ち上げる。

### Core

- **プラグインの自動更新通知**: 週次バックグラウンドで GitHub API から latest release / tag fetch、更新あれば設定ページにバッジ
- **`nohrs plugin publish`**: Plugin Store への submit 用 PR を自動作成 (`gh CLI` 経由)
- **plugin permission revoke UI**: 設定ページで個別 toggle、reload で反映
- **plugin ロールバック UI**: 1 つ前のバージョンに戻す

### Web

- **Plugin Store ページ** (`/plugins`):
  - PR ベース登録 (`web/content/plugins/<id>.toml` を編集)
  - ビルド時に GitHub API で stars / last update / license / README を enrich
  - 5 カテゴリ (productivity / developer-tools / media / cloud / theme)
  - 各カードに permission バッジ (`fs:home` / `net` 等)
  - install ボタン: `nohrs://install?source=user/repo` でアプリディープリンク (アプリ未起動時は config 断片をクリップボードコピー)
  - 詳細は [`docs/plugin-distribution.md`](./plugin-distribution.md)
- **release ページのリッチ化**: 主要 release は web 側でハイライトを追加 (frontmatter)

### Quality

- Plugin Store 登録 PR への自動 CI チェック (`plugin.toml` schema 検証、SHA-256 整合性、`engine.nohrs_version` 範囲)

### 完了条件

- Plugin Store ページに **5+ コミュニティプラグイン** が掲載
- ユーザーが Plugin Store からワンクリックで install 完了

---

## Phase 6 — Stabilization (v0.7.0)

**ゴール**: マルチ OS 対応戦略の決定、performance ゲート、ドキュメント完成。`v1.0.0` への助走。

### Core

- **多 OS 戦略決定**: Linux 完全対応 / Windows 対応 / macOS 専用継続のいずれを取るかを ADR で記録 (この時点で gpui の OS サポート状況が判断材料)
- **performance ゲート**: launcher 起動時間 <100ms、search latency 中央値 <500ms、indexing バックグラウンド時 CPU <10% を CI で測定
- **`#![warn(missing_docs)]` → `deny`**: pub API 全てに rustdoc
- **`nohrs-gpui-wasmtime` 再評価**: sync wasmtime のボトルネック実測 → 必要なら `noh-rs/gpui-wasmtime` リポジトリを別 lifecycle で立ち上げ。詳細は [§Future Work](#future-work)

### Web

- **docs 完成度向上**: API 全ページ、tutorial 系記事 5 本以上、screenshot/動画整備

### Quality / Release

- **`v0.7.0` リリース**: `.github/workflows/release.yml` で macOS バイナリ + `dmg` を gh release に自動掲載
- **`cargo-release` 設定**: `release.toml`、`CHANGELOG.md` 自動更新

### 完了条件

- GitHub Releases に `v0.7.0` のバイナリが掲載
- `cargo doc --no-deps` が warning 0
- performance ゲートが CI で安定 pass

---

## Future Work

> v1.0.0 以降 / 未確定。これらはコミットしないアイデアの一時置き場です。状況の変化で削除・昇格・統合の可能性があります。

- **アプリ UI の i18n** (P6 で再検討、`fluent-rs` ベース想定)
- **Linux / Windows 完全サポート** (P6 の戦略決定次第)
- **`noh-rs/gpui-wasmtime` リポジトリ**: GPUI executor 上で wasmtime async runtime をホストするブリッジ crate。実測ボトルネックが出た段階で別 lifecycle で開発、GPUI コミュニティへの貢献を兼ねる
- **AI agent 統合** (NL 自然言語 → ファイル操作 / search / automation)
- **クラウド統合** (S3 互換、ハイブリッドオフライン、共有)
- **CLI / HTTP API** (外部制御、リモート browsing via SSH)
- **PTY 統合** (built-in terminal)
- **Git 統合の本格化** (sidebar、blame、conflict UI)
- **plugin 間の依存解決** (現時点は self-contained のみ)
- **Plugin Store の動的データ** (DL 数、評価) → CF Workers + KV / D1 backend
- **menubar 常駐モード** (macOS / Linux tray)
- **plugin の async 通信モデル** (long-running task のキャンセル対応)
- **Office / PDF / OCR の content extraction**
- **noh.rs の独立ランディング化** (CLI install one-liner 等、リダイレクト以上の役割を持たせる場合)

---

## 遡及 ADR (P1 で作成)

過去に既に確定した設計判断を ADR として記録します。すべて `docs/adr/NNNN-kebab-case.md` 命名:

1. `0001-sqlite-tantivy-hybrid-search.md` — SQLite + Tantivy ハイブリッド検索を採用 (旧プランの Spotlight 一本化案を棄却)
2. `0002-macos-only-short-term.md` — macOS 専用を当面維持し、Linux/Windows 対応は P6 で判断
3. `0003-cargo-workspace-layer-split.md` — レイヤー別の Cargo workspace 分割
4. `0004-remove-tokio.md` — tokio を撤去し GPUI executor / postage / async-channel / ureq に統一
5. `0005-wit-bindgen-component-model.md` — プラグインホストは Extism を経由せず wit-bindgen + WASM Component Model 一直線
6. `0006-monorepo-web.md` — `web/` を nohrs リポジトリ同居 (monorepo)、Cargo workspace から exclude
7. `0007-cloudflare-hosting.md` — web ホスティングは Cloudflare Pages + Workers、カバレッジは R2

---

## 検証コマンド (各フェーズ共通)

```bash
# フォーマット & lint
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings \
    -W clippy::unwrap_used -W clippy::expect_used

# ビルド
cargo build
cargo build --features gui

# テスト
cargo test --all-features

# カバレッジ (P1 以降)
cargo llvm-cov --all-features --lcov --output-path lcov.info
cargo llvm-cov --all-features --html

# unsafe / 残置 panic 検出
grep -rn 'unsafe' crates/ | grep -v '//' | grep -v 'unsafe_code' || echo "no unsafe blocks"
grep -rn 'panic!\|unimplemented!\|todo!' crates/

# 依存禁則 (P2 以降、tokio が消えていること)
cargo deny check bans

# ドキュメント (P6 以降は warning 0 必須)
cargo doc --no-deps --all-features

# 公開チェック
cargo publish --dry-run --package nohrs
```

---

## ライセンスと貢献

- **License**: MIT
- **Contributing**: [`CONTRIBUTING.md`](../CONTRIBUTING.md)
- **Code of Conduct**: [`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md)
- **Security**: [`SECURITY.md`](../SECURITY.md)
- **Discord**: <https://discord.gg/dZM7fUtE94>
- **X**: <https://x.com/nohrsdotapp>
