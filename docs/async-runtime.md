# Async Runtime — アプリコアの tokio 撤去と GPUI executor 統一

> Status: Draft (P1 でフラグ撤去、P2 で完全置換)
> Related: [`ROADMAP.md`](./ROADMAP.md), [ADR 0004 (remove-tokio)](./adr/0004-remove-tokio.md)

本書はアプリコアの tokio 依存を撤去し GPUI executor + runtime-agnostic crates に置換する戦略と、tokio を WASI プラグイン実行層に隔離する方針 (§6) を定めます。

---

## 1. 動機

| 動機 | 詳細 |
|------|------|
| **重い依存** | tokio は依存ツリーが大きく、ビルド時間とバイナリサイズに影響 |
| **GPUI executor との二重スタック** | GPUI は独自 executor を持つ。tokio と並走させると thread の使い分けが不明確 |
| **WASM 統合の切り分け** | wasmtime core async は executor 非依存だが wasmtime-wasi は tokio 依存。tokio はアプリ全体ではなくプラグイン実行層に隔離する (§6) |
| **HTTP server を持たない** | nohrs は GUI app であり axum/HTTP server は当面不要 |

---

## 2. 置換マッピング

| 現状 (tokio) | 置換先 | 採用 crate / API |
|------------|--------|----------------|
| `#[tokio::main]` | GPUI `App::new().run(...)` | (GPUI 標準) |
| `tokio::spawn` | foreground: `cx.spawn(async move \|cx\| ...)` <br> background: `cx.background_spawn(async move { ... })` | GPUI |
| `tokio::task::spawn_blocking` | `cx.background_spawn` (GPUI background executor は thread pool) | GPUI |
| `tokio::sync::watch` | runtime-agnostic な watch | **`postage::watch`** |
| `tokio::sync::mpsc` | runtime-agnostic な channel | **`async-channel`** |
| `tokio::sync::oneshot` | `futures::channel::oneshot` | `futures` |
| `tokio::sync::Mutex` | (await を持たないなら) `parking_lot::Mutex`、async ロックが要れば `async-lock::Mutex` | `parking_lot` / `async-lock` |
| `tokio::time::timeout` | `futures::future::select` + GPUI timer | `futures` + GPUI |
| `tokio::time::sleep` | `cx.background_executor().timer(duration).await` | GPUI |
| `axum` | (未使用、削除) | — |

---

## 3. HTTP クライアント

| 候補 | 採否 | 理由 |
|------|------|------|
| `reqwest` (async) | ❌ | tokio 必須 |
| `reqwest` (blocking) | ❌ | 内部で tokio を立ち上げ (隠れた依存) |
| **`ureq`** | ✅ | 軽量、sync、tokio 不要、rustls feature 選択可 |
| `isahc` | ❌ | libcurl 依存 |
| `hyper` 直接 | ❌ | 低レベル過ぎ |

`ureq` を `cx.background_spawn` 内で sync 実行。

```rust
let res = cx.background_spawn(async {
    ureq::get("https://api.github.com/repos/noh-rs/nohrs/releases")
        .call()
}).await?;
```

---

## 4. プロセス起動

| 用途 | 方針 |
|------|------|
| ripgrep / mdfind / git 等の short-lived process | `std::process::Command` を `cx.background_spawn` で実行 |
| 長時間の stream (stdout を逐次読みたい) | `std::thread::spawn` で BufReader::lines() を回し、`async-channel` で foreground に渡す |

`async-process` crate の導入は **しない** (上記で十分)。

---

## 5. file watcher (`notify`)

`notify` と `notify-debouncer-mini` は **runtime-agnostic** (内部で `std::thread` を使う)。tokio 撤去とは独立で、変更不要。

---

## 6. Wasmtime / プラグイン実行層 (P4)

wasmtime core の async は executor 非依存だが、**wasmtime-wasi は tokio に深く結合している** (公式に「Tokio executor を必要とし、設計に結びついている」と明記)。nohrs プラグインは WASI Preview 2 を前提とする ([ADR 0005](./adr/0005-wit-bindgen-component-model.md)) ため、ホストは wasmtime-wasi を使い、tokio が必要になる。

方針: **アプリ全体を tokio 化せず、tokio を `nohrs-plugin-host` 内の専用 `current_thread` runtime に隔離する**。

| レイヤー | ランタイム |
|---------|-----------|
| アプリコア (UI / 検索 / インデックス / ファイル操作 / HTTP) | GPUI executor / worker thread / rayon / notify (tokio-free) |
| プラグイン実行層 (wasmtime + wasmtime-wasi) | 専用の小さな tokio `current_thread` runtime |
| 公開プラグイン API (`Plugin` trait) | sync (tokio 非依存、内部で `block_on` してブリッジ) |

```rust
pub struct WasiRuntime { rt: tokio::runtime::Runtime }   // Builder::new_current_thread().enable_all()

impl Plugin for WasiPlugin {
    fn search(&self, query: &str) -> anyhow::Result<Vec<SearchItem>> {
        // tokio を crate の外に漏らさない
        self.wasi_runtime.rt.block_on(async { self.call_wasm_search(query).await })
    }
}
```

ホストは `Plugin` trait を `cx.background_spawn` 内で sync 呼び出しし、UI を block しない。

詳細は [`docs/plugin-overview.md`](./plugin-overview.md) と [`docs/plugin-api.md`](./plugin-api.md) 参照。

将来の `nohrs-gpui-wasmtime` (GPUI executor 上で wasmtime async を駆動し、専用 tokio runtime を不要にするブリッジ) は Future Work。詳細は [`ROADMAP.md`](./ROADMAP.md) §Future Work。

---

## 7. 撤去スケジュール

| Phase | 作業 |
|-------|------|
| **P1** | <ul><li>`axum` を削除 (未使用)</li><li>`#[tokio::main]` → GPUI main 化</li><li>`tokio::task::spawn_blocking` を `cx.background_spawn` に置換 (旧 QUALITY_IMPROVEMENT_PLAN P1.3 と統合)</li></ul> |
| **P2** | <ul><li>残りの `tokio::sync::*` を `postage` / `async-channel` / `futures::channel::oneshot` に置換</li><li>`tokio::spawn` を `cx.background_spawn` に統一</li><li>`Cargo.toml` から `tokio` を削除</li><li>HTTP は `ureq` に置換</li></ul> |
| **検証** | `cargo-deny` で tokio を deny し、`nohrs-plugin-host` 経由 (P4 以降) のみ `wrappers` で許可。P2〜P3 は plugin host 未導入のため `cargo tree \| grep -E '^tokio'` は **空** |

---

## 8. GPUI executor の thread pool サイズ

| 観点 | 推奨 |
|------|------|
| デフォルト | CPU 数 (GPUI 標準) |
| プラグイン host が増えた P4 以降 | 実測で調整。CPU 数 × 1.5 程度まで増やしても OK |
| バッテリー / LowPowerMode | indexing 系は別途リソース throttle (詳細は [`docs/search.md`](./search.md) §resource throttling) |

---

## 9. 検証 (`cargo-deny`)

`deny.toml` の `[bans]`:

```toml
[bans]
deny = [
  # tokio はアプリコアでは禁止。WASI プラグイン実行層 (nohrs-plugin-host, P4 以降)
  # からの依存のみ wrappers で許可する
  { name = "tokio", wrappers = ["nohrs-plugin-host"] },
  { name = "tokio-util", wrappers = ["nohrs-plugin-host"] },
  { name = "openssl-sys" },        # rustls 統一
]
```

`nohrs-plugin-host` 以外の crate に tokio が混入したら CI で fail。P2〜P3 は plugin host 未導入のため tokio は完全に消える。

---

## 10. テストでの注意 (CLAUDE.md 抜粋)

- GPUI テスト (`#[gpui::test]`) では **必ず GPUI executor の timer を使う**: `cx.background_executor().timer(duration).await`
- `smol::Timer::after(...)` / `tokio::time::sleep` は使わない (GPUI scheduler が tracking しない)
- `cx.run_until_parked()` は GPUI 内 task のみを認識
