# Architecture Decision Records (ADR)

このディレクトリは nohrs の **設計判断の事後記録 (ADR)** を蓄積します。RFC (事前提案) は GitHub Discussions で議論し、固まった判断のみ ADR としてここに残します。

## 採用フォーマット

各 ADR は短文 (おおむね 100-200 行)、テンプレートは:

```markdown
# NNNN — Title

> Status: Proposed | Accepted | Superseded by ADR XXXX
> Date: YYYY-MM-DD

## Context

## Decision

## Consequences

### Positive

### Negative

## Alternatives Considered
```

## 一覧

| 番号 | タイトル | Status |
|------|---------|--------|
| [0001](./0001-sqlite-tantivy-hybrid-search.md) | SQLite + Tantivy ハイブリッド検索採用 | Accepted |
| [0002](./0002-macos-only-short-term.md) | macOS 専用を当面維持 | Accepted |
| [0003](./0003-cargo-workspace-layer-split.md) | レイヤー別 Cargo workspace 分割 | Accepted |
| [0004](./0004-remove-tokio.md) | tokio をアプリコアから撤去し WASI プラグイン層に隔離 | Accepted |
| [0005](./0005-wit-bindgen-component-model.md) | プラグインホストは wit-bindgen + WASM Component Model 一直線 | Accepted |
| [0006](./0006-monorepo-web.md) | web/ を nohrs リポジトリ同居 (monorepo) | Accepted |
| [0007](./0007-cloudflare-hosting.md) | web ホスティングは Cloudflare Pages + Workers + R2 | Accepted |
| [0008](./0008-web-design-system.md) | web のデザイン北極星は zed.dev、FE は Tailwind v4 + Radix 再スキン | Accepted |

## 命名規約

- ファイル名: `NNNN-kebab-case.md` (4 桁連番)
- 番号は連続採番、欠番は作らない (`Superseded` は status で表現)
- タイトルは命題形 (Decision を 1 行で要約)
