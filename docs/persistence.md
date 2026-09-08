# Persistence — SQLite + redb

> Status: Draft (P2 で SQLite + redb ホスト KV を実装、P4 で plugin KV 拡張)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/async-runtime.md`](./async-runtime.md), [`docs/plugin-api.md`](./plugin-api.md)

本書はメタデータ・履歴・プラグイン状態の永続化レイヤを定めます。

---

## 1. 全体方針

| データ種別 | ストア | ファイル | 理由 |
|-----------|--------|---------|------|
| **ファイルメタデータ・履歴** | **SQLite (rusqlite)** | `db.sqlite` | SQL 表現力 (差分 query / 結合 query / 順序付き query) が必要 |
| **ホスト KV** (window 位置・タブ/セッション復元・動的設定) | **redb** | `state.redb` | 純粋な key→blob の高頻度・小サイズ書き込み。SQL 不要、メタデータ書き込みと隔離 |
| **プラグイン専用 KV** (P4) | **redb** | `plugin-kv.redb` | 高速 R/W、plugin_id でテーブル隔離、host data と分離 |
| **設定ファイル** | TOML (`config.toml`) | — | 詳細は [`docs/config.md`](./config.md) |

理由:
- 検索メタデータ (差分検出など) と履歴 (kind+時刻順) は SQL 表現力が必要 → SQLite
- ホスト KV (タブ復元・window 位置等) は純 KV で SQL 不要。高頻度・小サイズの書き込みを、P3 のメタデータインデクサがハンマーする SQLite 単一ライター WAL から隔離するため redb に置く
- プラグイン KV (P4) はシンプルかつ高速性が要求され (Raycast / VSCode 流の plugin state)、host KV と同じ redb 実装 (`RedbKvStore`) を再利用する
- 複数の DB を持つが、それぞれ単一ファイルで backup 単純、`MetadataStore` / `KvStore` の trait は別物なので混乱無し

### 1.1 SQLite と redb の使い分け基準

> **判断基準: 「キー完全一致以外で問い合わせる必要があるか？」**

| 答え | 例 | ストア |
|------|----|--------|
| **Yes** — 範囲 / 差分 / 順序 / 二次インデックスでクエリする | `list_children` / `list_changed_since` / `find_by_inode` / kind+時刻順の履歴 | **SQLite** |
| **No** — 純粋な key→blob の get / put / prefix だけ | タブ/セッション復元、window 位置、ホスト動的 KV、plugin KV | **redb** |

新しい永続データを追加するときは必ずこの基準で配置先を決める。判断に迷う「とりあえず DB」を避け、SQL 表現力を実際に使うものだけを SQLite に集約する。

---

## 2. SQLite (rusqlite)

### 設定

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled", "blob"] }
```

- `bundled` で SQLite 自体を vendoring (システム SQLite に依存しない、docker/nix 安定)
- WAL モード (`PRAGMA journal_mode=WAL`) で single writer + many readers
- `cx.background_spawn` 経由で UI 層から async に見せる
- tokio 依存なし

### スキーマ (P2 時点)

```sql
-- ファイルメタデータ (検索インデックスの状態管理)
CREATE TABLE files (
    id            INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    parent_path   TEXT NOT NULL,
    inode         INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    mtime_ns      INTEGER NOT NULL,
    content_hash  BLOB,                   -- blake3 first-N-KB hash (P3 で利用)
    indexed_at    INTEGER,                -- Tantivy indexing 時刻 (P3)
    deleted_at    INTEGER                 -- 論理削除 (P3 watcher で使用)
);
CREATE INDEX idx_files_parent ON files(parent_path);
CREATE INDEX idx_files_inode  ON files(inode);

-- (ホスト KV は redb `state.redb` に置く。§3 参照。SQLite には持たない)

-- 履歴 (recent files, search history, command usage)
CREATE TABLE history (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,    -- "open" | "search" | "command"
    payload     TEXT NOT NULL,
    occurred_at INTEGER NOT NULL
);
CREATE INDEX idx_history_kind_time ON history(kind, occurred_at DESC);

-- マイグレーション管理
CREATE TABLE _migrations (
    version    INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);
```

### スキーマ (P4 追加)

```sql
-- プラグイン状態
CREATE TABLE plugins (
    id                   TEXT PRIMARY KEY,    -- "user/repo" or "core/<name>"
    version              TEXT NOT NULL,
    enabled              INTEGER NOT NULL,
    installed_at         INTEGER NOT NULL,
    manifest             TEXT NOT NULL,       -- TOML
    granted_permissions  TEXT NOT NULL,       -- JSON
    auto_disabled_until  INTEGER              -- 異常終了で自動 disable
);
```

(plugin の KV データは redb に置く。SQLite には manifest と permission のみ)

### マイグレーション

| 案 | 採用 |
|----|------|
| 自前 (`migrations/<version>.sql`)、`_migrations` テーブルで version 管理 | ✅ |

```rust
// 擬似コード
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/001_init.sql")),
    (2, include_str!("../migrations/002_add_plugins.sql")),
];

fn migrate(conn: &Connection) -> Result<()> {
    let applied: Vec<u32> = conn.prepare("SELECT version FROM _migrations")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for (ver, sql) in MIGRATIONS {
        if !applied.contains(ver) {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO _migrations (version, applied_at) VALUES (?, ?)",
                (ver, now_ns()),
            )?;
        }
    }
    Ok(())
}
```

ロールバックは forward-only (現代の運用慣行)。`refinery` 等の外部 crate は不要。

---

## 3. redb (ホスト KV + plugin KV)

### 設定

```toml
[dependencies]
redb = "2"
```

- ACID + MVCC、SQLite と同じく WAL 風 crash recovery
- ホスト KV (P2) と plugin KV (P4) は **別ファイル** に分ける

| ファイル | 用途 | フェーズ |
|---------|------|---------|
| `$XDG_DATA_HOME/nohrs/state.redb` | ホスト KV (window 位置・タブ/セッション復元・動的設定) | **P2** |
| `$XDG_DATA_HOME/nohrs/plugin-kv.redb` | プラグイン専用 KV / cache | P4 |

### ホスト KV テーブル設計 (P2)

```rust
// crates/nohrs-store/src/nohrs_store.rs (擬似コード)
use redb::TableDefinition;

// 単一テーブル。key は `KvKey` (= "window.position" / "session.tabs")。
const HOST_KV: TableDefinition<'static, &str, &[u8]> = TableDefinition::new("kv");
```

- `KvStore::get` / `put` / `delete` は `HOST_KV` への単純な点アクセス
- `KvStore::list_namespace(ns)` は `range("<ns>.".. )` を走査し prefix 不一致で打ち切る
- `KvStore::batch(ops)` は 1 つの write transaction にまとめて atomic commit
- value は JSON or MessagePack で serialize した blob (タブ群のスナップショット等)

#### キーの名前空間は型で強制する

key は `&str` ではなく **`KvKey`** です。`<namespace>.<name>` のドット区切りで、セグメントは
1 つ以上・小文字 ASCII / 数字 / `_`。

名前空間は「文字列の慣習」だった時期があり、それだと `put("tabs", …)` が普通にコンパイルされ、
書けて読み戻せてしまいます。壊れるのは後から別の場所で、`list_*` が行を取りこぼす形です。
そこで:

| 作り方 | 検査 | 用途 |
|--------|------|------|
| `KvKey::from_static("session.explorer_tabs")` | **コンパイル時** (`const fn`) | サブシステムが持つ固定キー。ほぼ全部これ |
| `KvKey::new(ns, name)` / `KvKey::parse(s)` | 実行時 (`Result`) | 実行時に組み立てるキー |

`from_static` は `const fn` なので、`KvKey::from_static("tabs")` は実行時エラーではなく
**ビルドエラー**になります。これが「規約」を規約以上のものにしている部分です。

`list_namespace` が prefix ではなく名前空間を取るのも同じ理由です。自由な prefix だと
`list_prefix("sess")` が `session.*` にたまたま一致し、`list_prefix("window")` は
`window_backup.*` まで拾います。末尾のドットを内部で足すことで、走査は名前空間の中で閉じます。

> **書き込み頻度に関する注意**: redb の commit はデフォルトで durable (fsync) なので、window ドラッグ等の高頻度更新を 1 操作ずつ `put` すると fsync が多発する。呼び出し側 (UI 層) で **debounce してから書く**、複数キーは `batch` でまとめる、を原則とする。

### プラグイン KV テーブル設計 (P4)

`plugin-kv.redb` に plugin_id ごとの隔離テーブルを置く (本 Issue #63 では対象外、P4 で実装)。

```rust
// 擬似コード
use redb::TableDefinition;

// plugin_id ごとに別 table。命名: "plugin_kv__<plugin_id>"
fn table_for(plugin_id: &str) -> TableDefinition<'static, &str, &[u8]> {
    TableDefinition::new(format!("plugin_kv__{}", plugin_id).leak())
}
```

これにより:
- 1 プラグインが他プラグインのデータを誤って読むことが構造的に不可能 (defense in depth)
- `iter` で全 key を列挙しても自分の plugin_id 配下のみ

### キャッシュ用テーブル (TTL)

```rust
// 各 plugin_id ごとに "plugin_cache__<plugin_id>" もう一つの table
// value は (expires_at_ns, payload) のタプル
fn cache_for(plugin_id: &str) -> TableDefinition<'static, &str, (i64, &[u8])>;
```

`get(key)` 時に `expires_at_ns < now_ns` なら expired として返す。

### 制約

| 観点 | 値 |
|------|-----|
| 1 値あたりのサイズ上限 | **1 MB** (超過時はエラー、`StorageError::TooLarge`) |
| バッチ操作 | `batch(ops: Vec<KvOp>)` で 1 トランザクション、atomic commit |
| TTL の resolution | 秒単位で十分 (cache 用途) |
| プラグインの永続データ削除 | uninstall 時にデフォルトは保持、`uninstall --purge` で削除 |

---

## 4. Trait 設計 (Interface Segregation)

```rust
// crates/nohrs-store/src/nohrs_store.rs

// このスケッチ内だけの略記。実装では値はそのまま `Vec<u8>` です。
type Bytes = Vec<u8>;

pub trait MetadataQuery: Send + Sync {
    fn get_file(&self, path: &Path) -> Result<Option<FileRecord>>;
    fn list_children(&self, parent: &Path) -> Result<Vec<FileRecord>>;
    fn list_changed_since(&self, ts_ns: i64) -> Result<Vec<FileRecord>>;
    fn find_by_inode(&self, inode: u64) -> Result<Option<FileRecord>>;
}

pub trait MetadataStore: MetadataQuery {
    fn upsert_file(&self, entry: &FileEntry) -> Result<FileId>;
    fn delete_file(&self, path: &Path) -> Result<()>;
    fn mark_indexed(&self, id: FileId, indexed_at_ns: i64) -> Result<()>;
}

pub trait KvStore: Send + Sync {
    fn get(&self, key: &KvKey) -> Result<Option<Bytes>>;
    fn put(&self, key: &KvKey, value: &[u8]) -> Result<()>;
    fn delete(&self, key: &KvKey) -> Result<()>;
    fn list_namespace(&self, namespace: &str) -> Result<Vec<(KvKey, Bytes)>>;
    fn batch(&self, ops: Vec<KvOp>) -> Result<()>;
}

pub trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Bytes>>;
    fn put_with_ttl(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub trait HistoryStore: Send + Sync {
    fn record(&self, entry: HistoryEntry) -> Result<()>;
    fn list(&self, kind: HistoryKind, limit: usize) -> Result<Vec<HistoryEntry>>;
}

// (P4) plugin 関連
pub trait PluginStore: Send + Sync {
    fn register(&self, manifest: &PluginManifest) -> Result<()>;
    fn list(&self) -> Result<Vec<PluginRecord>>;
    fn kv_for(&self, plugin_id: &str) -> Box<dyn KvStore>;
    fn cache_for(&self, plugin_id: &str) -> Box<dyn Cache>;
}
```

実装:

```rust
pub struct SqliteStore { conn: Arc<Mutex<Connection>> }
impl MetadataQuery  for SqliteStore { ... }
impl MetadataStore  for SqliteStore { ... }
impl HistoryStore   for SqliteStore { ... }

// ホスト KV は redb backend (P2)。`state.redb` の単一 "kv" テーブルを使う。
pub struct RedbKvStore { db: Arc<redb::Database> }
impl KvStore  for RedbKvStore { ... }

// (P4) プラグイン専用 KV / cache。`plugin-kv.redb` を plugin_id で隔離。
pub struct RedbPluginKv { db: Arc<redb::Database>, plugin_id: String }
impl KvStore  for RedbPluginKv { ... }
impl Cache    for RedbPluginCache { ... }
```

理由:
- plugin に渡す trait を細かく絞れる (`PluginContext { kv: ..., metadata_query: ..., ... }` で書き込み権限は host のみ)
- テストで mock 可 (`MockMetadataQuery` を MetadataQuery だけ実装すれば足りる)
- 実装差し替え可 (将来 libsql or 他 backend へ)

---

## 5. 診断 / パフォーマンスログ

ストア操作の所要時間を計測してパフォーマンス解析に使うためのログ機構。デフォルトは全 off (本番は無音) で、`config.toml` の `[diagnostics.store]` で有効化する (スキーマは [`docs/config.md`](./config.md) §2)。出力は既存の `tracing` + `EnvFilter` (`RUST_LOG`) 経路 ([`crates/nohrs-core/src/telemetry/logging.rs`](../crates/nohrs-core/src/telemetry/logging.rs)) にそのまま乗る。

### SQLite

rusqlite の組み込みフックを使う:

- `Connection::profile(Some(callback))` — 各ステートメント完了後に実行 SQL と所要時間を受け取る。`slow_query_ms` 超過なら `warn`、`log_all_queries` 有効時は全件を `debug` で `tracing` へ emit (target = `nohrs_store::sql`)。
- `Connection::trace(Some(callback))` — 必要なら展開後 SQL を `trace` レベルで出力 (より詳細)。

### redb

redb には同等の組み込みフックが無いため、`RedbKvStore` の各操作 (`get` / `put` / `delete` / `batch`) を計測ラッパで囲み、`log_redb_ops` 有効時に操作名と所要時間を `tracing` へ emit (target = `nohrs_store::redb`)。

### 方針

- フックの登録はストア接続の open 時に config を見て決定する (config off ならフック自体を登録せず、無効時のオーバーヘッドをゼロにする)。
- 閾値・フラグの解釈は `config.toml` のレニエントなバリデーション方針 (config.md §6) に従う。

---

## 6. プラグインへの公開範囲

| 機能 | コミュニティ plugin の権限 |
|------|--------------------------|
| **自プラグイン専用 KV** (`RedbPluginKv` for own `plugin_id`) | ✅ 常時 |
| **自プラグイン専用 Cache** | ✅ 常時 |
| **`MetadataQuery::list_children` 等の読み取り** | ✅ `read_paths` permission の範囲内 |
| **他 plugin の KV** | ❌ 構造的に不可 (table 隔離) |
| **`MetadataStore::upsert_file` 等の書き込み** | ❌ host のみ |
| **`HistoryStore` 直接アクセス** | ❌ host 経由でのみ (`launcher.contribute` 等の API 越し) |

詳細は [`docs/plugin-api.md`](./plugin-api.md) §host imports 参照。

---

## 7. バックアップ・移行

- いずれも単一ファイル。P2 では `db.sqlite` (メタデータ・履歴) と `state.redb` (ホスト KV) の 2 ファイル、P4 で `plugin-kv.redb` が加わる
  ```sh
  cp ~/.local/share/nohrs/db.sqlite   ./backup-$(date +%Y%m%d).sqlite
  cp ~/.local/share/nohrs/state.redb  ./backup-$(date +%Y%m%d).redb
  ```
- ユーザー向けの export/import 機能は P5 以降で検討
