# Testing & Quality Infrastructure

> Status: Draft (P1 で基盤構築、後続 Phase で拡充)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/dev-environment.md`](./dev-environment.md)

本書はテスト方針 / カバレッジパイプライン / 静的解析のセットを定めます。

---

## 1. テスト層構成

| 層 | 配置 | 用途 |
|----|------|------|
| **ユニットテスト** | 各 crate 内の `#[cfg(test)] mod tests` | 関数単位の検証 |
| **GPUI テスト** | 各 crate 内、`TestAppContext` を使う | view state / async / cx interaction の検証 |
| **統合テスト** | `crates/<crate>/tests/` | crate 跨ぎでない、対外的なシナリオ |
| **end-to-end** | (P5 以降検討) | UI レベルの自動操作。`script/ui-run.sh` の延長 |

ワークスペース root の `tests/` ディレクトリは **撤廃** します (現在の `tests/indexing_test.rs` / `tests/watcher_test.rs` は P1 で削除予定。検索リアーキの一環)。

### GPUI テストの基本パターン

CLAUDE.md のルール (重要):

```rust
#[gpui::test]
async fn test_my_view(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| MyView::new(cx));
    // 必ず GPUI executor の timer を使う
    cx.background_executor.timer(Duration::from_millis(50)).await;
    cx.run_until_parked();
    entity.read_with(cx, |this, _cx| {
        assert_eq!(this.something, expected);
    });
}
```

**避けるべきパターン**: `smol::Timer::after` / `tokio::time::sleep`。これらは GPUI scheduler が tracking しないため `run_until_parked()` が早期に "nothing left" を返す。

### snapshot / property

| 技術 | 用途 |
|------|------|
| `insta` | 検索パーサ / config パーサ / WIT bindings 等の出力固定 |
| `proptest` | 検索クエリ正規化、permission ガード等で限定的に。**過剰投入しない** |
| `tempfile` | 実ディレクトリ作成型のテスト |

---

## 2. カバレッジパイプライン

GitHub の native PR coverage 機能 (2026-05 public preview) と R2 への HTML レポートを **併用**。

### CI ワークフロー (擬似)

```yaml
- name: Run coverage
  run: |
    cargo llvm-cov --all-features \
      --lcov --output-path lcov.info \
      --html --output-dir target/llvm-cov

- name: Upload lcov to GitHub Coverage
  uses: actions/upload-artifact@v4
  with:
    name: coverage-lcov
    path: lcov.info
  # → GitHub Native の PR diff coverage に inline 表示

- name: Upload HTML to R2
  run: |
    wrangler r2 object put nohrs-coverage/pr/${{ github.event.number }}/ \
      --file target/llvm-cov \
      --recursive

- name: Comment PR
  uses: actions/github-script@v7
  with:
    script: |
      const url = `https://coverage.nohrs.app/pr/${{ github.event.number }}/`;
      github.rest.issues.createComment({
        ...context.repo,
        issue_number: context.issue.number,
        body: `📊 Coverage HTML report: ${url}`,
      });
```

### R2 上の HTML レポート 命名と寿命

| パス | 寿命 |
|------|------|
| `coverage.nohrs.app/pr/<number>/` | PR open 中のみ。PR close で Worker が削除 |
| `coverage.nohrs.app/main/<short-sha>/` | 最新 50 件保持。古いものは Worker で削除 |
| `coverage.nohrs.app/main/latest/` | 常に最新の main を指す symlink (object copy) |

### 閾値 (gate)

| Phase | 閾値 | 動作 |
|-------|------|------|
| P1〜P5 | 目標値 (core 80% / 全体 50%) を **PR コメント表示のみ** | fail させない (baseline 確立まで) |
| P6 | 閾値で fail (詳細は P6 時点で再検討) | CI required |

理由: 初期に厳しい gate を設けるとコントリビュータが心折れる。baseline が固まるまで informational に留める。

外部 SaaS (Codecov / Coveralls) は **採用しない**。GitHub Native + R2 HTML で十分かつエコシステム内で完結。

---

## 3. 静的解析・ルール定義

| ツール | 用途 | CI |
|--------|------|-----|
| `cargo fmt --check` | フォーマット | required |
| `cargo clippy -- -D warnings -W clippy::unwrap_used -W clippy::expect_used` | lint | required |
| `cargo-deny check` | ライセンス・脆弱性・重複依存・bans | required (P1 から) |
| `cargo-machete` | 未使用 dependency | 週次 |
| `typos` | typo 検出 | required (固有名詞は `typos.toml` で除外) |

### `clippy.toml`

```toml
disallowed-methods = [
  { path = "std::fs::read", reason = "use services::fs or cx.background_spawn" },
  { path = "std::fs::write", reason = "use services::fs or cx.background_spawn" },
  { path = "std::fs::read_to_string", reason = "use services::fs or cx.background_spawn" },
]
```

### `rustfmt.toml`

```toml
edition = "2021"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
reorder_imports = true
```

### `deny.toml`

```toml
[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "Unicode-DFS-2016", "ISC", "Zlib"]
confidence-threshold = 0.93

[advisories]
db-path = "~/.cargo/advisory-db"
db-urls = ["https://github.com/RustSec/advisory-db"]
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"

[bans]
multiple-versions = "warn"
deny = [
  { name = "tokio" },              # P2 以降、撤去後の混入防止
  { name = "openssl-sys" },        # rustls 統一
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-git = []
```

---

## 4. ドキュメントテスト

- `#![warn(missing_docs)]` を **P1 から workspace 全体で有効**
- pub API に rustdoc が付いていなければ warning
- P7 で `deny` に格上げ
- `#[doc(test)]` でコード例の動作を確認

---

## 5. ベンチマーク (P2 以降)

| crate | ベンチ対象 |
|-------|-----------|
| `nohrs-store` | SQLite/redb の get/put レイテンシ、batch 操作 |
| `nohrs-services` | fs listing 並列度、search クエリ実行 |
| `nohrs-launcher` (P3) | nucleo ranking、起動時間 |
| `nohrs-plugin-host` (P4) | WIT host call レイテンシ、permission check overhead |

`criterion` crate を採用。CI では fail させず、main へのマージで履歴保存 (将来 regression 検知に使用)。

---

## 6. ローカル検証コマンド

```bash
# 全テスト
cargo test --all-features --workspace

# カバレッジ (HTML を target/llvm-cov/html で開く)
cargo llvm-cov --all-features --html
open target/llvm-cov/html/index.html

# lint
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings \
    -W clippy::unwrap_used -W clippy::expect_used

# 静的解析
cargo deny check
cargo machete
typos

# unsafe / panic 残置検出
grep -rn 'unsafe' crates/ | grep -v '//' | grep -v 'unsafe_code' || echo OK
grep -rn 'panic!\|unimplemented!\|todo!' crates/

# rustdoc
cargo doc --no-deps --all-features --workspace
```
