# CLI — `noh`

> Status: Draft (P2 で `rm` / `restore` / `trash` / `shim` / `doctor` を実装、以降コマンドを追加)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`explorer-essentials.md`](./explorer-essentials.md), [`architecture.md`](./architecture.md), [`logging.md`](./logging.md)

`noh` (crate としては `nohrs-cli`) は nohrs の**ターミナル側の入口**です。GUI (`nohrs` バイナリ) と同じファイル操作
(`nohrs-services::fs`) をシェルから使えるようにするもので、gpui に依存しないため
`default-members` に含まれ Linux CI でもビルド・テストされます。

| コマンド | 役割 |
|---------|------|
| [`noh rm`](#1-noh-rm) | 既定でゴミ箱へ送る削除。標準 `rm` の手前に置ける |
| [`noh restore`](#2-noh-restore) | ゴミ箱から元の場所へ戻す |
| [`noh trash list` / `purge` / `empty`](#3-noh-trash) | ゴミ箱の一覧・個別完全削除・全消去 |
| [`noh shim`](#4-noh-shim) | `rm` を乗っ取る symlink の設置・解除 |
| [`noh doctor`](#5-noh-doctor) | 設置状態と依存物の診断 |
| [`noh completions`](#6-noh-completions) | シェル補完スクリプト |

---

## 1. `noh rm`

標準の `rm(1)` の**手前に置く**ことを目的とした削除コマンドです。既定では対象を OS のゴミ箱へ
移動し、`--permanent` を付けたときだけ本当に削除します。GUI 側の Delete / Delete Permanently
([`explorer-essentials.md` §1.1](./explorer-essentials.md)) と同じ実装を共有します。

### 1.1 インストール (標準 `rm` の手前に置く)

`PATH` 上で `/bin/rm` より先に来る場所へ `rm` という名前の symlink を張ります。手で `ln -sf` する
代わりに [`noh shim install`](#4-noh-shim) を使ってください (既存の実ファイルを絶対に壊さないため)。

```sh
cargo build --release -p nohrs-cli
./target/release/noh shim install rm     # 既定の設置先は ~/.local/bin
export PATH="$HOME/.local/bin:$PATH"     # /bin より先に来ていること
noh doctor                               # 効いているか確認
```

argv[0] が `rm` (Windows では `rm.exe`) のときは全引数を `rm` サブコマンドのものとして解釈するため、
`rm -rf build/` のような既存のコマンドライン・シェルスクリプトがそのまま通り、削除物はゴミ箱に入ります。
symlink を使わない場合は `noh rm ...` と明示的に呼べます。

### 1.2 フラグ

| フラグ | 動作 |
|--------|------|
| `-r`, `-R`, `--recursive` | ディレクトリを中身ごと削除する (これが無いとディレクトリは拒否) |
| `-d`, `--dir` | 空のディレクトリも削除対象にする (中身があれば拒否) |
| `-f`, `--force` | 存在しない対象を無視し、確認もしない。**`--permanent` は含意しない** |
| `-i`, `--interactive` | 対象ごとに確認する (`--force` があるとそちらが優先) |
| `-v`, `--verbose` | 処理した対象を stdout に出力する (`trashed <path>` / `deleted <path>`) |
| `-P`, `--permanent`, `--no-trash` | ゴミ箱を経由せず完全に削除する。**元に戻せない** |
| `--` | 以降を `-` で始まる名前のファイルとして扱う |

`-f` を「完全削除」にしない点が設計の要です。既存スクリプトの `rm -rf` はそのままゴミ箱行きになり、
本当に消したいときだけ `--permanent` を明示します。

### 1.3 安全策と終了コード

- `.` / `..` で終わるオペランド、およびルートディレクトリは拒否します (POSIX `rm` と同じ)。
- symlink はリンク自体を対象にし、リンク先はたどりません (`-r` も不要)。
- 失敗した対象があっても残りの処理は続行し、終了コードは 1 になります。全て成功なら 0。
- オペランドが 1 つも無い場合はエラー (終了コード 1)。ただし `-f` 付きなら POSIX どおり黙って 0。

---

## 2. `noh restore`

ゴミ箱の中身を**元の場所へ戻します**。オペランドを付けなければ**直近に捨てた 1 件**が戻るので、
やってしまった直後は `noh restore` だけで済みます。

```sh
noh rm notes.txt
noh restore                    # 直近の 1 件
noh restore notes.txt          # 名前で指定 (パスでも可)
noh restore --all              # ゴミ箱の中身すべて
noh restore --since 2h --all   # 直近 2 時間に捨てたものすべて
```

| フラグ | 動作 |
|--------|------|
| `--all` | 一致するもの全部。オペランドが無ければゴミ箱全体 |
| `--since <DURATION>` | この期間内に捨てたものだけを対象にする (`45s` `30m` `12h` `7d` `2w`) |
| `-i`, `--interactive` | 1 件ずつ確認する |
| `-f`, `--force` | 一致するものが無くてもエラーにしない |

オペランドは**元のパス**か、区切りを含まない**ファイル名**で一致します
(`noh restore notes.txt` で、どのディレクトリから消したかを打ち直さずに済みます)。
同じ名前が複数ある場合は既定で最新の 1 件を戻し、他にもあることを stderr で知らせます。

### 2.1 戻せない場合

- **元の場所が埋まっている**とき、上書きはせずエラーにします (`something else is at the original
  location; move it aside first`)。退避してから戻してください。判定は事前チェックだけに頼らず、
  移動そのものが `RENAME_NOREPLACE` (Linux) / `RENAME_EXCL` (macOS) 付きの `rename` なので、
  チェックと移動の**間に**現れたファイルも上書きされません。フラグを受け付けない
  ファイルシステムでは copy + delete に落ちますが、そこでも `create_new` /
  `create_dir` / `symlink` で先に名前を確保するので上書きは起きません
  (シンボリックリンクとパーミッションもそのまま引き継ぎます)。
- 元のディレクトリが既に無い場合は**作り直してから**戻します。
- 完全削除 (`noh rm --permanent`、`noh trash purge`) したものは戻せません。

---

## 3. `noh trash`

```sh
noh trash list                      # 新しい順に一覧
noh trash list -l                   # file / dir の別も表示
noh trash list --json               # 1 行 1 オブジェクトの JSON
noh trash list --older-than 30d     # 30 日より前に捨てたものだけ

noh trash purge notes.txt           # 個別に完全削除 (確認あり)
noh trash purge --all -f            # 全部、確認なし
noh trash purge --older-than 30d -f # 古いものだけ掃除
noh trash empty                     # purge --all と同じ
```

一覧の時刻は**相対表記** (`just now`, `3h ago`, `2d ago`) です。ローカルタイムゾーンの解決を
避けるためで、正確な時刻が要るときは `--json` の `deleted_at_unix` を使ってください。

`purge` と `empty` は**元に戻せない**ので、既定で 1 件ずつ確認します。`-f` / `--force` で確認を
省略できます。オペランドも `--all` も `--older-than` も無い `purge` は、事故防止のためエラーです。

なお Linux / Windows では一覧が OS のゴミ箱全体を指すため (§7.2)、`--all` は**他のアプリが
捨てたものも**完全削除します。ゴミ箱を空にするとはそういうことですが、意識しておいてください。

---

## 4. `noh shim`

`noh` を標準コマンドの手前に置く symlink を管理します。手作業の `ln -sf` と違い、**実ファイルは
`--force` を付けても絶対に置き換えません**し、`uninstall` は「自分を指す symlink」であることを
確認してからでないと消しません (`noh shim uninstall rm` が `/bin/rm` を消すことはありません)。

```sh
noh shim install            # noh が代行できるコマンド全部 (今は rm)
noh shim install rm --dir ~/bin
noh shim status             # 何が入っていて、PATH で勝てているか
noh shim uninstall rm
```

| フラグ | 動作 |
|--------|------|
| `--dir <DIR>` | 設置先 (既定: `~/.local/bin`) |
| `-f`, `--force` | 別のプログラムを指す**symlink** なら置き換える (実ファイルは対象外) |

設置後に `PATH` 上でまだ標準コマンドの方が先に見つかる場合は、その旨を stderr で警告します。
Windows は symlink に権限が要るため未対応です (バイナリのあるディレクトリを `PATH` に足してください)。

---

## 5. `noh doctor`

`rm` を乗っ取る以上、設定ミスが**静かに**効かなくなるのが一番まずいので、その確認用です。

```console
$ noh doctor
ok    binary  /Users/me/.local/bin/noh
ok    shim    `rm` runs /Users/me/.local/bin/rm (this binary)
ok    trash   /Users/me/.Trash (no OS trash index here, so restores use the nohrs ledger)
ok    ledger  /Users/me/.local/share/nohrs/db.sqlite (12 items recorded)
ok    config  /Users/me/.config/nohrs/config.toml
```

`warn` は助言 (終了コード 0)、`error` は壊れている状態 (終了コード 1) です。

---

## 6. `noh log`

nohrs が自分について記録したものを読み返します。GUI は stderr の行き先が無いので、CLI と GUI は
共通のローリングファイル (`$XDG_STATE_HOME/nohrs/logs/`) に JSON Lines で追記しており、
このコマンドがその読み手です。詳細は [`logging.md`](./logging.md)。

| コマンド | 動作 |
|---------|------|
| `noh log show [-n N]` | 直近 N 件 (既定 50) を整形して出力 |
| `noh log show --ops` | **完了した操作だけ**を、所要時間つきで出力 |
| `noh log show --json` | 生の JSON Lines をそのまま出力 (`jq` に流す用) |
| `noh log path` | ログディレクトリと現存するファイルを出力 |
| `noh log clear [--force]` | ログを空にする。`--force` 無しでは件数を報告するだけ |

`--ops` の出力は「何がどれだけかかったか」の一覧です。

```
14:22:47  fs.trash                      696µs path=/work/notes.txt
14:22:49  search.query                 12.2ms query=report scope=Home
```

計測対象は `#[tracing::instrument(target = "nohrs::op", …)]` の付いた操作すべてで、
ファイル操作・ディレクトリ一覧・検索・インデックス作成が既に含まれます。新しい操作を
一覧に載せるには属性を 1 つ足すだけです ([`logging.md`](./logging.md) §2)。

ログファイルが 1 つも無い場合 (初回起動直後など) はその旨を報告し、終了コードは 0 です。
`--json` のときだけは何も出しません — `jq` に流す先で散文の 1 行はパースエラーになるためで、
「レコードが無い」の JSON Lines での綴りは空のストリームです。

`noh log` は**ログの読み手なので、自分ではログを書きません**。`clear --force` が GUI の開いている
ファイルを unlink せず truncate する理由と併せて [`logging.md`](./logging.md) §3 に書きました。
記録されるものとその置き場所の権限は同 §1.1 です。

---

## 7. `noh completions`

```sh
noh completions zsh  > ~/.zfunc/_noh
noh completions bash > ~/.local/share/bash-completion/completions/noh
noh completions fish > ~/.config/fish/completions/noh.fish
```

---

## 8. 実装メモ

### 8.1 ゴミ箱台帳 (`nohrs-store` の `trash` テーブル)

「戻す」ためには**どこから消したか**が要りますが、その情報の出どころは OS によって違います。

| | 元パスの記録 | `noh restore` の実装 |
|---|---|---|
| **Linux** | OS が `.trashinfo` に保存 | `trash::os_limited` に委譲 |
| **Windows** | OS が `$I` ファイルに保存 | 同上 |
| **macOS** | Finder の非公開 `.DS_Store` のみ | **nohrs 自身の台帳**を使う |

macOS では `trash` クレートの `os_limited` モジュールがそもそもコンパイルされず、さらに macOS 実装は
`trashItemAtURL_resultingItemURL_error(&url, None)` と呼んでいてゴミ箱内の移動先を捨てています。
そこで nohrs は、**OS のゴミ箱はそのまま使いつつ**、自前の台帳を横に置きます。実体は
`nohrs-store` のメタデータ DB (`$XDG_DATA_HOME/nohrs/db.sqlite`) の `trash` テーブルで、
`nohrs_store::TrashLedger` trait 越しに読み書きします ([`persistence.md`](./persistence.md) §2)。
捨てたもの自体は Finder のゴミ箱に普通に入るので、ユーザーの慣れた復元経路を壊しません。

台帳には移動先ではなく**元のパス・ファイル名・サイズ・mtime・削除時刻**を記録し、
ゴミ箱内の実体は**復元時に**それらで突き合わせます。ゴミ箱が名前を変えて受け入れた場合
(`notes.txt` → `notes 2.txt`) も候補に含めますが、認めるのは**ゴミ箱自身が衝突回避に使う形**
だけです。すなわち拡張子が同じで、ステムが「元のステム + 半角スペース + 次のどちらか」に
なっているものに限ります。

- **連番** — 2 以上の 10 進数字、ゼロ埋めなし (`notes 2.txt`, `notes 10.txt`)。
  ゴミ箱は 2 つ目の到着を `2` と数え、桁を揃えないので、`notes 1.txt` や
  `notes 02.txt` はユーザーが付けた名前です
- **時刻** — `H.MM.SS AM` / `PM`、時は 1〜12、分秒は 2 桁で 60 未満
  (`notes 10.30.15 AM.txt`)。`notes 99.99.99 AM.txt` は形が似ているだけの別物です

単なる前方一致にすると `notes-backup.txt` のような**別のファイル**まで候補に入り、
サイズが同じでゴミ箱に入った時刻が近ければ、それを `notes.txt` として復元 (あるいは
`trash purge` で完全削除) しかねません。スペースまでで止めても `notes backup.txt` が
残るので、接尾辞の形まで見ます。移動はサイズと mtime を保存するので、
名前の条件と合わせればこれで候補を十分に絞れます。

候補が複数残ったときに勝つのは、**名前が一番それらしいもの**ではなく
**その行が書かれた時刻に最も近くゴミ箱へ入ったもの**です (移動は ctime を更新するので、
それが「いつ来たか」の代わりになります)。ゴミ箱は最初に来たものに元の名前を残して後から来たものを
改名するため、名前の一致を優先すると、名前・サイズ・mtime が同一な 2 つのコピーで
**新しい行に古いファイルを割り当ててしまう** — つまり中身を取り違えて復元します。
ctime を持たないプラットフォームでは全候補が同距離になり、従来どおり名前で決まります。

台帳は `fs::ops::trash_path` が書くため、**GUI から捨てたものも `noh restore` で戻せます**
(逆も同じ)。台帳への書き込みに失敗しても削除自体は失敗扱いにせず、`tracing` に警告を出します
(すでに移動は済んでいるため)。

**台帳を書くのは、それを読む側がいるプラットフォームだけ**です。Linux / Windows では OS の索引が
同じ情報を持っていて `OsStore` がそちらから復元するので、読み手のいない二重記録が
際限なく伸びるのを避けています。そちらでは `db.sqlite` を開くことすらしません
(`nohrs_cli::ledger::open_if_needed`)。

行の順序も台帳の仕事です。削除時刻は `trashed_at` (ナノ秒) に持ちますが、同一時刻の
タイブレークは行 id (単調増加) が行うため、`noh restore` の「直近の 1 件」は常に確定します。

### 8.2 プラットフォームによる差

- **一覧の範囲**: Linux / Windows では OS のゴミ箱索引を読むので、**他のアプリが捨てたものも**
  一覧に出ます。macOS では nohrs が捨てたものだけです。
- **同一秒内の順序**: 台帳は行 id を持つので、macOS では同じ瞬間に捨てた複数のアイテムでも
  「直近の 1 件」が確定します。Linux / Windows の削除時刻は OS 側が秒精度でしか持たないため、
  同じ秒に複数捨てた直後の `noh restore` (引数なし) はそのうちの 1 件になります。
  複数消したときは `--all` かパス指定を使ってください。
- **外部ボリューム**: macOS で外部ボリュームから捨てたものは
  `/Volumes/<name>/.Trashes/<uid>` に入るため、まだ追跡していません。
- ゴミ箱から (Finder などで) 消えたアイテムの台帳行は、`noh trash list` が見つけられなかった時点で
  破棄されます。復元も完全削除もできない行を永遠に残さないためです。

### 8.3 テスト

- 破壊的操作は `Store` / `Backend` trait 越しに行います。ヘッドレス CI にはデスクトップのゴミ箱が
  無く実際の `trash::delete` を呼べないため、テストは記録用のフェイク実装や、
  一時ディレクトリを指した `LedgerStore` で検証します。
- `LedgerStore` はゴミ箱ディレクトリと台帳の両方を注入できるので、macOS が実際に通る経路
  (in-memory の `SqliteStore` + 一時ゴミ箱ディレクトリ) を全プラットフォームの CI でテストできます。
- プロセス配線 (実 stdio のロック、実ゴミ箱のオープン、環境変数の読み取り) は
  `nohrs-cli/src/main.rs` に集約し、ライブラリ側は解析とエンジンだけにしてあります。
