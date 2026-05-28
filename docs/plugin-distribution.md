# Plugin Distribution — Install / Update / Store

> Status: Draft (P4 で基盤、P5 で Plugin Store)
> Related: [`docs/plugin-overview.md`](./plugin-overview.md), [`docs/plugin-permissions.md`](./plugin-permissions.md), [`docs/web.md`](./web.md)

本書はコミュニティプラグインのインストール / 更新 / 削除 / Plugin Store 統合を定めます。

---

## 1. インストールソース

| ソース | 例 | 解析 |
|--------|-----|------|
| **GitHub `user/repo`** | `syuya2036/nohrs-plugin-example` | デフォルトで `git clone https://github.com/user/repo` (常に HTTPS に解決) |
| **任意 URL (git)** | `https://gitlab.com/.../.git` | **`https://` 必須**。URL を直接 `git clone` |
| **任意 URL (zip/tar.gz)** | `https://example.com/plugin.zip` | **`https://` 必須**。download → SHA-256 checksum 検証 → 解凍 |
| **ローカルパス** | `file:///Users/.../my-plugin` | `[plugins.dev_paths]` に追加、symlink (dev mode) |

- リモート取得 (git / zip / tar.gz) は **`https://` のみ許可**。平文 `http://` は MITM の余地があるため拒否し、install を fail-fast させる。
- 上記 HTTPS 必須はリモートソースのみに適用。ローカル / 開発フロー (`[plugins.dev_paths]`、「ローカルパス」) は例外で、`file://` と dev symlink を許可する。
- ダウンロードしたアーカイブ (zip / tar.gz) は展開前に `plugin.toml` の `[verify] sha256` と SHA-256 を照合し、不一致なら install を中止する (§4 参照)。

---

## 2. プラグインリポジトリの構造

```text
nohrs-plugin-example/
├── plugin.toml                 # manifest (required)
├── README.md
├── LICENSE
├── icon.png                    # 64x64 PNG, plugin icon
├── component.wasm              # Component Model WASM (build artifact, required)
├── assets/                     # 任意の static asset
│   └── icons/
└── src/                        # ソース (build には不要、参考用)
```

| 観点 | 仕様 |
|------|------|
| バイナリ配布 | `component.wasm` を **repo に commit** (GitHub releases asset 経由にしない) |
| 理由 | release asset を fetch するより repo の特定 tag/branch を clone するほうがシンプル。checksum 検証も commit hash で済む |
| プラットフォーム別 build | 不要 (WASM はプラットフォーム非依存)。OS 依存ロジックは WASI cap でランタイム判定 |
| 大きさ上限 | 50 MB 超で警告、200 MB 超で拒否 |

---

## 3. インストールパイプライン

```text
1. 起動: ユーザー操作 (nohrs CLI / Plugin Store ボタン / `nohrs://install?source=user/repo`)
2. config.toml に追加 or memory state 更新
3. nohrs-plugin-host が install task を起動:
   a. source 解析 → git clone or download
   b. plugin.toml 読み取り、schema 検証
   c. engine.nohrs_version の範囲チェック (適合しなければ reject)
   d. component.wasm を wit-bindgen で validate (world 互換性)
   e. permission prompt 表示、ユーザー同意取得
   f. $DATA/nohrs/plugins/<plugin-id>/ にコピー (sha256 で integrity check)
   g. SQLite plugins テーブルに登録、granted_permissions 保存
4. Activation (lazy なら何もしない、eager なら即 Load)
5. UI に "Installed" 通知
```

| 観点 | 仕様 |
|------|------|
| 並列インストール | **不可** (1 つずつ、UI も lock)。同時インストールは複雑化要因 |
| 失敗時 ロールバック | atomic install: tmp dir に展開 → 完了で move。途中失敗で部分状態を残さない |
| 再インストール | 既存があれば overwrite 可、永続データ (KV) は保持 |
| dependency 解決 | plugin A が plugin B に依存することは **当面サポートしない** (P9+)、各 plugin は self-contained |

---

## 4. Integrity / Signing

| 案 | 採用 |
|----|------|
| なし (clone した repo を信用) | ❌ MITM 攻撃の余地 |
| **SHA-256 ハッシュ** (`plugin.toml` の `[verify] sha256` で `component.wasm` の hash 固定) | ✅ |
| minisign / sigstore | ❌ P9+ (key 管理が plugin 作者に重い) |
| **Plugin Store cache 二重照合** | ✅ Plugin Store 経由 install の場合は store side でも同じ hash を保持、相互照合 |

採用方針:
- GitHub repo の commit hash 自体が hash chain なので git で改変検出可
- `plugin.toml` に `[verify] sha256 = "..."` を必須化し、install 時に再計算照合
- Plugin Store 経由 install では store side cache とも一致確認
- 個別 URL インストールは SHA-256 一致のみ (ユーザー責任の範囲)

---

## 5. 更新

| 観点 | 仕様 |
|------|------|
| 更新確認 | 週次バックグラウンド (起動時 + 7 日経過で GitHub API で latest release/tag fetch) |
| 通知 UI | 更新あれば設定ページに "Updates available (3)" バッジ、明示ユーザー操作で update 実施 |
| 自動更新 | デフォルト **オフ** (security 重視)。設定で opt-in |
| 互換性チェック | 新 version の `engine.nohrs_version` が範囲外なら update 不可、ユーザーに警告 |
| permission diff | 増えた permission のみ再プロンプト ([`docs/plugin-permissions.md`](./plugin-permissions.md) §2.1 参照) |
| ロールバック | 1 つ前のバージョンを `$DATA/nohrs/plugins/<id>/.prev/` に保持、UI でロールバック可 |

---

## 6. アンインストール

| 観点 | 仕様 |
|------|------|
| 完全削除 | config から除外、`$DATA/nohrs/plugins/<id>/` 削除、SQLite `plugins` レコード削除 |
| ユーザーデータの扱い | `plugin_kv` table の対応 rows は **デフォルト保持** (再 install したい場合に備えて)。`uninstall --purge` で全削除 |
| 共有データ | 当面 plugin 間共有データ無し → 関連処理なし |

---

## 7. Plugin Store ページ (web、P5)

詳細は [`docs/web.md`](./web.md) §6.5 参照。要点:

### 7.1 登録方式

- PR ベース登録: `web/content/plugins/<id>.toml` に最小情報を書いた PR を web リポジトリに送る
- ビルド時に GitHub API で stars / last commit / README / license / `plugin.toml` を fetch して enrich
- Plugin Store の自動 CI チェック:
  - `plugin.toml` schema 検証
  - SHA-256 整合性
  - `engine.nohrs_version` 範囲
  - icon / README 必須

### 7.2 カテゴリ (初期 5 つ)

- `productivity`
- `developer-tools`
- `media`
- `cloud`
- `theme`

### 7.3 カード UI

各 plugin の Plugin Store カードには:
- icon / name / version / author / stars / last update / license
- **permission バッジ** (`fs:home`, `net`, `process` 等)。詳細は [`docs/plugin-permissions.md`](./plugin-permissions.md)
- Install ボタン: `nohrs://install?source=user/repo` で deeplink
- アプリ未起動時のフォールバック: config.toml に追加する snippet をクリップボードにコピー

### 7.4 動的データ (Future)

- DL 数 / 評価は Future Work (Cloudflare Workers + KV / D1)

---

## 8. CLI サブコマンド (Phase 4 以降)

```bash
# install
nohrs plugin install user/repo
nohrs plugin install https://gitlab.com/.../.git
nohrs plugin install ./local-plugin              # dev mode のみ

# update
nohrs plugin update                              # 全 plugin の更新確認
nohrs plugin update user/repo                    # 個別

# uninstall
nohrs plugin uninstall user/repo
nohrs plugin uninstall user/repo --purge         # KV データも削除

# list
nohrs plugin list

# Plugin Store への submit 用 PR 作成 (P5)
nohrs plugin publish                             # gh CLI 経由で web リポジトリに PR
```

---

## 9. セキュリティ運用

### 9.1 plugin 削除 (緊急)

malicious plugin が判明した場合:
- nohrs 側で blocklist を fetch (`https://nohrs.app/security/blocklist.json`)
- blocklist にある plugin は起動時に **強制 disable** + ユーザー通知
- blocklist は web リポジトリで管理、PR で追加

### 9.2 脆弱性報告

- `SECURITY.md` で `security@nohrs.app` (Future)、当面 GitHub Security Advisories
