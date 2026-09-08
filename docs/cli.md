# CLI — `noh`

> Status: Draft (P2 で `rm` を実装、以降コマンドを追加)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`explorer-essentials.md`](./explorer-essentials.md), [`architecture.md`](./architecture.md), [`logging.md`](./logging.md)

`noh` (crate としては `nohrs-cli`) は nohrs の**ターミナル側の入口**です。GUI (`nohrs` バイナリ) と同じファイル操作
(`nohrs-services::fs::ops`) をシェルから使えるようにするもので、gpui に依存しないため
`default-members` に含まれ Linux CI でもビルド・テストされます。

---

## 1. `noh rm`

標準の `rm(1)` の**手前に置く**ことを目的とした削除コマンドです。既定では対象を OS のゴミ箱へ
移動し、`--permanent` を付けたときだけ本当に削除します。GUI 側の Delete / Delete Permanently
([`explorer-essentials.md` §1.1](./explorer-essentials.md)) と同じ実装を共有します。

### 1.1 インストール (標準 `rm` の手前に置く)

`PATH` 上で `/bin/rm` より先に来る場所へ `rm` という名前の symlink を張ります。

```sh
cargo build --release -p nohrs-cli
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/noh" ~/.local/bin/rm
# ~/.local/bin が /bin より先に来ていること
export PATH="$HOME/.local/bin:$PATH"
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

### 1.4 実装メモ

- 破壊的操作は `Backend` trait 越しに行います。ヘッドレス CI にはデスクトップのゴミ箱が無く、
  実際の `trash::delete` を呼べないため、テストは記録用のフェイク実装で「何を消そうとしたか」を検証します。
- オペランドは削除前に絶対パスへ解決します (`std::path::absolute`)。ゴミ箱はプロセスの作業ディレクトリを
  前提にできないため。symlink は解決しません。

---

## 2. `noh log`

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
