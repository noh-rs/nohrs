# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Versioning policy: before `0.1.0` (the `0.0.z` pre-MVP stream) there is no stability
guarantee and any release may break. From `0.1.0` on, breaking changes are batched into
the next minor (`x`) bump, which is cut when a roadmap phase completes; patch (`y`) bumps
are additive changes within a phase. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for details.

## [Unreleased]

### Added

- `nohrs-indexd` owns the search index's writer and the file watcher beside it.
  tantivy allows one writer across all processes, and the watcher's output is a
  stream of write requests, so putting both in one place makes "the index is
  keeping up with the filesystem" a single thing to check rather than something
  inferred from two — the daemon outlives a watcher it failed to install, and
  says so, which is what `noh index status`'s `daemon` line reports. It is
  not installed or started at login: the first process that wants it starts it,
  and it stops once its last client has been gone for 90 seconds — so the only
  thing to quit is nohrs. Searches never go through it; readers open the index
  directly, which is what makes a daemon that is down, busy or a version behind
  cost freshness and never an answer. See
  [ADR 0010](docs/adr/0010-indexd-owns-the-index-writer.md).
- `noh search <QUERY> [PATH]...` looks for a query in a directory tree, matching
  both the names walked and the text inside the files (`--name` / `--content`
  narrow it to one). Name matches print as the bare path and content matches as
  `path:line:text`, with `-i`, `-F`, `-l`, `--limit`, `--max-depth`, `--hidden`,
  `--no-ignore` and `--json` to shape the search and its output. Matching
  nothing exits `0` and says so on stderr, leaving stdout the empty stream a
  pipe expects. Where the index covers the scope it chooses the candidate files
  in BM25 order and the lines are matched in those files; `--engine` forces
  index or walk, and a fallback says on stderr why the index stood aside. See
  [`docs/cli.md`](docs/cli.md) §4.
- `noh index status` / `noh index build` report where the index is, what it
  covers and how much it holds, and build it on a machine that never opens the
  GUI. `build` is incremental: it re-reads only the files whose modification
  time differs from the index's, and `--full` forces the rest. Both ask the
  daemon, starting one where none is running, so a build no longer fails
  because the app is open; only where a daemon cannot be had at all — no unix
  sockets, or a start that fails — does `build` index in-process, where it can
  still find the writer held elsewhere, which it reports rather than hides.
  `status` needs no writer either way — it reads the index's own documents —
  which is why it runs beside the app; starting a daemon to ask it installs the
  watcher and begins a pass in the background. It also
  reports whether anything is watching — the difference between "up to date"
  and "up to date as of whenever this last ran". `noh index stop` stops the
  daemon without touching the index. See [`docs/cli.md`](docs/cli.md) §5.
- nohrs now records what it does to a rolling JSON Lines file under
  `$XDG_STATE_HOME/nohrs/logs/`, so a GUI session's log survives the window
  closing, and `noh log show` / `path` / `clear` read it back. Every operation
  carrying `#[tracing::instrument(target = "nohrs::op", …)]` — file operations,
  directory listings, search, and indexing so far — writes one record with how
  long it took, which `noh log show --ops` lists. The records name the files
  touched and the searches run, so on Unix the directory is created `0700` and
  its files `0600` (Windows has no mode to set: the directory inherits the
  per-user profile's ACL), and they are dropped after seven days. See
  [`docs/logging.md`](docs/logging.md).

### Changed

- Indexing costs the walk rather than the index. Reading what the index already
  holds took 678ms of a 700ms pass over 20,000 files, against 39ms for the walk
  and every `stat` in it: the modification times came out of the document store,
  which is compressed in blocks, and each path was resolved by seeking around a
  sorted dictionary. Both now come from the index's own columns, with the
  dictionary streamed once in its own order — 6.9ms. The walk itself is
  parallel (`IndexWriter::add_document` takes `&self`), on half the machine's
  threads and no more than four, and indexing a file no longer copies it a
  second time to prepend its path. A warm pass over those 20,000 files went
  from 0.7s to under 0.1s, which is what the app pays on every launch.
- `IndexReader` holds its reader and query parser open rather than rebuilding
  them per query, and takes tantivy's manual reload policy, so reading the
  index no longer installs a directory watcher in every process that reads.
- Indexing re-reads only what changed. Documents now carry the file's
  modification time, so a pass compares it against the file on disk and skips
  what matches, and documents whose file has disappeared are removed instead of
  answering searches forever — though not the ones under a directory the pass
  could not read, since an unreadable directory is reported once and not
  descended into, and "the walk could not look there" is not "the files are
  gone". The app therefore runs the pass on every launch
  rather than only when the index is empty: the file watcher only sees changes
  made while the app is running, so files that changed between quitting and
  launching again reached the index nowhere else. An index left by the previous
  schema is rebuilt once, since it carries no times to compare.
- A change the watcher reports is checked against the index before the file is
  opened, and skipped when its modification time still matches. Indexing a file
  opens it, and `notify` reports an open as an event like any other, so a pass
  that re-indexed whatever was reported was feeding the watcher its own reads:
  the daemon committed a fresh pass every debounce interval, indefinitely, over
  a tree nobody was touching.
- The search index takes tantivy's writer lock only when something is actually
  written, instead of from the moment a process opens the index. The app used to
  hold it from launch to quit whether or not it indexed anything, which made the
  index private to whichever process got there first — `noh index build` beside a
  running app was refused, and so was a second window. Readers never take the
  lock at all. See [ADR 0010](docs/adr/0010-indexd-owns-the-index-writer.md).
- The default stderr log filter quietens tantivy to `warn`: it narrates every
  commit and merge at `info`, which is half a screen of "save metas" for one
  `noh index build`. `RUST_LOG=info` brings it back.
- The minimum supported Rust version is 1.95, up from the 1.85 that edition 2024
  alone required. Nothing verified that floor, so it had drifted: `rusqlite`
  pulls in a `libsqlite3-sys` whose build script uses `cfg_select!`, stable only
  since 1.95, and the dependency graph separately asks for 1.89. Building on an
  older toolchain already failed — inside that build script, with a message that
  named neither nohrs nor a version — and now fails as an MSRV error instead. A
  new `msrv` CI job compiles the workspace on exactly the declared version, so
  the floor cannot drift again without CI saying so.
- Host KV keys are a `KvKey` rather than a `&str`, so the `<namespace>.<name>`
  convention is enforced instead of merely documented. A literal goes through
  the `kv_key!` macro, whose `const { … }` block forces const evaluation, so a
  key without a namespace is a build error; keys built at runtime go through
  `KvKey::new` / `parse` and return a `Result`.
  `KvStore::list_prefix` becomes `list_namespace`, which appends the
  separator itself so a listing cannot straddle into a longer namespace
  (`window` no longer picking up `window_backup.*`). See
  [`docs/persistence.md`](docs/persistence.md) §3.
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
- A trash ledger: the `trash` table of the metadata database, behind
  `nohrs_store::TrashLedger` (migration `002_trash.sql`). It records what nohrs
  trashed and where it came from, because macOS keeps that information in
  Finder's private `.DS_Store` and exposes no trash index of its own. Linux and
  Windows keep using the OS index, and the ledger is neither written nor even
  opened there. `fs::ops::trash_path` writes it, so items deleted in the GUI can
  be restored from the CLI and vice versa.
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
