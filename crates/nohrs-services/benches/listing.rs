//! Baseline measurements for directory listing.
//!
//! `list_dir_sync` is what the explorer calls on every navigation, and it runs
//! on the UI thread today (#268). Moving it to the background executor will not
//! make it cheaper, so these numbers are the ones to watch when the listing
//! path is reworked: `list_dir_impl` collects and sorts every name in the
//! directory before taking a page, so cost grows with the directory rather
//! than with the page.

// See the same block in `nohrs-store/benches/store.rs`: benches are fixture
// code, and `clippy.toml`'s test exemptions do not reach them.
#![allow(clippy::expect_used)]
#![allow(missing_docs)]
// `std::fs::write` is banned so that blocking I/O stays off the GPUI foreground
// thread. A bench harness has no UI thread, and building the fixture directory
// is precisely the I/O being set up for.
#![allow(clippy::disallowed_methods)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use nohrs_services::fs::listing::{ListParams, list_dir_sync};
use std::fs;
use std::hint::black_box;
use tempfile::TempDir;

/// Directory sizes to measure at. 1,000 is the current `DIR_LISTING_LIMIT`, so
/// 10,000 shows what a directory past the limit costs to page through.
const SIZES: [usize; 4] = [10, 100, 1_000, 10_000];

/// The page size the explorer asks for.
const PAGE: usize = 1_000;

/// Builds a directory of `count` empty files, named so that sorting has real
/// work to do rather than walking an already-ordered list.
fn directory_of(count: usize) -> TempDir {
    let directory = TempDir::new().expect("create temp dir");
    for index in 0..count {
        // Reversing the digits keeps creation order and sort order apart.
        let name: String = index.to_string().chars().rev().collect();
        fs::write(directory.path().join(format!("{name}_{index}.txt")), b"")
            .expect("create bench file");
    }
    directory
}

fn path_of(directory: &TempDir) -> &str {
    directory
        .path()
        .to_str()
        .expect("temp dir path is valid UTF-8")
}

fn first_page(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("fs/list_dir/first_page");

    for size in SIZES {
        let directory = directory_of(size);
        let path = path_of(&directory);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
            bencher.iter(|| {
                black_box(
                    list_dir_sync(ListParams {
                        path: black_box(path),
                        limit: PAGE,
                        cursor: None,
                    })
                    .expect("list_dir_sync"),
                )
            });
        });
    }

    group.finish();
}

fn later_page(criterion: &mut Criterion) {
    // A later page costs the same as the first, because the whole directory is
    // read and sorted before the page is sliced. This benchmark is here to make
    // that visible: if paging is ever made incremental, the two curves separate.
    let directory = directory_of(10_000);
    let path = path_of(&directory);

    let mut group = criterion.benchmark_group("fs/list_dir/later_page");
    group.bench_function("offset_9000", |bencher| {
        bencher.iter(|| {
            black_box(
                list_dir_sync(ListParams {
                    path: black_box(path),
                    limit: PAGE,
                    cursor: Some("9000"),
                })
                .expect("list_dir_sync"),
            )
        });
    });
    group.finish();
}

fn page_size(criterion: &mut Criterion) {
    let directory = directory_of(10_000);
    let path = path_of(&directory);

    let mut group = criterion.benchmark_group("fs/list_dir/page_size");
    for limit in [10_usize, 100, 1_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(limit),
            &limit,
            |bencher, &limit| {
                bencher.iter(|| {
                    black_box(
                        list_dir_sync(ListParams {
                            path: black_box(path),
                            limit,
                            cursor: None,
                        })
                        .expect("list_dir_sync"),
                    )
                });
            },
        );
    }
    group.finish();
}

fn nested(criterion: &mut Criterion) {
    // Directories cost an extra `symlink_metadata` each in the per-entry loop,
    // so a directory of directories is the more expensive shape.
    let directory = TempDir::new().expect("create temp dir");
    for index in 0..1_000 {
        fs::create_dir(directory.path().join(format!("dir{index}"))).expect("create bench dir");
    }
    let path = path_of(&directory);

    let mut group = criterion.benchmark_group("fs/list_dir/subdirectories");
    group.bench_function("1000", |bencher| {
        bencher.iter(|| {
            black_box(
                list_dir_sync(ListParams {
                    path: black_box(path),
                    limit: PAGE,
                    cursor: None,
                })
                .expect("list_dir_sync"),
            )
        });
    });
    group.finish();
}

criterion_group!(benches, first_page, later_page, page_size, nested);
criterion_main!(benches);
