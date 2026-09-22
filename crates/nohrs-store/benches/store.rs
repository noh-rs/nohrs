//! Baseline measurements for the store backends.
//!
//! These exist so the P3 indexer has numbers to regress against: it will drive
//! `upsert_file` and `list_changed_since` at a rate nothing here does yet, and
//! a change that makes either an order of magnitude slower should be visible as
//! a number rather than as a report that indexing "feels slow".
//!
//! Every benchmark runs against an on-disk database in a `tempfile::TempDir`,
//! not `:memory:`. The in-memory backend skips WAL and the page cache, which is
//! most of what these calls actually cost.

// Benchmarks are fixture code in the same sense tests are: setup that cannot
// proceed on failure panics rather than propagating, and criterion's generated
// `main` takes no `Result`. `clippy.toml` exempts `#[cfg(test)]` code from
// `expect_used` but has no equivalent for benches, so spell it out here.
#![allow(clippy::expect_used)]
// Fires on the function `criterion_group!` generates, which is not ours to
// document.
#![allow(missing_docs)]

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use nohrs_store::{
    FileUpsert, HistoryEntry, HistoryKind, HistoryStore, KvKey, KvOp, KvStore, MetadataQuery,
    MetadataStore, RedbKvStore, SqliteStore, StoreLogConfig,
};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// How many rows the read benchmarks query against. Large enough that an index
/// matters, small enough that setup does not dominate the run.
const ROW_COUNT: usize = 10_000;

/// Directories the rows are spread across, so `list_children` returns a page
/// rather than the whole table.
const DIRECTORY_COUNT: usize = 100;

fn entry(index: usize) -> FileUpsert {
    let directory = index % DIRECTORY_COUNT;
    let parent_path = PathBuf::from(format!("/home/bench/dir{directory}"));
    FileUpsert {
        path: parent_path.join(format!("file{index}.rs")),
        parent_path,
        inode: index as u64 + 1,
        size: 4096,
        mtime_ns: 1_700_000_000_000_000_000 + index as i64,
        content_hash: None,
    }
}

/// A populated store plus the directory holding it. The `TempDir` is returned
/// alongside because dropping it deletes the database.
fn populated_sqlite() -> (TempDir, SqliteStore, PathBuf) {
    let directory = TempDir::new().expect("create temp dir");
    let path = directory.path().join("bench.sqlite");
    let store = SqliteStore::open(&path, &StoreLogConfig::default()).expect("open store");
    for index in 0..ROW_COUNT {
        store.upsert_file(&entry(index)).expect("seed row");
    }
    (directory, store, path)
}

fn metadata_reads(criterion: &mut Criterion) {
    let (_directory, store, _path) = populated_sqlite();
    let mut group = criterion.benchmark_group("sqlite/metadata_read");

    let known_path = entry(ROW_COUNT / 2).path;
    group.bench_function("get_file", |bencher| {
        bencher.iter(|| black_box(store.get_file(black_box(&known_path)).expect("get_file")));
    });

    let parent = Path::new("/home/bench/dir7");
    group.bench_function("list_children", |bencher| {
        bencher.iter(|| {
            black_box(
                store
                    .list_children(black_box(parent))
                    .expect("list_children"),
            )
        });
    });

    // Halfway through the seeded range, so the query returns roughly half the
    // table — the shape the P3 incremental re-index will ask for.
    let since = 1_700_000_000_000_000_000 + (ROW_COUNT / 2) as i64;
    group.bench_function("list_changed_since", |bencher| {
        bencher.iter(|| {
            black_box(
                store
                    .list_changed_since(black_box(since))
                    .expect("list_changed_since"),
            )
        });
    });

    group.bench_function("find_by_inode", |bencher| {
        bencher.iter(|| {
            black_box(
                store
                    .find_by_inode(black_box(ROW_COUNT as u64 / 2))
                    .expect("find_by_inode"),
            )
        });
    });

    group.finish();
}

fn metadata_writes(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("sqlite/metadata_write");

    group.bench_function("upsert_file/insert", |bencher| {
        bencher.iter_batched(
            || {
                let directory = TempDir::new().expect("create temp dir");
                let store = SqliteStore::open(
                    &directory.path().join("bench.sqlite"),
                    &StoreLogConfig::default(),
                )
                .expect("open store");
                // The store comes first so it drops before the directory it
                // lives in. Deleting a directory whose database is still open
                // fails on Windows, and the sample would leak its fixture.
                (store, directory)
            },
            |(store, directory)| {
                store.upsert_file(&entry(0)).expect("insert");
                // Returned so the drop (and the file deletion it triggers) is
                // charged to teardown rather than to the measured routine.
                (store, directory)
            },
            BatchSize::SmallInput,
        );
    });

    let (_directory, store, _path) = populated_sqlite();
    group.bench_function("upsert_file/update", |bencher| {
        bencher.iter(|| store.upsert_file(black_box(&entry(0))).expect("update"));
    });

    group.finish();
}

fn history(criterion: &mut Criterion) {
    let (_directory, store, _path) = populated_sqlite();
    let mut group = criterion.benchmark_group("sqlite/history");

    for index in 0..1_000 {
        store
            .record(HistoryEntry {
                kind: HistoryKind::Open,
                payload: format!("/home/bench/dir0/file{index}.rs"),
                occurred_at: 1_700_000_000_000_000_000 + index,
            })
            .expect("seed history");
    }

    group.bench_function("record", |bencher| {
        bencher.iter(|| {
            store
                .record(HistoryEntry {
                    kind: HistoryKind::Search,
                    payload: "bench".to_string(),
                    occurred_at: 1_700_000_000_000_000_000,
                })
                .expect("record")
        });
    });

    group.bench_function("list/50", |bencher| {
        bencher.iter(|| black_box(store.list(HistoryKind::Open, black_box(50)).expect("list")));
    });

    group.finish();
}

fn host_kv(criterion: &mut Criterion) {
    let directory = TempDir::new().expect("create temp dir");
    let store = RedbKvStore::open(
        &directory.path().join("bench.redb"),
        &StoreLogConfig::default(),
    )
    .expect("open kv");

    let key = KvKey::new("session", "explorer_tabs").expect("valid key");
    // Roughly the size of a serialized tab session, which is what the explorer
    // writes on its 500ms debounce.
    let value = vec![0_u8; 4096];
    store.put(&key, &value).expect("seed value");

    let mut group = criterion.benchmark_group("redb/host_kv");

    group.bench_function("get", |bencher| {
        bencher.iter(|| black_box(store.get(black_box(&key)).expect("get")));
    });

    group.bench_function("put", |bencher| {
        bencher.iter(|| store.put(black_box(&key), black_box(&value)).expect("put"));
    });

    let batch: Vec<KvOp> = (0..16)
        .map(|index| KvOp::Put {
            key: KvKey::new("session", &format!("pane{index}")).expect("valid key"),
            value: value.clone(),
        })
        .collect();
    // `KvStore::batch` takes the ops by value, so each sample needs its own
    // copy. Cloning 16 × 4 KiB inside the measured closure would put that
    // allocation into the number, so it goes in setup.
    group.bench_function("batch/16", |bencher| {
        bencher.iter_batched(
            || batch.clone(),
            |batch| store.batch(batch).expect("batch"),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("list_namespace", |bencher| {
        bencher.iter(|| {
            black_box(
                store
                    .list_namespace(black_box("session"))
                    .expect("list_namespace"),
            )
        });
    });

    group.finish();
}

criterion_group!(benches, metadata_reads, metadata_writes, history, host_kv);
criterion_main!(benches);
