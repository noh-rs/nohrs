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
- GPUI view/state tests use `TestAppContext`. Drive async work with the GPUI
  executor timer (`cx.background_executor.timer(...).await`) and
  `run_until_parked()` — **not** `smol::Timer` / `tokio::time::sleep`, which the
  GPUI scheduler doesn't track. See [`docs/testing.md`](docs/testing.md).

### Coverage

Generate an HTML coverage report locally with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
(`cargo install cargo-llvm-cov`):

```sh
cargo llvm-cov --all-features --html
open target/llvm-cov/html/index.html   # xdg-open on Linux
```

CI feeds a Cobertura XML report into GitHub's native code coverage (inline PR
diff coverage) and uploads the HTML report as a downloadable `coverage-html`
artifact, linked from a PR comment alongside the overall line coverage. Coverage
is informational during P1–P5 (target: core 80% / overall 50%) and does not
block merge.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
