//! User configuration: the P1 minimal `config.toml` schema, its 4-layer
//! override/merge strategy, lenient validation, JSON Schema generation, and the
//! on-disk template.
//!
//! Parsing is deliberately forward-compatible: unknown keys and out-of-range
//! values are downgraded to warnings rather than hard errors so a single typo
//! never prevents the app from starting. The only fatal case is TOML that does
//! not parse at all, which the caller surfaces in the UI status bar while
//! falling back to [`Config::default`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Schema version understood by this build. Bumped only when the on-disk
/// schema changes in a way that requires migration.
pub const CURRENT_SCHEMA_VERSION: u64 = 1;

/// URL advertised in the `#:schema` header of `config.toml` so TOML-aware
/// editors can offer completion and validation.
pub const SCHEMA_URL: &str = "https://nohrs.app/schema/config.schema.json";

/// The fully merged, validated configuration.
///
/// Models the P1 surface (`theme` / `ui`) plus the P2 sections: `[indexing]`,
/// `[search]`, `[launcher]`, and `[diagnostics]`. `[keybindings]` and
/// `[plugins]` are reserved here as forward-compatible drafts — their types are
/// defined and parsed leniently so files can adopt them now, but the values do
/// not drive behaviour yet (keybindings land in P3, plugins in P4; see
/// `docs/config.md` §2/§5).
///
/// Only `theme`, `ui`, and `diagnostics` are consumed at runtime today. The
/// remaining sections (`keybindings`, `plugins`, `indexing`, `search`,
/// `launcher`) are accepted and validated now so files and editor completion can
/// adopt them, but editing them has no effect until the matching subsystem is
/// wired in a later phase — see the per-section notes and `docs/config.md` §5.
///
/// Every field is `#[serde(default)]`: the loader starts from
/// [`Config::default`] and overlays only the keys present in the file, so a
/// missing section is filled with its default rather than rejected. The
/// generated schema therefore has no `required` properties, matching the
/// loader (an empty `config.toml` is valid).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Config {
    /// On-disk schema version. Must be [`CURRENT_SCHEMA_VERSION`] for this build.
    pub schema_version: u64,
    /// Appearance settings.
    pub theme: Theme,
    /// Explorer / view settings.
    pub ui: Ui,
    /// Keybinding overrides (draft; full support in P3). Empty means the
    /// built-in keymap is used.
    pub keybindings: Keybindings,
    /// Plugin selection (reserved; full support in P4).
    pub plugins: Plugins,
    /// Filesystem indexing strategy and exclusions.
    pub indexing: Indexing,
    /// Search backend selection.
    pub search: Search,
    /// Launcher window settings.
    pub launcher: Launcher,
    /// Diagnostics / performance-logging settings.
    pub diagnostics: Diagnostics,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: Theme::default(),
            ui: Ui::default(),
            keybindings: Keybindings::default(),
            plugins: Plugins::default(),
            indexing: Indexing::default(),
            search: Search::default(),
            launcher: Launcher::default(),
            diagnostics: Diagnostics::default(),
        }
    }
}

/// Appearance settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Theme {
    /// Light/dark selection, or following the OS.
    pub mode: ThemeMode,
    /// Accent colour, given as a named colour or a hex string. Full
    /// customization lands in P5; P1 stores the value and applies the mode.
    pub accent: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            accent: "blue".to_string(),
        }
    }
}

/// Explorer / view settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Ui {
    /// Column the listing is sorted by on load.
    pub default_sort: SortOrder,
    /// Whether dotfiles are shown in listings.
    pub show_hidden: bool,
    /// Identifier of the icon pack used to render entries.
    pub icon_pack: String,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            default_sort: SortOrder::Name,
            show_hidden: false,
            icon_pack: "default".to_string(),
        }
    }
}

/// Keybinding overrides (config.md §2, draft for P3).
///
/// A draft surface: a map of action identifier to key chord (e.g.
/// `"quit" = "ctrl-q"`). An empty map — the default — means the built-in keymap
/// is used. The action vocabulary and live re-binding are defined in P3; until
/// then values are stored and validated but not applied (hot reload is a
/// restart, config.md §5), so arbitrary action names are accepted without
/// warning to keep forward compatibility.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Keybindings {
    /// Action identifier → key chord. Empty means "use the built-in keymap".
    #[serde(flatten)]
    pub bindings: BTreeMap<String, String>,
}

/// Plugin selection (config.md §2, reserved for P4).
///
/// Type definition / reservation only: the lists are parsed and stored, but no
/// plugin host loads them yet (that lands in P4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Plugins {
    /// Built-in (first-party) plugins to enable, by id (e.g. `"git"`).
    pub core: Vec<String>,
    /// Community plugins to enable, as `user/repo` or a URL.
    pub community: Vec<String>,
}

/// Filesystem indexing settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Indexing {
    /// When the background index is built and maintained.
    pub mode: IndexingMode,
    /// Paths and globs excluded from indexing.
    pub exclude: IndexingExclude,
}

/// When the filesystem index is built and kept up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum IndexingMode {
    /// Index on demand and keep warm while the app is in use (the default).
    #[default]
    Auto,
    /// Continuously index in the background, even while idle.
    AlwaysOn,
    /// Only index when explicitly requested.
    Manual,
}

impl IndexingMode {
    /// Parse the `config.toml` spelling (`auto`/`always-on`/`manual`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always-on" => Some(Self::AlwaysOn),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Paths and globs excluded from indexing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct IndexingExclude {
    /// Literal paths (absolute or relative to the indexed root) to skip.
    pub paths: Vec<String>,
    /// Glob patterns (e.g. `**/target/**`) to skip.
    pub globs: Vec<String>,
}

/// Search backend selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Search {
    /// Which engine answers content searches.
    pub backend: SearchBackend,
}

/// The engine used to answer content searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SearchBackend {
    /// Pick the best available backend at runtime (the default).
    #[default]
    Auto,
    /// SQLite full-text search (FTS5).
    SqliteFts,
    /// The Tantivy index.
    Tantivy,
    /// External `ripgrep`.
    Ripgrep,
}

impl SearchBackend {
    /// Parse the `config.toml` spelling
    /// (`sqlite-fts`/`tantivy`/`ripgrep`/`auto`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sqlite-fts" => Some(Self::SqliteFts),
            "tantivy" => Some(Self::Tantivy),
            "ripgrep" => Some(Self::Ripgrep),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// Launcher window settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Launcher {
    /// Global hotkey that summons the launcher.
    pub hotkey: String,
    /// Whether the launcher reopens at its last on-screen position.
    pub position_remember: bool,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            hotkey: "Cmd+Shift+Space".to_string(),
            position_remember: true,
        }
    }
}

/// Diagnostics / performance-logging settings (see `docs/persistence.md` §5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Diagnostics {
    /// Performance logging for the persistence layer (`nohrs-store`).
    pub store: DiagnosticsStore,
}

/// Performance logging for the SQLite / redb store. All off by default, so a
/// default config adds no overhead. Output goes through `tracing` (targets
/// `nohrs_store::sql` / `nohrs_store::redb`) and is filterable with `RUST_LOG`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DiagnosticsStore {
    /// Log every SQL query at `debug` (verbose).
    pub log_all_queries: bool,
    /// Log SQL queries slower than this many milliseconds at `warn`. Zero (the
    /// default) disables slow-query logging.
    pub slow_query_ms: u64,
    /// Log redb `get`/`put`/`delete`/`batch` operations and their durations.
    pub log_redb_ops: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
/// Light/dark appearance selection.
pub enum ThemeMode {
    /// Follow the operating system's light/dark preference.
    #[default]
    System,
    /// Always use the light appearance.
    Light,
    /// Always use the dark appearance.
    Dark,
}

impl ThemeMode {
    /// Parse the `config.toml` / CLI / env spelling (`system`/`light`/`dark`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
/// The column a directory listing is ordered by.
pub enum SortOrder {
    /// Sort alphabetically by entry name.
    #[default]
    Name,
    /// Sort by last-modified time.
    Modified,
    /// Sort by file size.
    Size,
    /// Sort by entry kind (file/directory/...).
    Kind,
}

impl SortOrder {
    /// Parse the `config.toml` / CLI / env spelling
    /// (`name`/`modified`/`size`/`kind`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "name" => Some(Self::Name),
            "modified" => Some(Self::Modified),
            "size" => Some(Self::Size),
            "kind" => Some(Self::Kind),
            _ => None,
        }
    }
}

/// Severity of a configuration [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Recoverable: a value was ignored and a default used. The app keeps running.
    Warning,
    /// Fatal for the file: it could not be parsed (or targets an unknown schema
    /// version), so defaults are used. Surfaced in the UI status bar.
    Error,
}

/// A single message produced while loading configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity of the message.
    pub level: DiagnosticLevel,
    /// Human-readable description of the problem.
    pub message: String,
}

impl Diagnostic {
    fn warn(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
        }
    }
}

/// A sparse set of overrides applied on top of a loaded [`Config`]. Used for the
/// environment-variable and CLI-argument layers, where only the keys that were
/// actually provided should take effect.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverride {
    /// Override for [`Theme::mode`].
    pub theme_mode: Option<ThemeMode>,
    /// Override for [`Theme::accent`].
    pub theme_accent: Option<String>,
    /// Override for [`Ui::default_sort`].
    pub ui_default_sort: Option<SortOrder>,
    /// Override for [`Ui::show_hidden`].
    pub ui_show_hidden: Option<bool>,
    /// Override for [`Ui::icon_pack`].
    pub ui_icon_pack: Option<String>,
}

impl ConfigOverride {
    /// Build the environment-variable layer (`NOHRS_*`). Invalid values are
    /// logged and skipped rather than aborting startup.
    pub fn from_env() -> Self {
        let mut over = ConfigOverride::default();

        if let Ok(value) = std::env::var("NOHRS_THEME") {
            match ThemeMode::parse(value.trim()) {
                Some(mode) => over.theme_mode = Some(mode),
                None => tracing::warn!("ignoring invalid NOHRS_THEME={value:?}"),
            }
        }
        if let Ok(value) = std::env::var("NOHRS_ACCENT") {
            over.theme_accent = Some(value);
        }
        if let Ok(value) = std::env::var("NOHRS_DEFAULT_SORT") {
            match SortOrder::parse(value.trim()) {
                Some(sort) => over.ui_default_sort = Some(sort),
                None => tracing::warn!("ignoring invalid NOHRS_DEFAULT_SORT={value:?}"),
            }
        }
        if let Ok(value) = std::env::var("NOHRS_SHOW_HIDDEN") {
            match parse_bool(value.trim()) {
                Some(flag) => over.ui_show_hidden = Some(flag),
                None => tracing::warn!("ignoring invalid NOHRS_SHOW_HIDDEN={value:?}"),
            }
        }
        if let Ok(value) = std::env::var("NOHRS_ICON_PACK") {
            over.ui_icon_pack = Some(value);
        }

        over
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Config {
    /// Apply a sparse override layer in place. Only `Some` fields take effect.
    pub fn apply_override(&mut self, over: &ConfigOverride) {
        if let Some(mode) = over.theme_mode {
            self.theme.mode = mode;
        }
        if let Some(accent) = &over.theme_accent {
            self.theme.accent = accent.clone();
        }
        if let Some(sort) = over.ui_default_sort {
            self.ui.default_sort = sort;
        }
        if let Some(show_hidden) = over.ui_show_hidden {
            self.ui.show_hidden = show_hidden;
        }
        if let Some(icon_pack) = &over.ui_icon_pack {
            self.ui.icon_pack = icon_pack.clone();
        }
    }

    /// Parse `config.toml` contents into a [`Config`], collecting non-fatal
    /// problems as warnings. A TOML syntax error or an unknown `schema_version`
    /// yields [`Config::default`] plus an error diagnostic.
    ///
    /// This walks the parsed [`toml::Table`] field by field rather than using a
    /// strict `Deserialize`, so a single bad value (e.g. `mode = "neon"`) only
    /// resets that field instead of discarding the whole file.
    pub fn from_toml_str(contents: &str) -> (Config, Vec<Diagnostic>) {
        let table: toml::Table = match contents.parse() {
            Ok(table) => table,
            Err(error) => {
                return (
                    Config::default(),
                    vec![Diagnostic::error(format!(
                        "failed to parse config.toml: {error}"
                    ))],
                );
            }
        };
        Config::from_table(&table)
    }

    fn from_table(table: &toml::Table) -> (Config, Vec<Diagnostic>) {
        let mut config = Config::default();
        let mut diagnostics = Vec::new();

        // Reject configs from a future schema before reading anything else, so
        // we never half-apply a layout this build does not understand.
        if let Some(value) = table.get("schema_version") {
            match value.as_integer() {
                Some(version) if version as u64 == CURRENT_SCHEMA_VERSION => {}
                _ => {
                    diagnostics.push(Diagnostic::error(format!(
                        "config.toml targets schema_version {value}, but this nohrs build \
                         supports {CURRENT_SCHEMA_VERSION}; a newer version may be required. \
                         Using defaults."
                    )));
                    return (Config::default(), diagnostics);
                }
            }
        }

        if let Some(theme) = table.get("theme") {
            match theme.as_table() {
                Some(theme) => read_theme(theme, &mut config.theme, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[theme] is not a table; ignoring")),
            }
        }
        if let Some(ui) = table.get("ui") {
            match ui.as_table() {
                Some(ui) => read_ui(ui, &mut config.ui, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[ui] is not a table; ignoring")),
            }
        }
        if let Some(value) = table.get("keybindings") {
            match value.as_table() {
                Some(keys) => read_keybindings(keys, &mut config.keybindings, &mut diagnostics),
                None => {
                    diagnostics.push(Diagnostic::warn("[keybindings] is not a table; ignoring"))
                }
            }
        }
        if let Some(value) = table.get("plugins") {
            match value.as_table() {
                Some(plugins) => read_plugins(plugins, &mut config.plugins, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[plugins] is not a table; ignoring")),
            }
        }
        if let Some(value) = table.get("indexing") {
            match value.as_table() {
                Some(indexing) => read_indexing(indexing, &mut config.indexing, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[indexing] is not a table; ignoring")),
            }
        }
        if let Some(value) = table.get("search") {
            match value.as_table() {
                Some(search) => read_search(search, &mut config.search, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[search] is not a table; ignoring")),
            }
        }
        if let Some(value) = table.get("launcher") {
            match value.as_table() {
                Some(launcher) => read_launcher(launcher, &mut config.launcher, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[launcher] is not a table; ignoring")),
            }
        }
        if let Some(value) = table.get("diagnostics") {
            match value.as_table() {
                Some(diag) => read_diagnostics(diag, &mut config.diagnostics, &mut diagnostics),
                None => {
                    diagnostics.push(Diagnostic::warn("[diagnostics] is not a table; ignoring"))
                }
            }
        }

        warn_unknown_keys(
            table,
            &[
                "schema_version",
                "theme",
                "ui",
                "keybindings",
                "plugins",
                "indexing",
                "search",
                "launcher",
                "diagnostics",
            ],
            "",
            &mut diagnostics,
        );

        (config, diagnostics)
    }

    /// Serialize the merged config as pretty JSON (used by `nohrs config show`).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// The default `config.toml` written on first run or `reset`, including the
    /// `#:schema` header. Every value matches [`Config::default`], so a freshly
    /// written file round-trips to defaults with no diagnostics.
    pub fn default_toml_template() -> String {
        format!(
            "#:schema {SCHEMA_URL}\n\
             schema_version = {CURRENT_SCHEMA_VERSION}\n\
             \n\
             [theme]\n\
             mode = \"system\"   # \"system\" | \"light\" | \"dark\"\n\
             accent = \"blue\"   # named colour or hex (full customization in P5)\n\
             \n\
             [ui]\n\
             default_sort = \"name\"   # \"name\" | \"modified\" | \"size\" | \"kind\"\n\
             show_hidden = false\n\
             icon_pack = \"default\"\n\
             \n\
             # The sections below are parsed and validated, but not yet applied at\n\
             # runtime — they take effect when their subsystem is wired in a later\n\
             # phase (docs/config.md §5). They are here so files and editor\n\
             # completion can adopt the shape early.\n\
             [indexing]\n\
             mode = \"auto\"   # \"auto\" | \"always-on\" | \"manual\"\n\
             \n\
             [indexing.exclude]\n\
             paths = []\n\
             globs = []\n\
             \n\
             [search]\n\
             backend = \"auto\"   # \"sqlite-fts\" | \"tantivy\" | \"ripgrep\" | \"auto\"\n\
             \n\
             [launcher]\n\
             hotkey = \"Cmd+Shift+Space\"\n\
             position_remember = true\n\
             \n\
             # Store performance logging for analysis; all off by default.\n\
             # Output via tracing (RUST_LOG), see docs/persistence.md §5.\n\
             [diagnostics.store]\n\
             log_all_queries = false   # log every SQL query at debug\n\
             slow_query_ms = 0         # warn on SQL slower than this (0 = off)\n\
             log_redb_ops = false      # log redb get/put/delete/batch timings\n\
             \n\
             # Draft sections; defaults are built-in, so these stay empty until\n\
             # keybindings (P3) and plugins (P4) are wired up.\n\
             [keybindings]\n\
             # quit = \"ctrl-q\"\n\
             \n\
             [plugins]\n\
             # core = [\"git\"]\n\
             # community = [\"user/repo\"]\n"
        )
    }
}

fn read_theme(table: &toml::Table, theme: &mut Theme, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("mode") {
        match value.as_str().and_then(ThemeMode::parse) {
            Some(mode) => theme.mode = mode,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid theme.mode {value}; using {:?}",
                theme.mode
            ))),
        }
    }
    if let Some(value) = table.get("accent") {
        match value.as_str() {
            Some(accent) if !accent.trim().is_empty() => theme.accent = accent.to_string(),
            _ => diagnostics.push(Diagnostic::warn(format!(
                "invalid theme.accent {value}; using {:?}",
                theme.accent
            ))),
        }
    }
    warn_unknown_keys(table, &["mode", "accent"], "theme.", diagnostics);
}

fn read_ui(table: &toml::Table, ui: &mut Ui, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("default_sort") {
        match value.as_str().and_then(SortOrder::parse) {
            Some(sort) => ui.default_sort = sort,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid ui.default_sort {value}; using {:?}",
                ui.default_sort
            ))),
        }
    }
    if let Some(value) = table.get("show_hidden") {
        match value.as_bool() {
            Some(flag) => ui.show_hidden = flag,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid ui.show_hidden {value}; using {}",
                ui.show_hidden
            ))),
        }
    }
    if let Some(value) = table.get("icon_pack") {
        match value.as_str() {
            Some(pack) if !pack.trim().is_empty() => ui.icon_pack = pack.to_string(),
            _ => diagnostics.push(Diagnostic::warn(format!(
                "invalid ui.icon_pack {value}; using {:?}",
                ui.icon_pack
            ))),
        }
    }
    warn_unknown_keys(
        table,
        &["default_sort", "show_hidden", "icon_pack"],
        "ui.",
        diagnostics,
    );
}

fn read_keybindings(
    table: &toml::Table,
    keybindings: &mut Keybindings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Draft: accept any action name → chord. Unknown actions are not warned
    // about (the action vocabulary is defined in P3); only non-string values are.
    for (action, value) in table {
        match value.as_str() {
            Some(chord) => {
                keybindings
                    .bindings
                    .insert(action.clone(), chord.to_string());
            }
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid keybindings.{action} {value}; expected a string, ignoring"
            ))),
        }
    }
}

fn read_plugins(table: &toml::Table, plugins: &mut Plugins, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("core") {
        if let Some(core) = read_string_array(value, "plugins.core", diagnostics) {
            plugins.core = core;
        }
    }
    if let Some(value) = table.get("community") {
        if let Some(community) = read_string_array(value, "plugins.community", diagnostics) {
            plugins.community = community;
        }
    }
    warn_unknown_keys(table, &["core", "community"], "plugins.", diagnostics);
}

fn read_indexing(table: &toml::Table, indexing: &mut Indexing, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("mode") {
        match value.as_str().and_then(IndexingMode::parse) {
            Some(mode) => indexing.mode = mode,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid indexing.mode {value}; using {:?}",
                indexing.mode
            ))),
        }
    }
    if let Some(value) = table.get("exclude") {
        match value.as_table() {
            Some(exclude) => read_indexing_exclude(exclude, &mut indexing.exclude, diagnostics),
            None => diagnostics.push(Diagnostic::warn(
                "[indexing.exclude] is not a table; ignoring",
            )),
        }
    }
    warn_unknown_keys(table, &["mode", "exclude"], "indexing.", diagnostics);
}

fn read_indexing_exclude(
    table: &toml::Table,
    exclude: &mut IndexingExclude,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(value) = table.get("paths") {
        if let Some(paths) = read_string_array(value, "indexing.exclude.paths", diagnostics) {
            exclude.paths = paths;
        }
    }
    if let Some(value) = table.get("globs") {
        if let Some(globs) = read_string_array(value, "indexing.exclude.globs", diagnostics) {
            exclude.globs = globs;
        }
    }
    warn_unknown_keys(table, &["paths", "globs"], "indexing.exclude.", diagnostics);
}

fn read_search(table: &toml::Table, search: &mut Search, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("backend") {
        match value.as_str().and_then(SearchBackend::parse) {
            Some(backend) => search.backend = backend,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid search.backend {value}; using {:?}",
                search.backend
            ))),
        }
    }
    warn_unknown_keys(table, &["backend"], "search.", diagnostics);
}

fn read_launcher(table: &toml::Table, launcher: &mut Launcher, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("hotkey") {
        match value.as_str() {
            Some(hotkey) if !hotkey.trim().is_empty() => launcher.hotkey = hotkey.to_string(),
            _ => diagnostics.push(Diagnostic::warn(format!(
                "invalid launcher.hotkey {value}; using {:?}",
                launcher.hotkey
            ))),
        }
    }
    if let Some(value) = table.get("position_remember") {
        match value.as_bool() {
            Some(flag) => launcher.position_remember = flag,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid launcher.position_remember {value}; using {}",
                launcher.position_remember
            ))),
        }
    }
    warn_unknown_keys(
        table,
        &["hotkey", "position_remember"],
        "launcher.",
        diagnostics,
    );
}

/// Read a TOML array of strings, warning about (and skipping) any non-string
/// element. Returns `None` — with a warning — when the value is not an array,
/// so the caller keeps its current value.
fn read_string_array(
    value: &toml::Value,
    field: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<String>> {
    match value.as_array() {
        Some(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(entry) => out.push(entry.to_string()),
                    None => diagnostics.push(Diagnostic::warn(format!(
                        "ignoring non-string entry {item} in {field}"
                    ))),
                }
            }
            Some(out)
        }
        None => {
            diagnostics.push(Diagnostic::warn(format!(
                "{field} is not an array; ignoring"
            )));
            None
        }
    }
}

fn read_diagnostics(
    table: &toml::Table,
    diag: &mut Diagnostics,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(value) = table.get("store") {
        match value.as_table() {
            Some(store) => read_diagnostics_store(store, &mut diag.store, diagnostics),
            None => diagnostics.push(Diagnostic::warn(
                "[diagnostics.store] is not a table; ignoring",
            )),
        }
    }
    warn_unknown_keys(table, &["store"], "diagnostics.", diagnostics);
}

fn read_diagnostics_store(
    table: &toml::Table,
    store: &mut DiagnosticsStore,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(value) = table.get("log_all_queries") {
        match value.as_bool() {
            Some(flag) => store.log_all_queries = flag,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid diagnostics.store.log_all_queries {value}; using {}",
                store.log_all_queries
            ))),
        }
    }
    if let Some(value) = table.get("slow_query_ms") {
        match value.as_integer() {
            Some(ms) if ms >= 0 => store.slow_query_ms = ms as u64,
            _ => diagnostics.push(Diagnostic::warn(format!(
                "invalid diagnostics.store.slow_query_ms {value}; using {}",
                store.slow_query_ms
            ))),
        }
    }
    if let Some(value) = table.get("log_redb_ops") {
        match value.as_bool() {
            Some(flag) => store.log_redb_ops = flag,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid diagnostics.store.log_redb_ops {value}; using {}",
                store.log_redb_ops
            ))),
        }
    }
    warn_unknown_keys(
        table,
        &["log_all_queries", "slow_query_ms", "log_redb_ops"],
        "diagnostics.store.",
        diagnostics,
    );
}

/// Emit a warning for every key in `table` that is not a known key. `prefix` is
/// prepended for nested tables (e.g. `"theme."`) so messages point at the
/// offending path.
fn warn_unknown_keys(
    table: &toml::Table,
    known: &[&str],
    prefix: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            diagnostics.push(Diagnostic::warn(format!(
                "unknown config key {prefix}{key}; ignoring"
            )));
        }
    }
}

/// Generate the JSON Schema for [`Config`] as pretty JSON. Backs both
/// `nohrs config schema` and the committed `docs/config.schema.json`.
///
/// The schema is normalized so the output is portable and byte-for-byte stable:
///
/// * Object keys are sorted, so the result does not depend on whether
///   `serde_json`'s `preserve_order` feature is enabled (it is in the GUI binary
///   but not in a `nohrs-core`-only build, via Cargo feature unification);
///   otherwise the committed file and `nohrs config schema` would disagree.
/// * Schemars' Rust-specific integer `format`s (e.g. `uint64`) are dropped:
///   they are not standard JSON Schema draft 2020-12 formats, and `type` +
///   `minimum` already capture the constraint. Standard string formats are kept.
///
/// There are no `required` arrays: every field is `#[serde(default)]`, so the
/// loader (and a conforming editor) accepts a file that omits any section.
/// Array element order (enum variants) is left untouched.
pub fn json_schema_string() -> serde_json::Result<String> {
    let schema = schemars::schema_for!(Config);
    let value = serde_json::to_value(&schema)?;
    serde_json::to_string_pretty(&normalize_schema(value))
}

/// Rust integer `format` values schemars emits that are not standard JSON Schema.
const RUST_INT_FORMATS: &[&str] = &[
    "uint", "uint8", "uint16", "uint32", "uint64", "uint128", "int", "int8", "int16", "int32",
    "int64", "int128",
];

fn normalize_schema(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut normalized = serde_json::Map::new();
            for (key, child) in entries {
                if key == "format"
                    && child
                        .as_str()
                        .is_some_and(|format| RUST_INT_FORMATS.contains(&format))
                {
                    continue;
                }
                normalized.insert(key, normalize_schema(child));
            }
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(normalize_schema).collect())
        }
        other => other,
    }
}

/// Read `path` and parse it, returning [`Config::default`] with no diagnostics
/// when the file does not exist (a missing config is normal, not an error).
pub fn load_from_path(path: &Path) -> (Config, Vec<Diagnostic>) {
    // Config is read synchronously at startup, before the GPUI foreground loop
    // runs, so blocking I/O here cannot stall rendering.
    #[allow(clippy::disallowed_methods)]
    match std::fs::read_to_string(path) {
        Ok(contents) => Config::from_toml_str(&contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (Config::default(), Vec::new())
        }
        Err(error) => (
            Config::default(),
            vec![Diagnostic::error(format!(
                "could not read {}: {error}",
                path.display()
            ))],
        ),
    }
}

/// Log all diagnostics and return the first error message (if any) so the caller
/// can surface it in the UI. Warnings are logged but never returned.
pub fn report_diagnostics(diagnostics: &[Diagnostic]) -> Option<String> {
    let mut first_error = None;
    for diagnostic in diagnostics {
        match diagnostic.level {
            DiagnosticLevel::Warning => tracing::warn!("config: {}", diagnostic.message),
            DiagnosticLevel::Error => {
                tracing::error!("config: {}", diagnostic.message);
                if first_error.is_none() {
                    first_error = Some(diagnostic.message.clone());
                }
            }
        }
    }
    first_error
}

#[cfg(test)]
// Rust 2024 made `std::env::set_var` unsafe; these tests must mutate the
// process environment to exercise env-var config overrides.
#[allow(clippy::unwrap_used, clippy::disallowed_methods, unsafe_code)]
mod tests {
    use super::*;
    use crate::config::test_env::env_lock;

    fn errors(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .map(|d| d.message.as_str())
            .collect()
    }

    #[test]
    fn defaults_match_spec() {
        let config = Config::default();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.theme.mode, ThemeMode::System);
        assert_eq!(config.theme.accent, "blue");
        assert_eq!(config.ui.default_sort, SortOrder::Name);
        assert!(!config.ui.show_hidden);
        assert_eq!(config.ui.icon_pack, "default");
    }

    #[test]
    fn parses_full_config() {
        let toml = r##"
            schema_version = 1
            [theme]
            mode = "dark"
            accent = "#ff8800"
            [ui]
            default_sort = "modified"
            show_hidden = true
            icon_pack = "nerd"
        "##;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(config.theme.mode, ThemeMode::Dark);
        assert_eq!(config.theme.accent, "#ff8800");
        assert_eq!(config.ui.default_sort, SortOrder::Modified);
        assert!(config.ui.show_hidden);
        assert_eq!(config.ui.icon_pack, "nerd");
    }

    #[test]
    fn parses_diagnostics_store() {
        let toml = r#"
            [diagnostics.store]
            log_all_queries = true
            slow_query_ms = 50
            log_redb_ops = true
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(config.diagnostics.store.log_all_queries);
        assert_eq!(config.diagnostics.store.slow_query_ms, 50);
        assert!(config.diagnostics.store.log_redb_ops);
    }

    #[test]
    fn parses_p2_sections() {
        let toml = r#"
            [indexing]
            mode = "always-on"
            [indexing.exclude]
            paths = ["/tmp", "node_modules"]
            globs = ["**/target/**"]
            [search]
            backend = "tantivy"
            [launcher]
            hotkey = "Ctrl+Space"
            position_remember = false
            [plugins]
            core = ["git", "calculator"]
            community = ["user/repo"]
            [keybindings]
            quit = "ctrl-q"
            search = "ctrl-f"
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(config.indexing.mode, IndexingMode::AlwaysOn);
        assert_eq!(config.indexing.exclude.paths, ["/tmp", "node_modules"]);
        assert_eq!(config.indexing.exclude.globs, ["**/target/**"]);
        assert_eq!(config.search.backend, SearchBackend::Tantivy);
        assert_eq!(config.launcher.hotkey, "Ctrl+Space");
        assert!(!config.launcher.position_remember);
        assert_eq!(config.plugins.core, ["git", "calculator"]);
        assert_eq!(config.plugins.community, ["user/repo"]);
        assert_eq!(
            config.keybindings.bindings.get("quit").map(String::as_str),
            Some("ctrl-q")
        );
        assert_eq!(
            config
                .keybindings
                .bindings
                .get("search")
                .map(String::as_str),
            Some("ctrl-f")
        );
    }

    #[test]
    fn p2_sections_reject_bad_values_leniently() {
        let toml = r#"
            [indexing]
            mode = "turbo"
            [indexing.exclude]
            paths = "not-an-array"
            globs = [1, "ok"]
            [search]
            backend = "grep"
            [launcher]
            hotkey = ""
            position_remember = "yes"
            [plugins]
            core = "git"
            [keybindings]
            quit = 7
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        // No fatal errors: bad fields fall back to defaults.
        assert!(errors(&diagnostics).is_empty());
        assert_eq!(config.indexing.mode, IndexingMode::Auto);
        assert!(config.indexing.exclude.paths.is_empty());
        // The non-string array entry is skipped; the valid one is kept.
        assert_eq!(config.indexing.exclude.globs, ["ok"]);
        assert_eq!(config.search.backend, SearchBackend::Auto);
        assert_eq!(config.launcher.hotkey, Config::default().launcher.hotkey);
        assert!(config.launcher.position_remember);
        assert!(config.plugins.core.is_empty());
        assert!(config.keybindings.bindings.is_empty());
        // mode, paths(not array), globs(one bad entry), backend, hotkey,
        // position_remember, plugins.core(not array), keybindings.quit.
        assert_eq!(diagnostics.len(), 8);
    }

    #[test]
    fn diagnostics_store_rejects_bad_values_leniently() {
        let toml = r#"
            [diagnostics.store]
            slow_query_ms = -5
            log_all_queries = "yes"
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(errors(&diagnostics).is_empty());
        // Bad values fall back to defaults.
        assert_eq!(config.diagnostics.store.slow_query_ms, 0);
        assert!(!config.diagnostics.store.log_all_queries);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn syntax_error_falls_back_without_panicking() {
        let (config, diagnostics) = Config::from_toml_str("this = = broken");
        assert_eq!(config, Config::default());
        assert_eq!(errors(&diagnostics).len(), 1);
    }

    #[test]
    fn out_of_range_value_warns_and_keeps_default() {
        let toml = r#"
            [theme]
            mode = "neon"
            [ui]
            default_sort = 42
            show_hidden = "yes"
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        // No errors: the file still loads.
        assert!(errors(&diagnostics).is_empty());
        // Bad fields fall back to defaults; others are untouched.
        assert_eq!(config.theme.mode, ThemeMode::System);
        assert_eq!(config.ui.default_sort, SortOrder::Name);
        assert!(!config.ui.show_hidden);
        // Three warnings: mode, default_sort, show_hidden.
        assert_eq!(diagnostics.len(), 3);
    }

    #[test]
    fn unknown_keys_warn_but_load() {
        let toml = r#"
            mystery = 1
            [theme]
            mode = "light"
            shade = "x"
            [keybindings]
            quit = "ctrl-q"
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(errors(&diagnostics).is_empty());
        assert_eq!(config.theme.mode, ThemeMode::Light);
        // `mystery` and `theme.shade` warn; `[keybindings]` is reserved and silent.
        let warnings: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(warnings.iter().any(|m| m.contains("mystery")));
        assert!(warnings.iter().any(|m| m.contains("theme.shade")));
        assert!(!warnings.iter().any(|m| m.contains("keybindings")));
    }

    #[test]
    fn future_schema_version_falls_back() {
        let (config, diagnostics) = Config::from_toml_str("schema_version = 99");
        assert_eq!(config, Config::default());
        assert_eq!(errors(&diagnostics).len(), 1);
        assert!(errors(&diagnostics)[0].contains("newer version"));
    }

    #[test]
    fn env_layer_overrides_file() {
        let (mut config, _) = Config::from_toml_str("[theme]\nmode = \"light\"");
        config.apply_override(&ConfigOverride {
            theme_mode: Some(ThemeMode::Dark),
            ..Default::default()
        });
        assert_eq!(config.theme.mode, ThemeMode::Dark);
    }

    #[test]
    fn override_is_sparse() {
        let mut config = Config::default();
        config.theme.accent = "green".into();
        config.apply_override(&ConfigOverride {
            ui_show_hidden: Some(true),
            ..Default::default()
        });
        // Only the provided field changes.
        assert!(config.ui.show_hidden);
        assert_eq!(config.theme.accent, "green");
    }

    #[test]
    fn parse_bool_accepts_common_spellings() {
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn default_template_round_trips() {
        let (config, diagnostics) = Config::from_toml_str(&Config::default_toml_template());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn json_schema_is_generatable() {
        let schema = json_schema_string().unwrap();
        assert!(schema.contains("schema_version"));
        assert!(schema.contains("default_sort"));
    }

    #[test]
    fn schema_has_no_required_so_partial_configs_validate() {
        // The loader defaults every field (it overlays a parsed table onto
        // `Config::default`), so no section is required. The schema must agree,
        // otherwise editors reject files the app accepts.
        let schema = json_schema_string().unwrap();
        assert!(
            !schema.contains("\"required\""),
            "schema must not mark any field required"
        );
        let (config, diagnostics) = Config::from_toml_str("");
        assert_eq!(config, Config::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn committed_schema_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/config.schema.json");
        let committed = std::fs::read_to_string(path).unwrap();
        let generated = json_schema_string().unwrap();
        assert_eq!(
            committed.trim_end(),
            generated.trim_end(),
            "docs/config.schema.json is stale; regenerate with `nohrs config schema > docs/config.schema.json`"
        );
    }

    // Snapshot of the field-by-field parser output, including the warning emitted
    // for an invalid value. Review/refresh with `cargo insta review` when the
    // parser's behavior intentionally changes.
    #[test]
    fn from_toml_str_output_snapshot() {
        let toml = r#"
            schema_version = 1
            [theme]
            mode = "neon"
            accent = "teal"
            [ui]
            show_hidden = true
            unknown_key = 1
        "#;
        let parsed = Config::from_toml_str(toml);
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn theme_mode_parse_accepts_only_known_spellings() {
        assert_eq!(ThemeMode::parse("system"), Some(ThemeMode::System));
        assert_eq!(ThemeMode::parse("light"), Some(ThemeMode::Light));
        assert_eq!(ThemeMode::parse("dark"), Some(ThemeMode::Dark));
        assert_eq!(ThemeMode::parse("Dark"), None);
        assert_eq!(ThemeMode::parse(""), None);
        assert_eq!(ThemeMode::parse("neon"), None);
    }

    #[test]
    fn sort_order_parse_accepts_only_known_spellings() {
        assert_eq!(SortOrder::parse("name"), Some(SortOrder::Name));
        assert_eq!(SortOrder::parse("modified"), Some(SortOrder::Modified));
        assert_eq!(SortOrder::parse("size"), Some(SortOrder::Size));
        assert_eq!(SortOrder::parse("kind"), Some(SortOrder::Kind));
        assert_eq!(SortOrder::parse("Size"), None);
        assert_eq!(SortOrder::parse("created"), None);
    }

    #[test]
    fn empty_config_is_defaults_without_diagnostics() {
        let (config, diagnostics) = Config::from_toml_str("");
        assert_eq!(config, Config::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn only_schema_version_loads_clean() {
        let (config, diagnostics) = Config::from_toml_str("schema_version = 1");
        assert_eq!(config, Config::default());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn non_table_sections_warn_and_keep_defaults() {
        let (config, diagnostics) = Config::from_toml_str("theme = \"x\"\nui = 1");
        assert_eq!(config, Config::default());
        let warnings: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(warnings.iter().any(|m| m.contains("[theme]")));
        assert!(warnings.iter().any(|m| m.contains("[ui]")));
    }

    #[test]
    fn empty_accent_warns_and_keeps_default() {
        let toml = "[theme]\naccent = \"\"";
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert_eq!(config.theme.accent, Config::default().theme.accent);
        assert!(diagnostics.iter().any(|d| d.message.contains("accent")));
    }

    #[test]
    fn apply_override_sets_every_field() {
        let mut config = Config::default();
        config.apply_override(&ConfigOverride {
            theme_mode: Some(ThemeMode::Dark),
            theme_accent: Some("crimson".into()),
            ui_default_sort: Some(SortOrder::Size),
            ui_show_hidden: Some(true),
            ui_icon_pack: Some("nerd".into()),
        });
        assert_eq!(config.theme.mode, ThemeMode::Dark);
        assert_eq!(config.theme.accent, "crimson");
        assert_eq!(config.ui.default_sort, SortOrder::Size);
        assert!(config.ui.show_hidden);
        assert_eq!(config.ui.icon_pack, "nerd");
    }

    #[test]
    fn empty_override_is_a_no_op() {
        let mut config = Config::default();
        let before = config.clone();
        config.apply_override(&ConfigOverride::default());
        assert_eq!(config, before);
    }

    #[test]
    fn to_json_round_trips_to_equal_config() {
        let config = Config::default();
        let json = config.to_json().unwrap();
        assert!(json.contains("schema_version"));
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn from_env_reads_valid_vars() {
        let _guard = env_lock();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_THEME", "dark") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_ACCENT", "violet") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_DEFAULT_SORT", "size") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_SHOW_HIDDEN", "on") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_ICON_PACK", "nerd") };
        let over = ConfigOverride::from_env();
        for key in [
            "NOHRS_THEME",
            "NOHRS_ACCENT",
            "NOHRS_DEFAULT_SORT",
            "NOHRS_SHOW_HIDDEN",
            "NOHRS_ICON_PACK",
        ] {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::remove_var(key) };
        }
        assert_eq!(over.theme_mode, Some(ThemeMode::Dark));
        assert_eq!(over.theme_accent.as_deref(), Some("violet"));
        assert_eq!(over.ui_default_sort, Some(SortOrder::Size));
        assert_eq!(over.ui_show_hidden, Some(true));
        assert_eq!(over.ui_icon_pack.as_deref(), Some("nerd"));
    }

    #[test]
    fn from_env_skips_invalid_values() {
        let _guard = env_lock();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_THEME", "neon") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("NOHRS_SHOW_HIDDEN", "maybe") };
        let over = ConfigOverride::from_env();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("NOHRS_THEME") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("NOHRS_SHOW_HIDDEN") };
        assert_eq!(over.theme_mode, None);
        assert_eq!(over.ui_show_hidden, None);
    }
}
