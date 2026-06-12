# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Versioning policy: before `0.1.0` (the `0.0.z` pre-MVP stream) there is no stability
guarantee and any release may break. From `0.1.0` on, breaking changes are batched into
the next minor (`x`) bump, which is cut when a roadmap phase completes; patch (`y`) bumps
are additive changes within a phase. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for details.

## [Unreleased]

### Changed

- Split the single `nohrs` crate into a Cargo workspace of six layered crates
  (`nohrs-core`, `nohrs-models`, `nohrs-services`, `nohrs-ui`, `nohrs-pages`, and
  the `nohrs` binary), with a strict downward dependency direction. Shared
  package metadata, dependency versions, and lints are inherited from
  `[workspace.*]`. The toolkit-free crates build on Linux CI via
  `default-members`; the GUI crates build with `--workspace` (macOS). See
  [ADR 0003](docs/adr/0003-cargo-workspace-layer-split.md).
- `FileEntryDto` moved to `nohrs-models` so the UI layer no longer depends on
  services. The Explorer window root (`RootView`) lives in `nohrs-pages` as the
  Explorer "pillar", symmetric with the future launcher window (`nohrs-launcher`,
  P3); `nohrs-ui` keeps only the shared window chrome, and the `nohrs` binary is
  a thin startup sequence (`NohrsApp`) that opens the window(s).

### Added

- OSS hygiene baseline: CI workflow, Dependabot config, issue/PR templates,
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, and this changelog.
- Package metadata (`description`, `repository`, `homepage`, `license`,
  `keywords`, `categories`, `authors`, `rust-version`) so `cargo publish` works.

[Unreleased]: https://github.com/noh-rs/nohrs/commits/develop
