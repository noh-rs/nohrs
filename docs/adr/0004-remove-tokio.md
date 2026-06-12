# 0004 — tokio をアプリコアから撤去し、WASI プラグイン層に隔離する

> Status: Accepted
> Date: 2026-05-28

## Context

0.0.x 時点 (single-crate 期) での tokio 使用箇所:

- `#[tokio::main]` (`src/gui/main.rs`)
- `tokio::task::spawn_blocking` (services / fs / search)
- `tokio::sync::watch` (search progress)
- `tokio::sync::mpsc` (watcher events)
- `tokio::spawn` (watcher task)
- `axum = "0.7"` (Cargo.toml にあるが未使用)

問題点:

- **GPUI executor との二重スタック**: GPUI は独自 executor を持ち、tokio と並走させると thread の使い分けが不明瞭
- **重い依存**: tokio はビルド時間とバイナリサイズに影響
- **WASM 統合との切り分け**: wasmtime core の async は executor 非依存だが、wasmtime-wasi は tokio に深く結合している (公式に「Tokio executor を必要とし、設計に結びついている」と明記)。WASI プラグインを動かすなら tokio は避けられない。ただしアプリ全体を tokio 化する必要はなく、プラグイン実行層に隔離できる
- **HTTP server を持たない**: nohrs は GUI app であり axum/HTTP server は当面不要
- **設計シグナルの混乱**: `#[tokio::main]` があると新規 contributor が tokio を前提とした設計を持ち込む

## Decision

**tokio をアプリコア (UI / 検索 / インデックス / ファイル操作 / HTTP) から撤去し、GPUI executor + runtime-agnostic な crates に統一する。tokio は WASI プラグイン実行層 (`nohrs-plugin-host`) だけに隔離し、専用の小さな `current_thread` runtime に閉じ込める。公開プラグイン API (`Plugin` trait) は tokio 非依存の sync に保つ**。

アプリコアの置換マッピング:

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

### プラグイン実行層の例外 (P4)

WASI Preview 2 プラグインのホスト実装 (`wasmtime` + `wasmtime-wasi`) は tokio に依存する。これはアプリ全体を tokio 化するのではなく、`nohrs-plugin-host` 内の専用 `current_thread` tokio runtime に閉じ込める:

```rust
pub struct WasiRuntime {
    rt: tokio::runtime::Runtime,   // Builder::new_current_thread().enable_all()
}

impl Plugin for WasiPlugin {
    // 公開 trait は sync。内部で block_on して tokio を crate の外に漏らさない
    fn search(&self, query: &str) -> anyhow::Result<Vec<SearchItem>> {
        self.wasi_runtime.block_on(async { self.call_wasm_search(query).await })
    }
}
```

ホスト側は `Plugin` trait を `cx.background_spawn` 内で sync 呼び出しする。tokio はこの crate の外には現れない。

スケジュール:
- **P1**: `axum` 削除、`#[tokio::main]` 削除、`spawn_blocking` を `cx.background_spawn` に置換
- **P2**: 残りの `tokio::sync::*` / `tokio::spawn` を置換、`Cargo.toml` から `tokio` を削除

検証: `cargo-deny` の `[bans]` で tokio を deny。ただし `nohrs-plugin-host` (P4 で追加) を `wrappers` に登録し、プラグイン実行層からの依存のみ許可する。それ以外の crate に tokio が混入したら CI で fail させる。P2〜P3 は plugin host 未導入のため tokio は完全に消える。

詳細は [`docs/async-runtime.md`](../async-runtime.md) 参照。

## Consequences

### Positive

- アプリコアの依存ツリー削減 (tokio + 関連 crates がコアから消える)、ビルド時間短縮
- アプリコアの async runtime が GPUI 一本に統一、思考コストが下がる
- tokio はプラグイン実行層に隔離されるため、アプリコアの依存ツリーと思考モデルは tokio-free を維持
- `cargo deny check bans` で `nohrs-plugin-host` 以外への混入を機械的に検出

### Negative

- 移行作業 (P1〜P2 を跨ぐ)
- 一部の人気 crate (例: reqwest async) が使えなくなる → ureq / `cx.background_spawn` で代替
- GPUI executor の thread pool サイズや QoS 設定の調整が将来必要 (P4 plugin host 導入時)
- プラグイン実行層に専用 tokio runtime を持つため、その lifecycle (起動 / 停止 / スレッド数) の管理が必要

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| アプリ全体を tokio 化 (GPUI と並走) | 思考コスト高、二重 executor。プラグイン実行層への隔離で足りる |
| `smol` を別途立ち上げ | GPUI executor で十分、smol 経由する理由がない |
| `async-std` | 開発停滞、tokio と同等のコスト |
| wasmtime-wasi を避け、sync wasmtime + 独自 host function のみで tokio を完全排除 | WASI Preview 2 ([ADR 0005](./0005-wit-bindgen-component-model.md)) がユーザー要件。WASI を捨てると言語サポートと capability モデルを失う |
