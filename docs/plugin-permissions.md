# Plugin Permissions

> Status: Draft (P4 で実装)
> Related: [`docs/plugin-overview.md`](./plugin-overview.md), [`docs/plugin-api.md`](./plugin-api.md), [`docs/plugin-distribution.md`](./plugin-distribution.md)

本書はコミュニティプラグインの権限モデル — マニフェスト・同意フロー・サンドボックス境界・危険操作の hard ban — を定めます。

---

## 1. 権限マニフェスト (`plugin.toml`)

```toml
[permissions]
# ファイルシステム読み取り
read_paths  = ["$HOME/Documents/**", "$HOME/Downloads/**"]
# 書き込み (危険度高)
write_paths = ["$HOME/.config/my-plugin/**"]
# ネットワーク (ドメインホワイトリスト必須)
network     = ["api.github.com", "*.example.com"]
# プロセス起動 (実行可能ファイル名のホワイトリスト)
process     = ["rg", "git"]
# クリップボード
clipboard    = "read-write"   # "none" | "read" | "write" | "read-write"
# ホスト API (暗黙許可の API は記述不要 — §1.4 参照)
host_apis    = ["launcher.contribute", "explorer.decorate"]
```

> `logging` / `kv` / `cache` / `metadata` / `notification` は暗黙許可 (§1.4) のため manifest に書く必要はありません。書いても無視されます。

### 1.1 パス指定

| 機能 | 仕様 |
|------|------|
| glob | `**`, `*`, `?` 使用可 (`globset` crate でマッチング) |
| 環境変数展開 | `$HOME`, `$XDG_DOCUMENTS_DIR` 等を展開 |
| 絶対パス必須 | 相対パスは禁止 (manifest として曖昧) |

### 1.2 ネットワーク

| 観点 | 仕様 |
|------|------|
| 形式 | **ホスト名ホワイトリスト** (`*.example.com` wildcard 可) |
| 禁止 | CIDR ブロック (人間が判断不能)、IP リテラル |
| プロトコル | HTTPS のみ。HTTP は dev mode のみ許可 |
| リダイレクト | 同一 host への redirect のみ追従 |

### 1.3 プロセス

| 観点 | 仕様 |
|------|------|
| 形式 | **コマンド名ホワイトリスト** (`["rg", "git"]`) |
| 禁止 | 絶対パス指定 (`/usr/local/bin/rg` 等)、shell 経由 (`sh -c`) |
| 引数フィルタ | なし (`process` 取得は十分強い権限と説明) |

### 1.4 暗黙許可 (manifest 記述不要)

- `logging`
- `kv` (自プラグイン専用)
- `cache` (自プラグイン専用)
- `metadata` (ただし `read_paths` 範囲内)
- `notification` (レート制限あり)

### 1.5 不明な permission

manifest に未知 key があれば:
- production: **install 拒否**
- alpha バージョン: warning ログ + 無視 (forward-compat のため)

---

## 2. ユーザー同意フロー

### 2.1 タイミング

| タイミング | 動作 |
|-----------|------|
| **インストール時** | 1 回だけプロンプト |
| 実行時 | プロンプトしない (UX 悪化) |
| update 時 | **増えた permission のみ** 再プロンプト、減少 / 同等なら自動継続 |

### 2.2 プロンプト UI (例)

```text
┌──────────────────────────────────────────────────────────┐
│ Install Plugin: syuya2036/nohrs-plugin-example v0.1.0    │
├──────────────────────────────────────────────────────────┤
│ This plugin requests the following permissions:          │
│                                                          │
│ 🔒 Read files in:                                       │
│    • ~/Documents/**                                      │
│    • ~/Downloads/**                                      │
│ ⚠️  Write files in:                                      │
│    • ~/.config/my-plugin/**                              │
│ 🌐 Network access to:                                    │
│    • api.github.com                                      │
│ 🚀 Run processes: rg, git                                │
│                                                          │
│ Source: https://github.com/syuya2036/nohrs-plugin-…      │
│ ✓ verified GitHub repo, 42 stars, MIT license            │
│                                                          │
│  [ Cancel ]  [ Customize ]  [ Allow All ]               │
└──────────────────────────────────────────────────────────┘
```

### 2.3 同意の粒度

| 操作 | 動作 |
|------|------|
| **Customize** | 各 permission を個別 toggle 可。uncheck したまま install すると、その API は plugin から呼ぶと `NotPermitted`。暗黙許可 (§1.4: logging / kv / cache / metadata / notification) は consent の対象外で、toggle にも現れない |
| 既定値 | 危険度低 (read_paths, clipboard read) は default checked、`write_paths` / `process` / `network` は default unchecked (明示 opt-in) |
| 危険度の表示 | アイコン色分け (🔒 グレー / ⚠️ オレンジ / 🚨 赤) |
| repo metadata 表示 | stars / last update / license / verified GitHub も同時表示 (transparency) |

### 2.4 同意の永続化

- SQLite `plugins.granted_permissions` カラムに JSON で保存
- 設定ページで個別 toggle 可、reload 後に反映

### 2.5 Revoke

設定 → Plugins → 該当 plugin → permission 一覧の toggle。reload 後に反映。

---

## 3. サンドボックスの 2 層構造

### 3.1 層 1: WASM sandbox (wasmtime)

| 観点 | 仕様 |
|------|------|
| メモリ分離 | wasmtime engine が物理メモリを分離 (plugin は host メモリを直接読めない) |
| WASI Capability | `WasiCtxBuilder` で **何も渡さない** (デフォルトで全閉)、必要な capability のみ明示的に渡す |
| File capability | `wasi:filesystem/preopens` で `read_paths` / `write_paths` をマウント |
| Network capability | `wasi:sockets/...` は使わず、host import `network.http-fetch` 経由のみ |

### 3.2 層 2: capability filter (host functions)

各 host function 実装の冒頭で permission チェック:

```rust
// 擬似コード
fn http_fetch(ctx: &PluginContext, req: HttpRequest) -> Result<HttpResponse> {
    ctx.permissions.check_network(&req.url)?;
    // ... 実行
}
```

`check_network` は manifest の `network` ホワイトリストと突き合わせ、不一致なら `NotPermitted`。

### 3.3 Defense in depth

両層で同じ判断を独立に行う:
- WASI で path を許可しても、host function 側で manifest 範囲外なら拒否
- どちらか一方の bug があってもセキュリティが maintain される

### 3.4 エラーモデル

permission 違反は **plugin に `NotPermitted` を返す** (wasmtime trap させない)。plugin が graceful にハンドルできるように。

```wit
// 例: fs.read-file
read-file: func(path: string) -> result<list<u8>, fs-error>;
variant fs-error { not-permitted, not-found, io-error(string) }
```

---

## 4. 特別な扱い

### 4.1 コアプラグイン

permission チェック **完全 bypass** (Rust ネイティブで host 信頼コード)。

### 4.2 dev mode

`nohrs --dev` 起動時:
- `--allow-all-permissions` フラグで全許可
- production では使えない (CLI が hidden)
- `~/dev/nohrs-plugins/<path>` を `[plugins.dev_paths]` に登録、permission prompt なしで load

### 4.3 明示的拒否 (deny)

Customize で uncheck したものは `granted: false` で保存し、API call で `NotPermitted` を返す (granted の逆として明示)。

---

## 5. 危険操作の追加防御 (hard ban)

manifest が要求してもブロックされる操作:

| カテゴリ | 禁止対象 | 理由 |
|---------|---------|------|
| **network: 個人情報送信** | `~/.ssh/`, `~/.aws/`, `~/.gnupg/`, `~/.config/gh/` を network 経由で送信 | manifest で fs read 許可されても、これらを send 先と組み合わせると検出して reject |
| **process: shell injection** | `sh`, `bash`, `zsh`, `cmd.exe`, `powershell` をコマンド名で要求 | 引数フィルタが効かなくなる |
| **process: 引数の shell expansion** | `process.spawn` は **必ず list、shell なし** | `sh -c` 直渡し禁止、host 側で reject |
| **fs.write: システムパス** | `/etc/`, `/usr/`, `/bin/`, `/sbin/`, `~/Library/Preferences/`, `~/Library/Application Support/` (nohrs 自身を除く) | manifest で要求しても install 時に hard reject |
| **fs.write: nohrs データ** | `$XDG_DATA_HOME/nohrs/db.sqlite`, `$XDG_DATA_HOME/nohrs/plugin-kv.redb` | 他 plugin のデータや host state の破壊を防ぐ |
| **network: localhost** | `localhost`, `127.0.0.1`, `0.0.0.0`, `::1` | manifest で許可しても reject (内部サービスへの攻撃を防ぐ) |
| **network: link-local / private IP** | `169.254.0.0/16`, `192.168.0.0/16`, `10.0.0.0/8`, `172.16.0.0/12` | SSRF 防御 |

これらは host 実装側で hardcoded、user override 不可。

---

## 6. plugin の signature/integrity

詳細は [`docs/plugin-distribution.md`](./plugin-distribution.md) §integrity 参照。

- SHA-256 ハッシュ (`plugin.toml` `[verify] sha256`) + Plugin Store cache の二重照合
- minisign / sigstore は P9+ 検討

---

## 7. ログ・監査

| 項目 | 仕様 |
|------|------|
| permission denied | tracing で `WARN` ログ |
| 設定からの履歴閲覧 | "Last 30 days of permission events" を設定ページに表示 (P5) |
| 異常パターン | network 大量呼び出し / fs 大量書き込み 等は将来 anomaly detection (Future Work) |
