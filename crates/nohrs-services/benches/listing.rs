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

/// The largest directory measured, and the one the paging benchmarks use.
/// 1,000 is the current `DIR_LISTING_LIMIT`, so 10,000 shows what a directory
/// past the limit costs to page through.
const LARGE: usize = 10_000;

/// Smaller directory sizes, measured alongside `LARGE`.
const SMALL_SIZES: [usize; 3] = [10, 100, 1_000];

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

/// Builds a directory of `count` empty subdirectories.
fn directory_of_dirs(count: usize) -> TempDir {
    let directory = TempDir::new().expect("create temp dir");
    for index in 0..count {
        fs::create_dir(directory.path().join(format!("dir{index}"))).expect("create bench dir");
    }
    directory
}

fn path_of(directory: &TempDir) -> &str {
    directory
        .path()
        .to_str()
        .expect("temp dir path is valid UTF-8")
}

fn list(path: &str, limit: usize, cursor: Option<&str>) {
    black_box(
        list_dir_sync(ListParams {
            path: black_box(path),
            limit,
            cursor,
        })
        .expect("list_dir_sync"),
    );
}

/// All the listing benchmarks. They share one function because each fixture
/// costs `count` filesystem writes, and the `LARGE` one is wanted by three of
/// the groups — building it once keeps the CI smoke run from paying for it
/// three times.
fn listing(criterion: &mut Criterion) {
    let large = directory_of(LARGE);
    let large_path = path_of(&large);

    let mut first_page = criterion.benchmark_group("fs/list_dir/first_page");
    for size in SMALL_SIZES {
        let directory = directory_of(size);
        let path = path_of(&directory);
        first_page.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, _| {
            bencher.iter(|| list(path, PAGE, None));
        });
    }
    first_page.bench_with_input(BenchmarkId::from_parameter(LARGE), &LARGE, |bencher, _| {
        bencher.iter(|| list(large_path, PAGE, None));
    });
    first_page.finish();

    // Two questions about the same fixture. A later page costs the same as the
    // first, and a small page costs the same as a large one, because the whole
    // directory is read and sorted before the page is sliced. Both are here to
    // make that visible: if paging is ever made incremental, the curves split.
    let mut paging = criterion.benchmark_group("fs/list_dir/paging");

    // Derived from LARGE rather than written out, because `list_dir_impl`
    // clamps an offset past the end (`cursor…min(total)`) and silently returns
    // an empty page — a stale literal would stop measuring a later page at all,
    // without failing.
    let last_page_offset = LARGE.saturating_sub(PAGE);
    let cursor = last_page_offset.to_string();
    paging.bench_function(format!("offset/{last_page_offset}"), |bencher| {
        bencher.iter(|| list(large_path, PAGE, Some(&cursor)));
    });

    for limit in [10_usize, 100, 1_000] {
        paging.bench_with_input(
            BenchmarkId::new("page_size", limit),
            &limit,
            |bencher, &limit| {
                bencher.iter(|| list(large_path, limit, None));
            },
        );
    }
    paging.finish();

    // Every entry costs a `symlink_metadata` in the per-entry loop regardless
    // of its kind, so this is not a more expensive shape than a flat listing —
    // it pins the all-directory case so a future kind-dependent branch shows up.
    let directories = directory_of_dirs(1_000);
    let directories_path = path_of(&directories);
    let mut subdirectories = criterion.benchmark_group("fs/list_dir/subdirectories");
    subdirectories.bench_function("1000", |bencher| {
        bencher.iter(|| list(directories_path, PAGE, None));
    });
    subdirectories.finish();
}

criterion_group!(benches, listing);
criterion_main!(benches);
