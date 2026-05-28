# Plugin Templates — 言語別テンプレと AI Agent 開発支援

> Status: Draft (P4 で公開、P5 で Go 追加検討)
> Related: [`docs/plugin-overview.md`](./plugin-overview.md), [`docs/plugin-api.md`](./plugin-api.md), [`docs/plugin-distribution.md`](./plugin-distribution.md)

本書は plugin 開発を加速するための言語別テンプレートと AI agent 開発支援 (skills / MCP) を定めます。

---

## 1. 対応言語

| 言語 | バインディング | P4 提供 | 備考 |
|------|--------------|---------|------|
| **Rust** | `wit-bindgen` (公式) | ✅ tier 1 | host と同言語、wit-bindgen reference 実装 |
| **TypeScript** | `jco` (componentize-js) | ✅ tier 1 | Raycast 開発者の最大潜在層 |
| **Python** | `componentize-py` | ✅ tier 2 | データ系・スクリプト系の作者 |
| **Go** | tinygo + wit-bindgen-go | ❌ → P5 | tinygo の component model 対応がまだ発展途上 |
| **C / C++** | `wit-bindgen-c` | ❌ → P9+ | ニッチ |

---

## 2. テンプレートの配布

各テンプレは **別リポジトリ** として `noh-rs` org 配下:

```text
noh-rs/
├── nohrs/                            # 本体
├── plugin-template-rust/
├── plugin-template-typescript/
├── plugin-template-python/
└── (P5+) plugin-template-go/
```

---

## 3. テンプレ repo の中身

```text
plugin-template-rust/
├── plugin.toml                       # manifest テンプレ (TODO コメント付き)
├── README.md                         # quick start
├── LICENSE
├── Cargo.toml                        # wit-bindgen, wasm32-wasip2 target
├── src/
│   └── lib.rs                        # commands trait の最小実装
├── wit/
│   └── world.wit                     # nohrs:plugin@0.1.0 import
├── .gitignore
├── justfile                          # `just build` で component.wasm
├── .github/
│   └── workflows/
│       └── build.yml                 # WASM build + lint
├── .factory/                         # AI agent 開発支援
│   ├── skills/
│   │   ├── plugin-dev/SKILL.md
│   │   ├── wit-types/SKILL.md
│   │   └── nohrs-api/SKILL.md
│   └── prompts/
│       └── plugin-init.md
├── .claude/
│   └── commands/
│       └── nohrs-plugin.md           # /nohrs-plugin スラッシュコマンド
└── CLAUDE.md / AGENTS.md             # AI agent 向けプロジェクトガイド
```

TypeScript / Python テンプレも同様の構造 (justfile or package.json or pyproject.toml の差分はあり)。

---

## 4. CLI スキャフォルダ (`nohrs plugin`)

```bash
# 新規 plugin 作成 (template repo を内部実装で clone、placeholder 置換)
nohrs plugin new --lang rust --name my-plugin --id user/my-plugin

# ビルド (言語別に justfile / npm script / poethepoet を呼ぶラッパー)
nohrs plugin build

# ローカル install (dev mode、symlink for `~/dev/plugins/`)
nohrs plugin install ./my-plugin

# permission チェック (manifest の妥当性検証)
nohrs plugin check

# Plugin Store への submit 用 PR 作成 (P5、gh CLI 経由)
nohrs plugin publish
```

| 観点 | 仕様 |
|------|------|
| 実装 | `nohrs` メインバイナリのサブコマンド (clap の subcommand)、別バイナリにしない |
| template fetch | `gh repo clone noh-rs/plugin-template-{lang}` 相当を内部実装 |
| placeholder | `{{plugin_name}}`, `{{plugin_id}}`, `{{author}}` を template 内に埋め込んで置換 |
| 言語自動検出 | `--lang` 省略時は対話プロンプト |

---

## 5. サンプルコード (各テンプレ初期同梱)

P4 で各 template に **30 分以内で動かせる** サンプルを同梱:

| サンプル | Rust | TS | Python |
|---------|------|-----|--------|
| Command: "Hello, nohrs!" 通知 | ✅ | ✅ | ✅ |
| Decoration: `.rs` ファイルに `R` バッジ | ✅ | ✅ | ✅ |

完成フロー目標:
1. `nohrs plugin new --lang rust --name hello`
2. `cd hello && just build`
3. `nohrs plugin install .`
4. nohrs で "Hello" コマンド実行 → 通知表示

---

## 6. AI Agent 開発支援

### 6.1 各テンプレ内に同梱するもの

```text
.factory/skills/
├── plugin-dev/SKILL.md          # plugin 開発の基本フロー (build, install, debug)
├── wit-types/SKILL.md           # WIT 型と各言語表現のマッピング表
├── nohrs-api/SKILL.md           # nohrs:plugin@0.1.0 の全 import/export の用例集
└── debug/SKILL.md               # plugin が動かない時の切り分けフロー (permission, version, ...)

.claude/commands/
└── nohrs-plugin.md              # /nohrs-plugin スラッシュコマンド

CLAUDE.md                        # Claude Code 向け project guide
AGENTS.md                        # 汎用 AI agent guide
```

### 6.2 MCP server `@nohrs/mcp-plugin-dev`

npm パッケージとして配布 (Node ベース MCP server):

| ツール | 機能 |
|--------|------|
| `nohrs_wit_lookup(name)` | 任意の WIT 型/関数のシグネチャと使用例 |
| `nohrs_doc_search(query)` | nohrs ドキュメント (`docs/plugin-*`) の semantic search |
| `nohrs_plugin_validate(path)` | `plugin.toml` + `component.wasm` の validation |
| `nohrs_example_plugins(category)` | 既存 plugin の参考実装一覧 |

設定例 (Claude Code):

```json
{
  "mcpServers": {
    "nohrs-plugin-dev": {
      "command": "npx",
      "args": ["-y", "@nohrs/mcp-plugin-dev"]
    }
  }
}
```

### 6.3 CLAUDE.md / AGENTS.md の内容

各テンプレに含める project guide の項目:

- WIT 経由 imports の使い方
- permission の意味と書き方
- build / install / test のコマンド一覧
- nohrs 本家との互換 version 確認方法
- よくある落とし穴:
  - `tokio` を持ち込もうとしてビルド失敗
  - 絶対パス指定で permission に当たる
  - WIT 型の変換ミス
  - manifest の `engine.nohrs_version` 範囲ミス

---

## 7. テンプレ更新フロー

| 観点 | 仕様 |
|------|------|
| nohrs version 追従 | 各 template repo の CI で nohrs 最新版で `nohrs plugin check` を週次 run、breaking change 検出時に issue 自動起票 |
| AI skill / MCP の更新 | nohrs 本体の WIT が変わったら release 時に各テンプレの `.factory/skills/wit-types/SKILL.md` を自動更新 (`script/sync-templates.sh`) |
| サンプル plugin の依存 | nohrs WIT 最新 minor バージョンに追従、breaking なら手動更新 |
| テンプレ自身の version | 各テンプレも SemVer 管理、tag に対応 nohrs version を embed (`v0.5.0-nohrs-0.5`) |

---

## 8. 各言語のビルド手順 (テンプレ README に記載)

### 8.1 Rust

```bash
# 前提: wasm32-wasip2 target
rustup target add wasm32-wasip2
cargo install --locked cargo-component

# Build
just build
# → target/wasm32-wasip2/release/component.wasm
```

### 8.2 TypeScript

```bash
# 前提: Node.js 20+, jco
npm install -g @bytecodealliance/jco

# Build
npm install
npm run build
# → dist/component.wasm
```

### 8.3 Python

```bash
# 前提: Python 3.12+, componentize-py
pip install componentize-py

# Build
poethepoet build
# → dist/component.wasm
```

---

## 9. デバッグ

| 課題 | 解決 |
|------|------|
| plugin が load されない | `nohrs --log-level=debug` で `plugin host` 関連ログを確認 |
| permission denied | nohrs UI の設定 → Plugins → 該当 plugin → permission 一覧で要確認 |
| WIT 型エラー | `nohrs plugin check` で staticly catch |
| trap で crash | `auto_disabled_until` に記録、次回起動時に "Plugin X was disabled due to crash" 通知 |

---

## 10. 公式ドキュメント (web)

`/docs/plugin-authoring/`:

- `/<lang>/getting-started` (Rust / TS / Python それぞれ)
- `/api/<interface>` (WIT API リファレンス自動生成)
- `/permissions` (permission モデル)
- `/distribution` (Plugin Store 登録手順)
- `/ai-agent` (MCP 利用例、推奨プロンプト)
