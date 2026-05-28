# 0005 — プラグインホストは wit-bindgen + WASM Component Model 一直線

> Status: Accepted
> Date: 2026-05-28

## Context

プラグイン設計の Gist (作者本人による設計案) は、初期 MVP に **Extism** を採用し、API が固まってから **wit-bindgen / Component Model** に移行する 2 段階アプローチを提示していた。

しかし、ROADMAP の検討で以下の点が浮上した:

- nohrs はまだ **既存ユーザーがいない** (v0.1.0、コミット 42 本)
- MVP 期間に維持すべき後方互換性が存在しない
- ユーザー (作者) が明示的に「**WASM Preview2 の仕様**」を要件として挙げている。Extism は Preview 2 (Component Model) を経由しない
- 2026 年現在、Component Model は wasmtime 30+ で **stable**、実用段階
- 多言語対応 (Rust / TypeScript / Python) の状況:
  - Rust: `wit-bindgen` 公式
  - TypeScript: `jco` / `componentize-js` で production-ready
  - Python: `componentize-py` で動作確認済
- Extism MVP → wit-bindgen 移行を行うと、**初期 plugin 作者が二度書き直しを強いられる**

## Decision

**Extism を経由せず、最初から wit-bindgen + WASM Component Model 一直線で plugin host を実装する**。

- **WASM runtime**: wasmtime 30+
- **モード**: sync API ([ADR 0004](./0004-remove-tokio.md) と整合)
- **bindgen**: wit-bindgen
- **WIT world**: `nohrs:plugin@0.1.0` (`crates/nohrs-plugin-host/wit/world.wit`)
- **対応言語 (P4)**: Rust + TypeScript + Python の 3 つ。Go は P5 以降 (tinygo の component model 対応待ち)

詳細は [`docs/plugin-overview.md`](../plugin-overview.md), [`docs/plugin-api.md`](../plugin-api.md), [`docs/plugin-templates.md`](../plugin-templates.md) 参照。

## Consequences

### Positive

- 初期 plugin 作者が一度書けばよい (移行コスト二重投資ゼロ)
- 型安全な API (WIT type system)、Extism のバイト列ベースより堅牢
- Component Model の言語サポートが急速に成熟しており、選定が将来安心
- ユーザー要件 (Preview 2) と整合

### Negative

- 初期 MVP まで時間がかかる (Extism の数日 vs wit-bindgen の 2-3 週間)
- wasmtime / wit-bindgen の API 変動 (毎リリースで微変更がある) に追随コストが発生
- plugin 作者は wit-bindgen / jco / componentize-py のいずれかをセットアップする必要 (Extism ほどのお手軽さはない) → テンプレ + AI agent skills でカバー

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| Extism MVP → wit-bindgen 移行 (Gist 原案) | 初期 plugin 作者の二度書き直し、API 仕様変更による混乱 |
| Wasmer ベース | Component Model 対応が wasmtime ほど成熟していない |
| 独自 WASM runtime / 独自 ABI | 過剰、エコシステム断絶 |
| WebExtensions 風 (JavaScript only) | Rust / Python 等の言語選択肢を失う |
