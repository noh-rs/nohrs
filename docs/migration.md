# Migration — 乗り換えの容易性

> Status: Draft (要件・設計。移行ウィザード v1 は P3.5、拡張互換 SDK は P5)
> Related: [`launcher-requirements.md`](./launcher-requirements.md) / [`plugin-api.md`](./plugin-api.md) /
> [`plugin-templates.md`](./plugin-templates.md) / [`config.md`](./config.md) / [`cli.md`](./cli.md)

本書は「Raycast / Tinycast / Supaste / Alfred などから nohrs に乗り換えるとき、**設定を作り直さずに済む**」ための
設計を定めます。同時に「**nohrs から出ていくのも同じくらい簡単**である」ことを保証します。この 2 つは同じ 1 つの
約束の裏表です ([`launcher-requirements.md`](./launcher-requirements.md) 原則 3)。

---

## 1. 目標と非目標

### 目標

| # | 目標 | 測り方 |
|---|------|--------|
| **G1** | 初回起動から **5 分以内**に、以前の環境の「毎日使う部分」が動く | スニペット・Quicklink・ホットキー・エイリアス・お気に入り・クリップボード履歴が移っている |
| **G2** | 移行は**差分レポート**を出す。移せなかったものを黙って落とさない | 「移行: 42 / 未対応: 3 (理由つき)」の画面 |
| **G3** | 移行は**取り消せる** | 実行前にスナップショットを取り、`noh migrate undo` で戻る |
| **G4** | 既存の Raycast 拡張を**ソースから**再ビルドして動かせる | `@nohrs/raycast-compat` + `jco componentize` で主要拡張が動く |
| **G5** | nohrs の全データを平文で持ち出せる | `noh export --all` が JSON + blob の zip を吐く |

### 非目標

- Raycast 拡張の**バイナリ互換実行** (Node ランタイム同梱)。理由は [`launcher-requirements.md`](./launcher-requirements.md) §10 と本書 §4.1。
- 移行元アプリの**自動アンインストール**。ユーザーの領分。
- 移行元アプリのライセンス・利用規約に反する取得方法 (ストアからの一括ダウンロード等)。

---

## 2. 移行元マトリクス

| 移行元 | 何を移すか | 入手経路 | 難度 | Phase |
|--------|-----------|---------|------|-------|
| **Raycast** | Snippets / Quicklinks | 公式の**非暗号化 JSON エクスポート** | 低 | P3.5 |
| Raycast | ホットキー / エイリアス / お気に入り / 設定 / クリップボード履歴 / Notes / Window レイアウト | `.rayconfig` (暗号化、パスフレーズ ≥8 文字) | 中〜高 (形式が非公開、R5) | P3.5→P4 |
| Raycast | 拡張 | ソースから再ビルド (§4) | 高 | P5 |
| **Tinycast** | 設定一式 | 「Backup & restore」のエクスポートファイル | 中 (形式要調査 R11) | P4 |
| **Supaste** | クリップボード履歴・カテゴリ | ローカル保存形式 (要調査 R6) | 中 | P4 |
| **Maccy / Paste / Pastebot / CopyClip** | クリップボード履歴 | 各アプリのローカル DB (要調査 R6) | 中 | P4 (コミュニティ importer) |
| **Alfred** | スニペット (`.alfredsnippets`) | zip + JSON。形式が公開されている | 低 | P4 |
| Alfred | ワークフロー | — | 非対応 (§4.5) | — |
| **Spotlight / Finder** | 何もない (移行不要) | — | — | — |

**方針**: 公式の**非暗号化エクスポートがあるものを最優先**にします。暗号化形式や内部 DB の解析に依存する経路は、
「あれば嬉しい」であって G1 の前提にしません。

---

## 3. 移行の仕組み

### 3.1 中間形式 — Migration Interchange Format (MIF)

すべての importer は、移行元の形式を**中間の JSON** に変換してから nohrs のストアに書きます。
importer とストア書き込みを分けることで、(a) 新しい移行元をコミュニティが足しやすく、(b) dry-run と差分レポートが
1 実装で済み、(c) そのまま**エクスポート形式**にもなります (G5)。

```jsonc
{
  "mif_version": 1,
  "source": { "app": "raycast", "version": "2.x", "exported_at": "2026-09-16T00:00:00Z" },
  // このアーカイブが実際に持っているストアの一覧。空のストアと「入っていないストア」を区別します
  "stores": ["snippets", "quicklinks", "hotkeys", "aliases", "favorites", "clips", "config", "history", "window_state", "shelf"],
  "snippets":   [ { "id": "...", "name": "Email", "keyword": ";em", "body": "...", "placeholders": ["clipboard"] } ],
  "quicklinks": [ { "id": "...", "name": "GitHub Search", "target": "https://github.com/search?q={argument}", "app": null } ],
  // ホットキー・エイリアス・お気に入り・Quicklink は同じ bound_command を指します (§5.10 の束縛済みコマンド)
  "hotkeys":    [ { "bound": { "command": "app.launch", "args": { "bundle_id": "com.tinyspeck.slackmacgap" }, "title": "Slack" },
                    "chord": "Cmd+Opt+S", "scope": "global", "leader": null } ],
  "aliases":    [ { "bound": { "command": "clipboard.history", "args": {}, "title": "クリップ履歴" }, "alias": "cb" } ],
  "favorites":  [ { "bound": { "command": "explorer.open_path", "args": { "path": "~/src" }, "title": "src" }, "rank": 1 } ],
  "clips":      [ { "kind": "text", "preview": "...", "body_ref": "blobs/0001.txt", "source_app": "com.apple.Safari", "copied_at": 0, "pinned": false } ],
  "notes":      [ { "title": "...", "body_ref": "notes/foo.md" } ],
  "unsupported":[ { "kind": "extension", "id": "raycast/spotify", "reason": "拡張は再ビルドが必要 (§4)" } ]
}
```

- **`stores` は「このアーカイブが持っていると主張するもの」の一覧**です。
  `plugins` と `plugin_kv` は plugin host (P4) が無いと存在しないので、P3.5 に作ったアーカイブには
  入りません ([`persistence.md`](./persistence.md) §7)。これを書いておかないと、
  **「元から無かった」と「入っているはずが欠けている」が区別できません**。
  import は apply の前に検証します:
  - `stores` に挙がっているのに中身が無い → **エラーで止めます** (壊れたアーカイブ)。
  - `stores` に無い → そのストアは移行対象外。エラーにも `unsupported` にもしません。
  - `stores` にあるが**こちらが知らない名前** → `unsupported` に落として報告します
    (新しい版の nohrs が作ったアーカイブを古い版で開いた場合)。

- **`bound` は 4 箇所で同じ形**です (`command` + `args` + `title`)。ホットキー・エイリアス・
  お気に入り・Quicklink をそれぞれ別の形にすると、「Slack を `Cmd+Opt+S` で開く」の**引数と表示名が
  往復で落ちます** ([`launcher-requirements.md`](./launcher-requirements.md) §5.10 の D10)。
  Quicklink は `bound` に加えて `target` (URL やパス) を持ちます。ホットキーだけは `scope`
  (`global` / `local`) と `leader` (リーダーの後の 1 文字。無ければ `null`) を持ちます。
  この 3 つのどれかを表現できない移行元の記録は、`unsupported` に落として報告します。
- `*_ref` は**アーカイブ内の相対パス**。大きい値 (画像・ファイル・メモ) は blob として同梱。
  **展開前に必ず検証します**: 絶対パス・`..` を含むもの・シンボリックリンク・展開後の実パスが
  ステージングルートの外に出るものは、すべて拒否して `unsupported` に落とす。
  移行アーカイブは他人から渡され得るので、ここが緩いとインポートが任意の場所への書き込みになります。
- **展開を始める前に、アーカイブ全体の大きさを見ます。** 1 アイテム 50MB / 総量 2GB は展開後の blob に
  対する上限で、**圧縮されたまま**の入力や、blob ですらないメタデータの件数は縛れません。
  メンバ数 (既定 10 万) と圧縮後の総バイト数 (既定 500MB)、そして**展開後の総バイト数 / 圧縮後の総バイト数**
  の比 (既定 100 倍) を先に見て、超えたら**1 バイトも書かずに**拒否します。
  ストリームしながら数え、途中で超えたらそこで止めます。拒否・失敗のどちらで終わっても
  ステージングは消します。これが無いと、数 KB のファイルで CPU とディスクを使い切らせられます。
- **上に挙げた 7 種類は移行元から来るものだけ**です。`noh export --all` (G5) はこれに加えて
  `config` (config.toml のスナップショット) / `history` (開封・検索・コマンドの履歴。**使用統計 `command_usage` は含めません** — `history` の `kind="command"` から再生成できる派生値で、二重に持つと import 時にどちらが正かを決める羽目になります) /
  `window_state` / `shelf` / `plugins` (id・バージョン・由来・**以前に許可された権限** + **`plugin.toml` と `component.wasm` の blob 本体と sha256**) / `plugin_kv`
  (plugin ごとの KV、[`persistence.md`](./persistence.md) §3 の隔離単位のまま) を含みます。
  **含まないもの**は再生成可能なキャッシュ (検索インデックス / サムネイル) だけで、それは export に入れません。
  plugin は**実体を同梱します**。メタデータだけでは、移行先に同じ plugin が無ければ復元できず、
  再取得はネットワークと配布元の生存に依存するからです。`--no-plugin-blobs` で由来だけにもでき、
  その場合の import は取得を試み、**失敗したものを 1 件ずつ理由つきで報告**します (黙って減らさない)。
  ただし**由来元に取りに行くのは、それ自体が通信**なので、原則 1 のとおり**別途の明示的な許可**を
  求めます。plugin の権限同意とは**別の確認**です (権限を許すことと、外に取りに行かせることは違います)。
  許可しなければ、その plugin は「実体が無い」として `unsupported` に落ちます。
  取りに行く先は**アーカイブに書かれた URL** — 他人が書いた文字列なので、OGP と同じく
  スキームを `https` に限り、プライベート IP・ループバック・リンクローカルへは行きません
  ([`launcher-requirements.md`](./launcher-requirements.md) §5.9)。取得した実体は
  記録された sha256 と照合し、合わなければ捨てます。
- **アーカイブに入っている権限は「以前に許可された」という記録であって、許可そのものではありません。**
  import はこれを**要求として扱い**、インストール時の同意フローを通常どおり出します。アーカイブは
  他人から渡され得るので (上の展開前検証と同じ理由)、中の `granted_permissions` をそのまま信じると、
  **アーカイブを 1 つ開くだけで plugin に権限が付く**ことになります。同意を取るまで plugin は
  アクティベートしません。
  - 記録は同意画面で使います: 「移行元では ① ② が許可されていました」と**前もって見せる**ので、
    1 件ずつ思い出す必要はありません。既定のチェックは**入れません** (見せることと、押しておくことは別です)。
  - 移行元と実体の `plugin.toml` がズレている場合は、[`launcher-requirements.md`](./launcher-requirements.md) §7.1 と
    同じく**広い方**を同意の対象にします。
- **このアーカイブは平文です。** クリップボードの中身・設定・履歴・シェルフ・plugin の KV が
  そのまま入るので、**読めた者はそれを全部読めます**。暗号化は付けません — 鍵の管理と復旧を背負うと、
  「ロックインしない」の逆側に倒れるからです。代わりに扱いを決めます:
  - 作るときは**所有者のみ** (`0600`、置くディレクトリは `0700`)。Windows は mode が無いので、
    ユーザープロファイル配下の ACL を継ぐ場所に置きます ([`logging.md`](./logging.md) と同じ扱い)。
  - **一時ファイルに書いてから rename** します。途中の中身が他から読める時間を作りません。
  - **書き出したあとに 1 行出します**: 平文であること、共有するなら自分で暗号化すること。
  - 適用前スナップショット (`backups/pre-migrate-<ts>.zip`) も**まったく同じ扱い**です。
    あれは自動で作られるぶん、忘れられたまま残りやすいので、なおさら同じにします。
- `unsupported` は**importer が自分で埋める**。これが G2 の差分レポートの元データになります。
- MIF は `docs/mif.schema.json` として JSON Schema を生成・コミットする ([`config.md`](./config.md) §4 と同じ運用)。

### 3.2 適用 (apply)

| 観点 | 仕様 |
|------|------|
| 衝突 | 既定は **skip** (既存を壊さない)。`--on-conflict=overwrite \| rename \| skip` で変更可 |
| dry-run | 既定。`--apply` を付けるまで書き込まない |
| スナップショット | 適用前に `noh export --all` 相当を `$XDG_DATA_HOME/nohrs/backups/pre-migrate-<ts>.zip` に取る (G3)。これは [`persistence.md`](./persistence.md) §7 の export/import を **P3.5 に前倒しする**ということで、同書もそう更新済み |
| **スナップショットは 1 時点** | データは SQLite・redb・blob ディレクトリに**分かれて**います。書き込みが走ったまま順に読むと、SQLite は新しく redb は古い、という**どの瞬間にも存在しなかった状態**が保存され、undo がそれを復元します。取得中は全ストアへの書き込みを止め、共通の世代番号を固定してから読みます。止められない場合 (長いファイル操作の途中など) は**スナップショットを取らず、移行を始めません** — 戻せない移行は「取り消せる」と言えないので。**blob も同じ柵の内側です**: SQLite の行は blob を参照しているだけなので、blob の作成・差し替え・削除を止めずに撮ると、undo が「行はあるが blob が無い」か「行より新しい blob」を復元します。blob の書き手も同時に止め、世代に紐づいた blob の一覧 (パスと sha256) をスナップショットに入れ、undo はその一覧どおりに戻します |
| **undo の安全規則** | `noh migrate undo` はスナップショットで**丸ごと戻す**操作なので、適用後に足したデータを消し得ます。適用時点のストア世代を記録しておき、**適用後に変更があったら既定で拒否**し、何が失われるかを示したうえで `--force` を要求します。「取り消せる」と「後の作業を消す」は別物です |
| 検証 | ホットキーは**衝突検出**を通し、衝突したものは `unsupported` に落として報告する (黙って上書きしない) |
| 機微情報 | 取り込むクリップにも除外ルール ([`launcher-requirements.md`](./launcher-requirements.md) §5.4) を適用する。移行元に残っていたパスワードを nohrs に持ち込まない |
| **保持の上限も適用する** | 除外ルールだけでなく、**容量の契約も import に効かせます** (同 §5.4)。移行元の上限がこちらより緩いことがあるので、ここを見ないと import が保持契約を破ります。書き込む前に順に見ます: ① 1 アイテム 50MB 超は落とす ② **ピン留めの合計 500MB** を超えるぶんは**ピンを外して**取り込む (アイテム自体は残す) ③ 総量 2GB は取り込み後に古いものから削除。①② はどちらも差分レポートに 1 件ずつ理由つきで出し、黙って落としたり黙ってピンを外したりしません |

### 3.3 UI と CLI

- **初回起動ウィザード**: インストール済みの移行元を検出し (`/Applications` と設定ディレクトリの存在)、
  チェックリストを出す。「あとで」を選べる。検出は**ファイルの存在確認のみ**で、移行元アプリを起動しない。
- **CLI** ([`cli.md`](./cli.md) に追加):

```bash
noh migrate detect                        # 検出されたソースを一覧
noh migrate import raycast ./export.json  # dry-run (既定)
noh migrate import raycast ./export.json --apply --on-conflict=rename
noh migrate undo                          # 直前の適用を戻す
noh export --all -o nohrs-backup.zip      # MIF + blob の平文 zip
noh import nohrs-backup.zip --apply
```

GUI とまったく同じ importer を呼びます (GUI 専用の移行ロジックを作らない)。

---

## 4. Raycast 拡張の互換

ここが乗り換え容易性の**最大の山**です。Raycast の資産の大半はストアの拡張にあります。

### 4.1 方式の比較

Raycast 拡張は「React + TypeScript を Node で実行し、独自の reconciler が React ツリーを **JSON の render tree** に
変換して本体に送り、本体がネイティブ UI を構築、更新は JSON Patch で差分適用」というモデルです
(出典: [How the Raycast API and extensions work](https://www.raycast.com/blog/how-raycast-api-extensions-work))。
Tinycast はこれを JavaScriptCore + SwiftUI で再現しています。

| 案 | 内容 | 長所 | 短所 |
|----|------|------|------|
| **A. 実行時互換** | JS エンジン (V8 / QuickJS) と Node 互換層をアプリに同梱し、配布済みの拡張をそのまま実行 | 「入れるだけで動く」 | ① サンドボックス原則が壊れる (任意の Node API が動く) ② `deno_core` は tokio を引き込み [ADR 0004](./adr/0004-remove-tokio.md) と衝突 ③ バイナリサイズと常駐メモリが跳ねる ④ Node 互換層の維持コストが永続 |
| **B. ソース互換 (推奨)** | `@raycast/api` と同じ形の shim を nohrs 向けに実装し、拡張のソースを `jco componentize` で WASM component に再ビルド | ① WASM サンドボックスと権限モデルに素直に乗る ([ADR 0005](./adr/0005-wit-bindgen-component-model.md)) ② tokio を持ち込まない ③ nohrs の TS プラグインテンプレ ([`plugin-templates.md`](./plugin-templates.md)) と同じ経路 | ① ワンクリックでは入らない (ビルドが要る) ② Node 固有 API を使う拡張は動かない |
| **C. 非対応** | 互換を作らない | 実装ゼロ | 乗り換え障壁が最大 |

**採用: B**。ただし「ビルドが要る」を **`noh plugin import-raycast <repo>` の 1 コマンド**に隠します
(内部で 依存取得 → shim 差し替え → `jco componentize` → インストール)。

#### ビルド自体をサンドボックスの穴にしない

この経路には、成果物が WASM であることでは塞げない穴があります。**ビルドはホスト上で走る**ので、
他人のリポジトリの `package.json` にある `preinstall` / `postinstall` が、権限モデルを一切通らずに
ユーザーの権限で実行されます。「サンドボックスされたプラグイン」を謳いながら、導入の過程が
任意コード実行では意味がありません。

| 規則 | 内容 |
|------|------|
| ライフサイクルスクリプトを既定で走らせない | 依存取得は `npm ci --ignore-scripts` (lockfile が無ければ `npm install --ignore-scripts`)。これで大半の拡張は通る |
| スクリプトが要る場合は明示オプトイン | `--allow-build-scripts` を付けたときだけ。**何が走るのかを実行前に列挙して見せ**、確認を取る |
| 可能なら隔離して走らせる | macOS は `sandbox-exec`、Linux は `bwrap` / コンテナ。無い環境では上の 2 つに倒し、隔離できていないことを明示する |
| ネットワークはレジストリのみ | 依存取得以外の外向き通信を許さない |
| **隔離の範囲はビルド全体** | 危ないのはライフサイクルスクリプトだけではありません。`jco componentize` も、その loader も、bundler の設定ファイルも、取得した依存そのものも、**ホスト上で実行されるコード**です。隔離できる環境では**取得からインストール直前までを丸ごと**サンドボックス内で行い、隔離できない環境ではその事実を提示したうえで続行の確認を取ります |
| 取り込み元を記録する | 由来 (repo / commit) と、スクリプトを許可したかどうかを `plugin.toml` に残す |

同じ注意は [`plugin-templates.md`](./plugin-templates.md) の `nohrs plugin build` にも本来必要です
(こちらは作者が自分のコードをビルドする前提なので危険度は下がりますが、ゼロではない)。

> **ライセンス上の注意**: 拡張のソースは各作者のライセンスに従います。互換 SDK は**ユーザーが自分の手元で
> ビルドする**ためのものであり、nohrs が拡張を再配布することはしません。Raycast Store からの一括取得も行いません。

### 4.2 `@raycast/api` → nohrs の対応

| Raycast API | nohrs の受け皿 | 備考 |
|-------------|---------------|------|
| `List` / `Grid` / `Detail` / `Form` | `view-node` の `list` / `grid`(要追加) / `detail` / `form` | [`plugin-api.md`](./plugin-api.md) §4 とほぼ同型。`grid` を足す |
| `ActionPanel` / `Action.*` | `list-item.actions` / `action` | `Action.CopyToClipboard` 等の定番は host 側の組み込みアクション id に写像 |
| `showToast` / `showHUD` | `notification` interface | レート制限あり |
| `Clipboard.*` | `clipboard` interface | 権限 `clipboard` |
| `LocalStorage` / `Cache` | `kv` / `cache` interface | そのまま |
| `getPreferenceValues` | `command-context` に `preferences` を追加 | manifest の `preferences` を nohrs の plugin.toml に変換 |
| `environment` | `command-context` を拡張 | `supportPath` は plugin KV の作業領域に写像 |
| `open` / `getSelectedFinderItems` | `explorer` interface | `getSelectedFinderItems` → `selected-paths` |
| `useNavigation` (push/pop) | `launcher.push-view` / `launcher.pop` | 既存 |
| `AI.ask` | 未定義 | AI は BYOK なので、権限つきの `ai` interface を P5 で検討 |
| `OAuth` | 未定義 | ブラウザ起動 + ローカル待受が要る。P5 で検討 |
| `fetch` / `node:https` | `network` interface | ドメイン allowlist の対象 |
| `child_process` | `process` interface | コマンド名 allowlist、`sh -c` 禁止 |
| `node:fs` | `fs` interface | `read_paths` / `write_paths` |
| `menu-bar` モードのコマンド | 未定義 | notch / メニューバーに出す先が要る。P5 |

**動かない拡張の扱い**: shim が未実装の API を呼んだら、`Unsupported(api_name)` のエラーを**ビルド時に**出す
(実行時に静かに `undefined` を返さない)。`noh plugin import-raycast` は「この拡張は `OAuth` を使うため現時点では
移行できません」と言い切る。

### 4.3 WIT の拡張 — セッションとイベント

現在の `commands` interface は `run-command(...) -> command-result` の **1 往復**です
([`plugin-api.md`](./plugin-api.md) §3.1)。React ベースの拡張は「入力のたびに再描画」「アクションのコールバック」
「フォーム送信」を必要とするため、この形では受け止められません。**Raycast 互換以前に、対話的なプラグイン UI
一般に必要な拡張**です。

```wit
interface commands {
  // 既存の command-info / arg-value / command-context はそのまま

  type session-id = u64;

  variant ui-event {
    search-changed(string),
    item-selected(string),                        // item id
    action-invoked(tuple<string, string>),        // (item id, action id)
    form-submitted(list<tuple<string, string>>),  // field id → value
    resync,                                       // host: 差分を当てられなかった。全体を送り直せ
    dismissed,
  }

  variant command-result {
    instant(option<string>),
    view(tuple<session-id, view-node>),   // セッションを開いてビューを返す (revision = 0)
    failure(string),
  }

  // host → plugin: セッションにイベントを届け、新しいビューを受け取る。
  // revision は「このイベントを起こした時点で host が表示しているビュー」の版。
  handle-event: func(session: session-id, revision: u64, event: ui-event)
      -> result<view-update, string>;

  // host → plugin: セッション終了 (launcher が閉じた / pop された)
  close-session: func(session: session-id);

  // 差分だけ返せるようにしておく (全置換も可)
  variant view-update {
    replace(view-node),      // 絶対。開いているセッションになら、revision を問わず適用される
    patch(patch-batch),      // 相対。base が一致するときだけ適用される
    close,
  }

  record patch-batch {
    // どの版のビューを土台に計算したか。handle-event で渡された revision をそのまま返す。
    base: u64,
    ops:  list<view-patch>,
  }

  // 差分の最小形。効くのは「大きなリストの一部だけが変わる」場合だけなので、
  // 対象は list-item に絞る。detail / form は replace で十分。
  record view-patch {
    // 変更対象。list-item の id、または append 時は section-info の id。
    target: string,
    op:     patch-op,
  }

  variant patch-op {
    set-item(list-item),      // target の item を差し替える
    remove-item,              // target の item を消す
    append-item(list-item),   // target のセクション末尾に足す
  }
}
```

- **`patch` は最初は使わなくてよい**。P4 の host は `replace` だけを実装し、`patch` を受け取ったら
  現在のビューに適用して `replace` 相当に畳む — plugin 側から見た意味は同じで、host の最適化は後から入れられます。
  それでも**型として最初から置く**のは、WIT の variant にケースを足すのが破壊的変更だからです
  (Raycast が JSON Patch を使っているのと同じ理由で、大きなリストの再送は最終的に避けたい)。
- **1 セッションにつき、`handle-event` の実行は同時に 1 つまで**です。応答待ちの間に届いたイベントは
  キューに積み、`search-changed` の連続は**最後の 1 つに畳んで**から渡します (打鍵ごとに 1 往復させない、という
  実利も兼ねます)。host は `cx.background_spawn` から plugin を呼ぶので ([`plugin-api.md`](./plugin-api.md) §5)、
  直列化を明示的に書いておかないと、2 つのイベントの応答が入れ替わって返る余地が残ります。
- **差分は 1 バッチが全か無か**で、適用条件は 2 つあります。`base` が host の現在の revision と一致すること、
  そして `target` がすべて現在のビューに存在すること。どちらか一方でも欠ければ、そのバッチは
  **丸ごと捨てて前のビューを保ち**、host は当該セッションに `ui-event::resync` を送ります。
  plugin はそれに `replace` で答える契約です (`resync` に対して再び `patch` を返したら、host は
  プロトコル違反としてセッションを閉じる — 直らないループを回すよりエラーとして見えるほうがよい)。
  適用された更新は `replace` / `patch` のどちらでも revision を 1 つ進めます (捨てられたバッチは進めない)。

  **`target` の存在確認だけでは足りません。** 直列化が (将来の host 側の変更やバグで) 破れたとき、古いビューを
  土台に計算された差分が新しいビューに当たることがあり、その差分が触る id がたまたますべて残っていれば、
  検査を素通りして間違った内容が適用されます。`base` は「この差分はどの版に対するものか」を明示するので、
  id の生き死にに依存せずに検出できます。直列化が効いていれば `base` の不一致は起きないので、これは
  規約が破れたことを**静かに壊れる前に**知らせるための検査です。

  当てられなかった差分を**黙って捨てるのも誤り**です。差分は「host の現在のビュー」を土台に計算されるので、
  1 回取りこぼした時点で plugin が信じているビューと host のビューがずれ、以降の差分はすべて誤った土台の上に
  乗ります。画面には消えたはずの行や、もう無いアクションが残り続け、しかも plugin はそれを知りません。
  部分適用も同じ理由で禁止で、どこまで当たったかが plugin から見えない状態を作ります。
- **閉じたセッションには何も適用しません。** host が閉じると決めた時点 (`Esc` / pop / launcher クローズ /
  plugin 自身の `view-update::close`) でそのセッションを closed にし、以降に返ってきた応答は `replace` であっても
  捨てます。`replace` が「絶対」なのは **revision に対して**であって、セッションの生死に対してではありません。
  これを書かないと、閉じた直後に遅れて返った応答が、もう無い画面を描き直します。
- **`session-id` はプロセス内で再利用しません** (単調増加)。再利用すると、遅れて返った応答が**別のセッション**に
  当たり、しかも型の上では正しく見えるので検出できません。
- セッションはプラグイン側の状態 (React の state) の寿命を定義します。`close-session` で確実に解放する。
  ただし呼ぶのは **in-flight の `handle-event` が返る (か下記のタイムアウトで打ち切られる) のを待ってから**で、
  実行中の呼び出しの足元で状態を解放させない。
- host 側のタイムアウト: `handle-event` が **200ms** を超えたら UI に loading を出し、**5 秒**で打ち切る。
  打ち切りの順序を決めておきます: host はまずセッションを closed にして**以降の結果を捨てる**状態にし、
  `close-session` を呼ぶのは **in-flight の呼び出しが実際に停止し終えてから**です。

  **「待つ」だけでは終わらない場合があります。** guest が無限ループに入っていれば `handle-event` は永遠に
  返らず、`close-session` も永遠に呼べません。P4 に協調的キャンセルが無い ([`plugin-api.md`](./plugin-api.md) §5)
  ことは、**中断できない**ことを意味しません。5 秒は wasmtime の **epoch interruption** (または fuel) の
  期限として設定し、期限が来たら guest は trap で戻ります。tokio runtime や `Instance` を drop するのは
  停止ではない — drop は「もう見ない」であって「止まった」ではないので、**trap が戻るのを待って**から
  セッションを解放します。guest ではなく host import の中でブロックしている場合は epoch では抜けないため、
  host import 側にも個別のタイムアウトが要ります (`network` の HTTP、`process` の spawn が該当)。

  **`close-session` 自身にも同じ 5 秒の期限をかけます。** そこで無限ループに入られると、
  `handle-event` を打ち切ったのに片付けで止まる、という同じ袋小路になります。
  期限内に返るか trap するまでセッションを解放せず、trap した場合は**インスタンスごと落として**
  (そのプラグインの他のセッションも道連れになるので、その旨をユーザーに出して) 解放し、
  **既存の trap 契約どおり 24 時間 auto-disable します** (`auto_disabled_until`、
  [`plugin-overview.md`](./plugin-overview.md) §2)。ここを通さないと、次に開いた瞬間に同じ plugin が
  また同じ場所で固まります。
  `close-session` が **host import の中で**ブロックしている場合は epoch では抜けないので、
  `handle-event` と同じく host import 側のタイムアウトが要ります (`network` の HTTP、`process` の spawn)。
  **片付け経路にもそれが要る**というのがここの要点で、それが無い間は「5 秒で必ず解放する」とは
  言えません — 言えるのは「guest のループなら 5 秒で抜ける」までです。

この拡張は P4 (plugin host) で入れる必要があります。P5 まで遅らせると、Raycast 互換のために WIT を
破壊的変更することになります (ROADMAP のバージョニング方針では破壊的変更はフェーズ完了時に集約するため)。

### 4.4 移行しやすさのための SDK パッケージ

| パッケージ | 役割 |
|-----------|------|
| `@nohrs/api` | nohrs の TS プラグイン向け公式 API ([`plugin-templates.md`](./plugin-templates.md)) |
| `@nohrs/raycast-compat` | `@raycast/api` と同じ形の named export を持ち、内部で `@nohrs/api` を呼ぶ shim。未対応 API はビルド時エラー |
| `@nohrs/mcp-plugin-dev` | 既存の MCP server。`nohrs_raycast_api_map(name)` を足し、AI エージェントが移植を手伝えるようにする |

移植手順は `package.json` の 1 行置換 (`"@raycast/api": "npm:@nohrs/raycast-compat"`) + `jco componentize` に収める。

### 4.5 Alfred ワークフロー

ワークフローは「ノードグラフ + シェルスクリプト + plist」で、Alfred 固有の実行モデルに強く依存します。
**変換は非対応**とし、スニペット (`.alfredsnippets`) のみ移行します。

---

## 5. クリップボード履歴の移行

- 各アプリのローカル保存形式は非公開のものが多いため (R6)、**MIF の `clips` を入口**にします。
- nohrs 本体に同梱する importer は、**形式が判明しているもの**に限ります。それ以外は
  `noh migrate import --format mif <file>` で受け、変換スクリプトはコミュニティに委ねます
  (web の `/docs/migration` に投稿先を用意)。
- 画像・ファイルは blob として同梱。1 アイテム 50MB / 総量 2GB の上限は移行時にも適用し、超過分は
  `unsupported` に落として報告します。

---

## 6. キーマッププリセット

データが移っても**指が覚えているキー**が違うと「乗り換えられた」ことになりません。
プリセットを `config.toml` の `[keybindings] preset = "raycast"` で選べるようにします。

| プリセット | summon | アクションパネル | 特徴 |
|-----------|--------|----------------|------|
| `nohrs` (既定) | `Cmd+Shift+Space` | `Cmd+K` | [`launcher.md`](./launcher.md) §2 |
| `raycast` | `Cmd+Space` (Spotlight を無効化する案内を出す) | `Cmd+K` | フォールバック・`Cmd+Enter` の割当も合わせる |
| `tinycast` | `Opt+Space` | `Cmd+K` | `Tab` でモード循環、`Cmd+1-0` でお気に入り |
| `alfred` | `Opt+Space` | `Cmd+K` | |
| `spotlight` | `Cmd+Space` | — | 最小。ファイル検索中心 |

プリセットは**出発点**であり、個別キーの上書きはユーザー定義が常に勝ちます。
`Cmd+Space` を要求するプリセットは、macOS の Spotlight ショートカットを無効化する手順を**案内するだけ**にし、
システム設定を勝手に書き換えません。

---

## 7. 出ていく側 (anti lock-in)

| データ | 置き場所 | 形式 |
|--------|---------|------|
| 設定 | `~/.config/nohrs/config.toml` | TOML (JSON Schema つき) |
| スニペット | `$XDG_DATA_HOME/nohrs/snippets/*.md` | Markdown + フロントマター |
| メモ | `$XDG_DATA_HOME/nohrs/notes/*.md` | Markdown |
| クリップボード履歴 | SQLite + blob ディレクトリ | 標準 SQLite。`noh export` で JSON 化 |
| 履歴 / 使用統計 | SQLite | 同上 |
| ウィンドウ位置・タブ・シェルフ | redb | `noh export` で JSON 化 |
| プラグイン | `$XDG_DATA_HOME/nohrs/plugins/<id>/` | WASM component + `plugin.toml` |

**約束**: 独自バイナリ形式に閉じ込めない。`noh export --all` の出力だけで、別のマシン・別のアプリに移れる。
この約束は [`persistence.md`](./persistence.md) §7 (バックアップ・移行) の実装で守ります。

---

## 8. 受け入れ基準

| # | 基準 |
|---|------|
| A1 | Raycast の snippets / quicklinks JSON を取り込むと、件数が一致し、プレースホルダが nohrs の記法に変換されている |
| A2 | 取り込みは dry-run が既定で、`--apply` なしでは 1 バイトも書き込まない (テストで検証) |
| A3 | 差分レポートに「未対応」が 1 件以上あるケースで、理由が人間に読める文で出る |
| A4 | `noh migrate undo` で、適用直前の状態に完全に戻る |
| A5 | 移行したクリップに、除外ルールに該当するものが 1 件も含まれない |
| A6 | `@nohrs/raycast-compat` を使い、**外部 API を叩かない**サンプル Raycast 拡張 (List + ActionPanel + Form) が動く |
| A7 | 未対応 API を使う拡張は、ビルド時に API 名つきで失敗する |
| A8 | `noh export --all` → 新規環境で `noh import` → 設定・スニペット・履歴・シェルフが一致する |

---

## 9. フェーズ割当

| Phase | 範囲 |
|-------|------|
| **P3.5** | MIF 定義 + JSON Schema / Raycast の非暗号化 JSON (snippets, quicklinks) / キーマッププリセット / `noh export`・`import` / 初回ウィザードの骨格 |
| **P4** | `.rayconfig` (解析できた範囲) / Tinycast / Alfred snippets / クリップボード履歴 importer / WIT のセッション・イベント拡張 (§4.3) |
| **P5** | `@nohrs/raycast-compat` / `noh plugin import-raycast` / `nohrs_raycast_api_map` / OAuth・AI interface の検討 |

---

## 10. 未解決 / 要調査

**`R<n>` の番号は 3 文書 ([`launcher-requirements.md`](./launcher-requirements.md)・
[`notch.md`](./notch.md)・本書) を通した 1 本の列です。** 新しく足すときは 3 文書の最大値の次を取ります。


| # | 内容 | 影響 |
|---|------|------|
| R5 | `.rayconfig` の暗号化・シリアライズ形式 | 移行できる範囲 (§2)。解析できなければ「公式 JSON エクスポートの範囲まで」と明示する |
| R6 | Supaste / Maccy / Paste / Pastebot のローカル保存形式 | クリップボード移行 (§5) |
| R11 | Tinycast の backup ファイル形式 | §2 |
| R12 | `jco componentize` (StarlingMonkey) で React の reconciler が実用速度で動くか | §4 の前提そのもの。**先に小さな PoC を打つべき** |
| R13 | Raycast 拡張の上位 50 個が使っている API の分布 | shim の実装順を決める材料 |
| R14 | Raycast 拡張ソースのライセンス分布 (再ビルド配布の可否) | §4.1 の注意書きの精度 |
