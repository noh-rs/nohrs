# 0003 — レイヤー別 Cargo workspace 分割

> Status: Accepted
> Date: 2026-05-28

## Context

v0.1.0 時点の nohrs は **単一 crate** (`nohrs`) で約 5000 行、`src/` 配下に `core / models / services / ui / pages / gui` をフラットに配置している。

このまま開発を進めると以下の問題が深刻化する:

- **コンパイル時間の単調増加**: 全部が 1 crate なので、UI 変更でも services 全体が再コンパイル
- **依存方向の崩壊**: モジュール間の循環依存が抑制されない
- **plugin (P4) の API surface 定義が困難**: plugin に公開する型を独立 crate にしないと、`nohrs-plugin-host` が core 全体に依存することになる
- **テスト時間の単調増加**: 1 crate のテストは段階実行できない
- **lint / static analysis の粒度**: crate ごとに `#![deny(unsafe_code)]` 等を強制したい

## Decision

**Cargo workspace 化し、レイヤー別に分割する**。

P1 開始時の 6 crate:

```text
crates/nohrs/            # main binary
crates/nohrs-core/       # errors, config, telemetry
crates/nohrs-models/     # FileEntry など pure data types
crates/nohrs-services/   # fs, search, syntax
crates/nohrs-ui/         # components, theme, app shell
crates/nohrs-pages/      # explorer, settings, git, s3
```

P2 以降に追加:
- `crates/nohrs-store/` (P2)
- `crates/nohrs-launcher/` (P3)
- `crates/nohrs-plugin-host/` (P4)
- `crates/plugins/nohrs-plugin-*` (P4 以降のコアプラグイン)

詳細は [`docs/architecture.md`](../architecture.md) 参照。

依存方向: `nohrs` (binary) → `pages / launcher / plugin-host` → `ui / services` → `store` → `models` → `core`。逆方向参照と横方向参照は禁止。

## Consequences

### Positive

- 部分再コンパイルが効く (UI 変更で services が再コンパイルされない)
- crate ごとに `#![deny(unsafe_code)]` を確実に強制可能
- plugin に公開する API surface を `nohrs-models` / `nohrs-store` / `nohrs-plugin-host` に絞れる
- workspace inheritance (`[workspace.dependencies]`, `[workspace.lints]`) で依存バージョン・ lint レベルを集中管理
- 将来コアプラグインを `crates/plugins/` 配下に並べやすい

### Negative

- 移行コスト (P1 の作業の一つ、5000 行を 6 crate に切り分け)
- 短期的に PR が膨れる
- crate 間の API を明示する必要 (pub use の整備)

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| ドメイン別細粒度分割 (zed 風、15-25 crates) | 5000 行規模に対し過剰。zed は数十万行で初めて機能する |
| 最小分割 (`nohrs-core` + `nohrs-plugin-host` の 2 つだけ) | plugin が core 全体に依存することになり、API surface が爆発 |
| workspace 化せず単一 crate のまま | 既存問題 (Context) を解決できない |
