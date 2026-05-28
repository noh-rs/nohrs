# 0006 — web/ を nohrs リポジトリ同居 (monorepo)

> Status: Accepted
> Date: 2026-05-28

## Context

nohrs は P1 で web (`nohrs.app` + `noh.rs`) の MVP を立ち上げる。web の tech stack は TanStack Start + Vite+ (TypeScript) で、Rust 本体 (nohrs) とは別の言語/build pipeline を持つ。

選択肢:

- **A. monorepo (`nohrs/web/`)**: web を nohrs リポジトリ同居
- **B. polyrepo (`noh-rs/web` 等)**: web を別リポジトリで管理
- **C. nohrs.app と noh.rs を別リポジトリ**: 3 リポジトリ体制

考慮事項:

- **リリース同期**: P1〜P6 で release notes / docs / 本体機能の同時更新が頻発
- **コントリビュータ数**: 初期は少ない、リポジトリ分散は認知負荷を増やす
- **Cargo workspace の影響**: `web/` を members に含めなくても、`exclude = ["web"]` で完全分離可能 (Cargo は技術的に問題なし)
- **dependabot ノイズ**: TypeScript 依存の dependabot PR が大量に来ると Rust 側に混乱

## Decision

**Monorepo を採用する。`web/` を nohrs リポジトリ直下に配置し、Cargo workspace から `exclude` する**。

```text
nohrs/
├── Cargo.toml         # workspace members = [...]; exclude = ["web"]
├── crates/...
├── docs/...
└── web/               # TanStack Start app (workspace 外)
    ├── package.json
    └── ...
```

- nohrs.app と noh.rs は **1 つの `web/` プロジェクト**で扱う (noh.rs のリダイレクトは `web/workers/` 配下の Worker 1 ファイル)
- web 用 CI / dependabot は `paths: web/**` filter で分離

## Consequences

### Positive

- 1 PR で本体 + release notes + docs + web を同時更新可能
- リポジトリが 1 つなので新規 contributor の認知負荷が低い
- `docs/` から `web/content/` への参照、`web/` から `docs/` への参照が relative path で書ける
- Cargo workspace の影響なし

### Negative

- リポジトリの language 構成が混在 (Rust + TS)
- TS の依存更新 (dependabot) が Rust 側の人に noise として見える → `paths` filter で軽減
- web 専用 CI を `paths: web/**` filter で分けないと、Rust 変更でも web ビルドが走る

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| polyrepo (`noh-rs/web`) | リリース同期コスト、認知負荷 (どっちに PR?)。zed は分けているが zed.dev は独自の歴史で例外的 |
| `nohrs.app` と `noh.rs` を別リポジトリで 3 リポ体制 | noh.rs はリダイレクトのみで Worker 1 ファイル、別リポジトリ化は過剰 |
| nohrs リポジトリに web を入れず GitHub Pages の静的 site のみ | TanStack Start の SSR / i18n / Plugin Store の動的データに対応できない |
