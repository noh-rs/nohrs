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
use std::path::Path;

/// Schema version understood by this build. Bumped only when the on-disk
/// schema changes in a way that requires migration.
pub const CURRENT_SCHEMA_VERSION: u64 = 1;

/// URL advertised in the `#:schema` header of `config.toml` so TOML-aware
/// editors can offer completion and validation.
pub const SCHEMA_URL: &str = "https://nohrs.app/schema/config.schema.json";

/// The fully merged, validated configuration.
///
/// Models the P1 surface (`theme` / `ui`) plus the P2 `[diagnostics]` section.
/// `[keybindings]` and `[plugins]` are recognised as reserved sections (see
/// [`is_reserved_section`]) but intentionally not deserialized yet, so their
/// future shapes can be added without breaking older files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    /// On-disk schema version. Must be [`CURRENT_SCHEMA_VERSION`] for this build.
    pub schema_version: u64,
    /// Appearance settings.
    pub theme: Theme,
    /// Explorer / view settings.
    pub ui: Ui,
    /// Split-view and pane settings.
    pub explorer: Explorer,
    /// Diagnostics / performance-logging settings.
    pub diagnostics: Diagnostics,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: Theme::default(),
            ui: Ui::default(),
            explorer: Explorer::default(),
            diagnostics: Diagnostics::default(),
        }
    }
}

/// Appearance settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Split-view and pane settings (see `docs/explorer-essentials.md` §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Explorer {
    /// Orientation a fresh split uses unless overridden by the split shortcut.
    pub split_direction: SplitDirection,
    /// When enabled, navigating in one pane mirrors the path into the others.
    pub synced_panes: bool,
}

impl Default for Explorer {
    fn default() -> Self {
        Self {
            split_direction: SplitDirection::Vertical,
            synced_panes: false,
        }
    }
}

/// How a 2-way split arranges its panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    /// Panes side by side, separated by a vertical divider (`Cmd+\`).
    #[default]
    Vertical,
    /// Panes stacked, separated by a horizontal divider (`Cmd+Shift+\`).
    Horizontal,
}

impl SplitDirection {
    /// Parse the `config.toml` spelling (`vertical`/`horizontal`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "vertical" => Some(Self::Vertical),
            "horizontal" => Some(Self::Horizontal),
            _ => None,
        }
    }
}

/// Diagnostics / performance-logging settings (see `docs/persistence.md` §5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostics {
    /// Performance logging for the persistence layer (`nohrs-store`).
    pub store: DiagnosticsStore,
}

/// Performance logging for the SQLite / redb store. All off by default, so a
/// default config adds no overhead. Output goes through `tracing` (targets
/// `nohrs_store::sql` / `nohrs_store::redb`) and is filterable with `RUST_LOG`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
        if let Some(value) = table.get("explorer") {
            match value.as_table() {
                Some(explorer) => read_explorer(explorer, &mut config.explorer, &mut diagnostics),
                None => diagnostics.push(Diagnostic::warn("[explorer] is not a table; ignoring")),
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
                "explorer",
                "diagnostics",
                "keybindings",
                "plugins",
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
    /// `#:schema` header and empty reserved sections.
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
             [explorer]\n\
             split_direction = \"vertical\"   # \"vertical\" (left/right) | \"horizontal\" (top/bottom)\n\
             synced_panes = false            # when true, panes mirror the same path\n\
             \n\
             # Store performance logging for analysis; all off by default.\n\
             # Output via tracing (RUST_LOG), see docs/persistence.md §5.\n\
             [diagnostics.store]\n\
             log_all_queries = false   # log every SQL query at debug\n\
             slow_query_ms = 0         # warn on SQL slower than this (0 = off)\n\
             log_redb_ops = false      # log redb get/put/delete/batch timings\n\
             \n\
             # Reserved for future use; left empty in P1.\n\
             [keybindings]\n\
             \n\
             [plugins]\n"
        )
    }
}

/// Whether `name` is a reserved top-level section that P1 recognises but does
/// not yet read (so its presence is not flagged as an unknown key).
pub fn is_reserved_section(name: &str) -> bool {
    matches!(name, "keybindings" | "plugins")
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

fn read_explorer(table: &toml::Table, explorer: &mut Explorer, diagnostics: &mut Vec<Diagnostic>) {
    if let Some(value) = table.get("split_direction") {
        match value.as_str().and_then(SplitDirection::parse) {
            Some(direction) => explorer.split_direction = direction,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid explorer.split_direction {value}; using {:?}",
                explorer.split_direction
            ))),
        }
    }
    if let Some(value) = table.get("synced_panes") {
        match value.as_bool() {
            Some(flag) => explorer.synced_panes = flag,
            None => diagnostics.push(Diagnostic::warn(format!(
                "invalid explorer.synced_panes {value}; using {}",
                explorer.synced_panes
            ))),
        }
    }
    warn_unknown_keys(
        table,
        &["split_direction", "synced_panes"],
        "explorer.",
        diagnostics,
    );
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

/// Emit a warning for every key in `table` that is neither a known key nor a
/// reserved top-level section. `prefix` is prepended for nested tables (e.g.
/// `"theme."`) so messages point at the offending path.
fn warn_unknown_keys(
    table: &toml::Table,
    known: &[&str],
    prefix: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for key in table.keys() {
        let is_known = known.contains(&key.as_str());
        let is_reserved = prefix.is_empty() && is_reserved_section(key);
        if !is_known && !is_reserved {
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
/// Array element order (enum variants, `required`) is left untouched.
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
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

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
    fn explorer_section_parses_direction_and_sync() {
        let toml = r#"
            [explorer]
            split_direction = "horizontal"
            synced_panes = true
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(config.explorer.split_direction, SplitDirection::Horizontal);
        assert!(config.explorer.synced_panes);
    }

    #[test]
    fn explorer_section_rejects_bad_values_leniently() {
        let toml = r#"
            [explorer]
            split_direction = "diagonal"
            synced_panes = "maybe"
        "#;
        let (config, diagnostics) = Config::from_toml_str(toml);
        assert!(errors(&diagnostics).is_empty());
        // Bad values fall back to defaults.
        assert_eq!(config.explorer.split_direction, SplitDirection::Vertical);
        assert!(!config.explorer.synced_panes);
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

    // `NOHRS_*` env tests mutate process-global state; serialize them so parallel
    // test threads do not observe each other's vars. Recover from a poisoned
    // lock so a panicking env test surfaces its own assertion rather than an
    // opaque poison panic in every later env test.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn from_env_reads_valid_vars() {
        let _guard = env_lock();
        std::env::set_var("NOHRS_THEME", "dark");
        std::env::set_var("NOHRS_ACCENT", "violet");
        std::env::set_var("NOHRS_DEFAULT_SORT", "size");
        std::env::set_var("NOHRS_SHOW_HIDDEN", "on");
        std::env::set_var("NOHRS_ICON_PACK", "nerd");
        let over = ConfigOverride::from_env();
        for key in [
            "NOHRS_THEME",
            "NOHRS_ACCENT",
            "NOHRS_DEFAULT_SORT",
            "NOHRS_SHOW_HIDDEN",
            "NOHRS_ICON_PACK",
        ] {
            std::env::remove_var(key);
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
        std::env::set_var("NOHRS_THEME", "neon");
        std::env::set_var("NOHRS_SHOW_HIDDEN", "maybe");
        let over = ConfigOverride::from_env();
        std::env::remove_var("NOHRS_THEME");
        std::env::remove_var("NOHRS_SHOW_HIDDEN");
        assert_eq!(over.theme_mode, None);
        assert_eq!(over.ui_show_hidden, None);
    }
}
