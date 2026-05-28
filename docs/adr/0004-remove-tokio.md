# 0004 — tokio を撤去し GPUI executor + runtime-agnostic crates に統一

> Status: Accepted
> Date: 2026-05-28

## Context

v0.1.0 時点での tokio 使用箇所:

- `#[tokio::main]` (`src/gui/main.rs`)
- `tokio::task::spawn_blocking` (services / fs / search)
- `tokio::sync::watch` (search progress)
- `tokio::sync::mpsc` (watcher events)
- `tokio::spawn` (watcher task)
- `axum = "0.7"` (Cargo.toml にあるが未使用)

問題点:

- **GPUI executor との二重スタック**: GPUI は独自 executor を持ち、tokio と並走させると thread の使い分けが不明瞭
- **重い依存**: tokio はビルド時間とバイナリサイズに影響
- **WASM 統合の障害**: wasmtime async モードは tokio 依存。sync モードを採用すれば tokio 不要
- **HTTP server を持たない**: nohrs は GUI app であり axum/HTTP server は当面不要
- **設計シグナルの混乱**: `#[tokio::main]` があると新規 contributor が tokio を前提とした設計を持ち込む

## Decision

**tokio を完全に撤去し、GPUI executor + runtime-agnostic な crates に統一する**。

置換マッピング:

| tokio | 置換先 |
|-------|--------|
| `#[tokio::main]` | GPUI `App::new().run(...)` |
| `tokio::spawn` | `cx.spawn` (foreground) / `cx.background_spawn` (background) |
| `tokio::task::spawn_blocking` | `cx.background_spawn` |
| `tokio::sync::watch` | `postage::watch` |
| `tokio::sync::mpsc` | `async-channel` |
| `tokio::sync::oneshot` | `futures::channel::oneshot` |
| `tokio::time::*` | GPUI `cx.background_executor().timer(...)` |
| `axum` | 削除 (未使用) |
| (HTTP client) | `ureq` |

スケジュール:
- **P1**: `axum` 削除、`#[tokio::main]` 削除、`spawn_blocking` を `cx.background_spawn` に置換
- **P2**: 残りの `tokio::sync::*` / `tokio::spawn` を置換、`Cargo.toml` から `tokio` を削除

検証: `cargo-deny` の `[bans] deny = ["tokio"]` で間接依存も含めて CI で fail させる。

詳細は [`docs/async-runtime.md`](../async-runtime.md) 参照。

## Consequences

### Positive

- 依存ツリー削減 (tokio + 関連 crates が消える)、ビルド時間短縮
- async runtime が GPUI 一本に統一、思考コストが下がる
- wasmtime (P4) を sync API で組める → tokio 不要のまま plugin host を実装可能
- `cargo deny check bans` で混入を機械的に検出

### Negative

- 移行作業 (P1〜P2 を跨ぐ)
- 一部の人気 crate (例: reqwest async) が使えなくなる → ureq / `cx.background_spawn` で代替
- GPUI executor の thread pool サイズや QoS 設定の調整が将来必要 (P4 plugin host 導入時)

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| tokio を維持しつつ GPUI と並走 | 思考コスト高、async runtime の使い分けが contributor に伝わりにくい |
| `smol` を別途立ち上げ | GPUI executor で十分、smol 経由する理由がない |
| `async-std` | 開発停滞、tokio と同等のコスト |
| wasmtime async モード (tokio 依存) を採用 | 撤去方針と矛盾、また nohrs-gpui-wasmtime ([Future Work](../ROADMAP.md#future-work)) との関係で再評価が必要 |
