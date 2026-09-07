# Logging and measurement

> Status: Draft (P2 で `noh log` を実装、`noh perf` は後続)
> Related: [`cli.md`](./cli.md), [`persistence.md`](./persistence.md), [`config.md`](./config.md)

nohrs が「何をしたか」「どれだけ時間がかかったか」を記録し、読み返すための仕組みです。

---

## 1. 2 つのシンク

| シンク | 形式 | 読み手 | span の記録 |
|--------|------|--------|-------------|
| **stderr** | 人間可読 | ターミナルの開発者、CLI のエラーを見るユーザー | しない |
| **ローリングファイル** | **JSON Lines** | `noh log`、後続の `noh perf` | **する** |

ファイルシンクが必要な理由は単純で、**GUI には stderr の行き先が無い**からです。ウィンドウを閉じればその
セッションのログは消えます。ファイルに残して初めて「さっき何が起きたか」を後から聞けます。

出力先は `$XDG_STATE_HOME/nohrs/logs/nohrs.log.<日付>` です。`state` に置くのは、ログが
**再起動をまたいで残ってほしいが、失っても困らず、バックアップもしたくない**ものだからです。
`data_dir` は DB と並ぶバックアップ対象、`cache_dir` は掃除ツールにいつ消されてもおかしくない場所で、
どちらも適しません。日次ローテーションで既定 7 世代を保持します。

---

## 2. 計測は span の終了レコードで行う

**これが「全操作に適用できる」根拠です。** `tracing_subscriber` のファイル層を
`FmtSpan::CLOSE` で設定してあるので、`#[tracing::instrument]` の付いた関数は**終了時に 1 レコード**を
書きます。そこに `time.busy`（実際に走っていた時間）が入ります。

したがって新しい操作を計測対象にするコストは、属性を 1 つ足すことだけです。カウンタもレジストリも
クレートごとの配線も要りません。

```rust
#[tracing::instrument(
    target = "nohrs::op",          // 計測対象が共有する唯一のターゲット
    name = "fs.move",              // 集計キー。<領域>.<動詞>
    level = "debug",
    skip_all,                      // 引数を丸ごと吐かない
    fields(src = %src.display(), dst = %dst.display())
)]
pub fn move_path(src: &Path, dst: &Path) -> Result<MoveKind> { … }
```

**規約が 3 つあります。**

- **`target = "nohrs::op"`** — クレートを問わず全ての計測操作が共有します。1 つのフィルタ指令で
  アプリ全体の計測が入り切りでき、`noh perf` が選択する対象も 1 つで済みます
- **`name = "<領域>.<動詞>"`** — `fs.move` / `search.query` / `index.build_home`。これが集計キーなので、
  安定していることが重要です
- **`level = "debug"`** — `info` に置くとディレクトリを開くたびターミナルが溢れます。ファイル側の
  既定フィルタは `info,nohrs::op=debug` なので、**stderr は静かなまま、ファイルには全操作が残ります**

`crates/nohrs-core/tests/log_file_records_operations.rs` がこの契約（名前・フィールド・`time.busy` が
既定設定でファイルに届くこと）を端から端まで固定しています。フォーマッタの設定は気付かずに変えられる
ので、設定そのものではなく結果を検証しています。

### 現在計測している操作

| 名前 | 場所 |
|------|------|
| `fs.copy` / `fs.move` / `fs.rename` / `fs.create_dir` / `fs.trash` / `fs.delete` | `nohrs-services::fs::ops` |
| `fs.list_dir` | `nohrs-services::fs::listing` |
| `search.query` | `nohrs-services::search::engine` |
| `index.build_home` / `index.remove` / `index.process_changes` | `nohrs-services::search::indexer` |

ストア層は別に、SQL 文そのものと遅いクエリの閾値検出を持っています
（`StoreLogConfig`、[`persistence.md`](./persistence.md) §5）。こちらはターゲットが
`nohrs_store::sql` / `nohrs_store::redb` で、`time.busy` ではなく SQL 本文が要るため別立てです。

---

## 3. `noh log`

```sh
noh log show                 # 直近 50 件
noh log show -n 200 --ops    # 操作だけ、200 件
noh log show --json          # 生の JSON Lines（jq に流す用）
noh log path                 # ログの場所
noh log clear --force        # 消す
```

`--ops` は span 終了レコードだけを残すので、実質「何がどれだけかかったか」の一覧になります。

```
14:22:47  fs.trash                      696µs path=/work/notes.txt
14:22:49  search.query                 12.2ms query=report scope=Home
```

CLI と GUI は**同じファイル**に追記します。`noh rm` で消したものが `noh log` に出るのはそのためです。
POSIX の `O_APPEND` は 1 行程度の書き込みでは原子的なので、両方が同時に動いても行は混ざりません。

---

## 4. 既知の限界

- **ファイルシンクの設定は既定値のみ。** `FileLogConfig` は有効・無効、フィルタ、保持世代を持ちますが、
  現状バイナリは `FileLogConfig::default()` を渡しています。設定を読んでから購読者を入れると、
  **設定の読み込み自体の失敗が記録できなくなる**ためで、この順序の解きほぐし（`reload::Layer` など）と
  `config.toml` への露出は `noh perf` と同じ後続作業に置いています
- **`time.busy` は文字列。** `"12.2ms"` / `"1.5s"` / `"696µs"` と整形済みで、数値ではありません。
  集計する側が単位を解釈する必要があります
- **集計が無い。** 生のレコードが読めるだけで、「合計時間の多い順」は `noh perf` の仕事です
- **パスがそのまま残る。** ローカルのファイルマネージャとして意図的な設計ですが、ログを共有するときは
  中身を見てからにしてください
