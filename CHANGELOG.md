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

- `nohrs-cli`, a toolkit-free CLI crate whose `noh` binary carries the first
  command: `noh rm`. It removes files the way `rm(1)` does (`-r`/`-R`, `-d`,
  `-f`, `-i`, `-v`, `--`) but moves them to the trash instead of unlinking them;
  `--permanent` opts back into a real delete. Symlinking the binary as `rm`
  earlier on `PATH` puts it in front of the system `rm`, so an existing `rm -rf`
  becomes recoverable. See [`docs/cli.md`](docs/cli.md).
- `noh restore` puts trashed items back where they came from — the most recent
  deletion by default, or the ones named by path or file name, with `--all` and
  `--since`. It refuses to overwrite whatever occupies the original location and
  recreates a directory that has since been removed.
- `noh trash list` / `purge` / `empty` inspect the trash and empty it, with
  `--older-than` for stale items, `--json` for scripting, and a confirmation
  before every irreversible delete unless `--force` is given.
- A trash ledger (`nohrs-services::fs::trash_ledger`) recording what nohrs
  trashed and where it came from. macOS keeps that information in Finder's
  private `.DS_Store` and exposes no trash index, so restoring there needs
  nohrs's own record; Linux and Windows keep using the OS index, and the ledger
  is not written where nothing reads it. `fs::ops::trash_path` writes it, so
  items deleted in the GUI can be restored from the CLI and vice versa.
- `noh shim install` / `uninstall` / `status`, which manage the symlinks that put
  `noh` in front of a system command. Unlike a hand-written `ln -sf`, they never
  replace a regular file and only remove a link that points back at this binary.
- `noh doctor`, reporting whether the shim wins on `PATH`, whether the trash
  directory and ledger are usable, and whether `config.toml` parses.
- `noh completions <shell>` for shell completion scripts.
- OSS hygiene baseline: CI workflow, Dependabot config, issue/PR templates,
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, and this changelog.
- Package metadata (`description`, `repository`, `homepage`, `license`,
  `keywords`, `categories`, `authors`, `rust-version`) so `cargo publish` works.

[Unreleased]: https://github.com/noh-rs/nohrs/commits/develop
