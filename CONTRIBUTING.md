# Contributing to Nohrs

Thanks for your interest in contributing! Nohrs is **pre-alpha**, so things move
fast — issues, pull requests, and discussion are all welcome.

By participating you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Getting started

1. Read the [Roadmap](docs/ROADMAP.md) to see where the project is headed.
2. Browse [open issues](https://github.com/noh-rs/nohrs/issues). Issues labeled
   `good first issue` are a good entry point.
3. For anything non-trivial, open (or comment on) an issue first so we can agree
   on the approach before you invest time.

## Development environment

The Rust toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
`rustup` picks it up automatically. The full setup matrix (macOS, Linux native,
Nix, Docker) lives in [`docs/dev-environment.md`](docs/dev-environment.md).

```sh
# Core library (no GUI) — builds on any platform
cargo build

# GUI binary — requires gpui's platform deps (Metal on macOS; see dev-environment.md)
cargo run --features gui --bin nohrs
```

The `gui` feature pulls in `gpui` and only builds where its platform backend is
available, so CI and the core library checks run **without** it.

## Branching and pull requests

- Branch off `develop` (the default branch). `main` tracks releases.
- Keep pull requests focused; split unrelated changes into separate PRs.
- Open your PR against `develop` and fill in the PR template.

### PR title rules

- Imperative mood, correctly capitalized — e.g. `Fix crash in project panel`.
- **No** conventional-commit prefixes (`fix:`, `feat:`, `docs:`, …).
- No trailing punctuation.
- Optionally prefix with the crate/scope when one is the clear owner —
  e.g. `search: Add fuzzy matching`.

### Release notes

Every PR body ends with a `Release Notes:` section containing exactly one bullet:

```text
Release Notes:

- Added ...        # or "Fixed ..." / "Improved ..." for user-facing changes
- N/A              # for docs-only / non-user-facing changes
```

User-facing changes should also be added to the `[Unreleased]` section of
[`CHANGELOG.md`](CHANGELOG.md).

## Commit messages

- Write in the imperative mood and explain the *why* when it isn't obvious.
- Group logically related changes into a single commit; avoid noisy fixups in
  the final history (squash locally before pushing if needed).

## Code style and quality gate

Run these before pushing — they mirror what CI enforces:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check bans licenses sources   # license / dependency policy
```

CI runs these on Linux against the headless library subset and additionally on
macOS across the full workspace (`--workspace --all-features`, which builds the
gpui GUI). `cargo deny check advisories` also runs in CI but is informational.

Coding conventions (error handling, naming, avoiding panics, GPUI patterns) are
documented in [`.rules`](.rules) / `CLAUDE.md`. Highlights:

- Prefer correctness and clarity over cleverness.
- Never panic in library code: propagate errors with `?` instead of `unwrap()` /
  `expect()`, and never silently discard errors with `let _ =`.
- Only add comments that explain a non-obvious *why*.

## Testing

- Unit tests live in each module under `#[cfg(test)] mod tests`.
- Snapshot tests use [`insta`](https://insta.rs) (e.g. the config parser in
  `nohrs-core`); refresh intentional changes with `cargo insta review`. Reach for
  `proptest` only for genuinely property-shaped logic (kept deliberately rare).

### Writing GPUI tests

GPUI view/state tests use `TestAppContext` and run headlessly (no display). The
gpui-backed crates (`nohrs-ui`, `nohrs-pages`) enable gpui's `test-support`
feature as a **dev-dependency** to make `TestAppContext` and the `#[gpui::test]`
macro available:

```toml
[dev-dependencies]
gpui = { version = "0.2", features = ["test-support"] }
```

Build the view inside a test window so its window-bound sub-entities can be
constructed, mutate it through `window.update`, and assert with
`window.read_with`. Drive async work with the GPUI executor timer
(`cx.background_executor.timer(...).await`) followed by `run_until_parked()` —
**never** `smol::Timer` / `tokio::time::sleep`, which the GPUI scheduler does not
track (so `run_until_parked()` would return early). See the worked examples in
[`crates/nohrs-pages/src/explorer/tests.rs`](crates/nohrs-pages/src/explorer/tests.rs)
and the patterns in [`docs/testing.md`](docs/testing.md).

```rust
#[gpui::test]
async fn loads_preview(cx: &mut TestAppContext) {
    let window = cx.add_window(|window, cx| MyView::new(window, cx));
    window.update(cx, |view, window, cx| view.start_async_work(window, cx)).unwrap();
    cx.background_executor.timer(Duration::from_millis(50)).await;
    cx.run_until_parked();
    window.read_with(cx, |view, _cx| assert!(view.is_ready())).unwrap();
}
```

### Coverage

Generate an HTML coverage report locally with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
(`cargo install cargo-llvm-cov`):

```sh
cargo llvm-cov --all-features --html
open target/llvm-cov/html/index.html   # xdg-open on Linux
```

CI measures coverage on two tiers: a Linux **core** tier (`-p nohrs-core`) and a
macOS **overall** tier (`--workspace --all-features`, which instruments the gpui
crates). Each tier feeds GitHub's native code coverage (inline PR diff) and
uploads its HTML report as a downloadable artifact (`coverage-html-core` /
`coverage-html-overall`); a single PR comment reports both against their targets.

Coverage is an **enforced gate** with two layers, so a high aggregate cannot hide
an untested file. The build fails if:

- `nohrs-core` drops below **80%** lines (aggregate *and* per file), or
- the overall workspace drops below **50%** lines, or
- any individually testable file drops below **70%** lines.

The per-file floor excludes code that is not unit-testable in-process (the gpui
binary entrypoint, pure rendering/view and window-chrome code, the external
search backends, and app-only glue) via `--ignore-filename-regex` — see the
`coverage-*` jobs in `ci.yml` for the exact list. Check locally before pushing:

```sh
cargo llvm-cov -p nohrs-core --fail-under-lines 80 --fail-under-file-lines 80
cargo llvm-cov --workspace --all-features --fail-under-lines 50
```

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
