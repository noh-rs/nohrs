# Persistence — SQLite + redb

> Status: Draft (P2 で実装、P4 で plugin KV 拡張)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/async-runtime.md`](./async-runtime.md), [`docs/plugin-api.md`](./plugin-api.md)

本書はメタデータ・履歴・プラグイン状態の永続化レイヤを定めます。

---

## 1. 全体方針

| データ種別 | ストア | 理由 |
|-----------|--------|------|
| **ファイルメタデータ・履歴・KV (ホスト)** | **SQLite (rusqlite)** | SQL 表現力 (差分 query / 結合 query) が必要 |
| **プラグイン専用 KV** | **redb** | 高速 R/W、plugin_id でテーブル隔離、host data と分離 |
| **設定ファイル** | TOML (`config.toml`) | 詳細は [`docs/config.md`](./config.md) |

理由:
- ホストデータは SQL 表現力 (検索メタデータの差分検出など) が必要
- プラグイン KV はシンプル かつ高速性が要求 (Raycast / VSCode 流の plugin state)
- 2 つの DB を持つが、それぞれ単一ファイルで backup 単純、`MetadataStore` / `KvStore` の trait は別物なので混乱無し

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

-- ホスト KV (動的設定、launcher window position 等)
CREATE TABLE key_value (
    key        TEXT PRIMARY KEY,
    value      BLOB NOT NULL,    -- JSON or MessagePack
    updated_at INTEGER NOT NULL
);

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

## 3. redb (plugin KV)

### 設定

```toml
[dependencies]
redb = "2"
```

### ファイル配置

- `$XDG_DATA_HOME/nohrs/plugin-kv.redb` (単一ファイル)
- ACID + MVCC、SQLite と同じく WAL 風 crash recovery

### テーブル設計

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
    fn get(&self, key: &str) -> Result<Option<Bytes>>;
    fn put(&self, key: &str, value: &[u8]) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
    fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, Bytes)>>;
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
impl KvStore        for SqliteStore { ... }   // ホスト KV
impl HistoryStore   for SqliteStore { ... }

pub struct RedbPluginKv { db: Arc<redb::Database>, plugin_id: String }
impl KvStore  for RedbPluginKv { ... }
impl Cache    for RedbPluginCache { ... }
```

理由:
- plugin に渡す trait を細かく絞れる (`PluginContext { kv: ..., metadata_query: ..., ... }` で書き込み権限は host のみ)
- テストで mock 可 (`MockMetadataQuery` を MetadataQuery だけ実装すれば足りる)
- 実装差し替え可 (将来 libsql or 他 backend へ)

---

## 5. プラグインへの公開範囲

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

## 6. バックアップ・移行

- SQLite は単一ファイル → `cp ~/.local/share/nohrs/db.sqlite ./backup-$(date +%Y%m%d).sqlite`
- redb も単一ファイル
- ユーザー向けの export/import 機能は P5 以降で検討
