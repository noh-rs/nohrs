//! Command-line interface: argument parsing plus the `nohrs config` subcommands
//! (config.md §8). With no subcommand the parsed global overrides are handed to
//! the GUI; with a `config` subcommand we run it and exit without opening a
//! window.

use clap::{Parser, Subcommand};
use nohrs_core::config::{self, ConfigOverride, ThemeMode};
use std::process::Command as ProcessCommand;

#[derive(Parser, Debug)]
#[command(name = "nohrs", version, about = "Launcher × Explorer file workspace")]
pub struct Cli {
    /// Override the theme mode for this run: system, light, or dark.
    #[arg(long, global = true, value_name = "MODE")]
    pub theme: Option<String>,

    /// Override the accent colour for this run (named colour or hex).
    #[arg(long, global = true, value_name = "COLOR")]
    pub accent: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Inspect or manage the user configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print the fully merged config (defaults < file < env < CLI) as JSON.
    Show,
    /// Open config.toml in $EDITOR (creating it with defaults if missing).
    Edit,
    /// Validate config.toml, reporting parse errors and warnings.
    Validate,
    /// Print the JSON Schema for config.toml to stdout.
    Schema,
    /// Print the full path to config.toml.
    Path,
    /// Reset config.toml to defaults, backing up any existing file first.
    Reset,
}

impl Cli {
    /// Build the CLI override layer from the global `--theme` / `--accent` flags.
    /// An invalid `--theme` value is warned about and ignored rather than
    /// aborting (it still leaves the file/env value in effect).
    pub fn overrides(&self) -> ConfigOverride {
        let mut over = ConfigOverride::default();
        if let Some(theme) = &self.theme {
            match ThemeMode::parse(theme.trim()) {
                Some(mode) => over.theme_mode = Some(mode),
                None => tracing::warn!("ignoring invalid --theme {theme:?}"),
            }
        }
        if let Some(accent) = &self.accent {
            over.theme_accent = Some(accent.clone());
        }
        over
    }
}

/// Run a `config` subcommand. Returns the process exit code.
pub fn run_config_command(action: &ConfigAction, cli: &Cli) -> i32 {
    let path = config::paths::config_file();
    match action {
        ConfigAction::Path => {
            println!("{}", path.display());
            0
        }
        ConfigAction::Schema => match config::json_schema_string() {
            Ok(schema) => {
                println!("{schema}");
                0
            }
            Err(error) => {
                eprintln!("failed to generate schema: {error}");
                1
            }
        },
        ConfigAction::Show => {
            let (mut merged, diagnostics) = config::load_from_path(&path);
            config::report_diagnostics(&diagnostics);
            merged.apply_override(&ConfigOverride::from_env());
            merged.apply_override(&cli.overrides());
            match merged.to_json() {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(error) => {
                    eprintln!("failed to serialize config: {error}");
                    1
                }
            }
        }
        ConfigAction::Validate => {
            let (_, diagnostics) = config::load_from_path(&path);
            let error = config::report_diagnostics(&diagnostics);
            if let Some(message) = error {
                eprintln!("invalid config ({}): {message}", path.display());
                1
            } else {
                let warnings = diagnostics.len();
                if warnings == 0 {
                    println!("{}: OK", path.display());
                } else {
                    println!(
                        "{}: OK with {warnings} warning(s) (see logs)",
                        path.display()
                    );
                }
                0
            }
        }
        ConfigAction::Reset => match config::reset(&path) {
            Ok(Some(backup)) => {
                println!(
                    "reset {}; backed up to {}",
                    path.display(),
                    backup.display()
                );
                0
            }
            Ok(None) => {
                println!("wrote default config to {}", path.display());
                0
            }
            Err(error) => {
                eprintln!("failed to reset config: {error}");
                1
            }
        },
        ConfigAction::Edit => edit_config(&path),
    }
}

fn edit_config(path: &std::path::Path) -> i32 {
    if let Err(error) = config::ensure_exists(path) {
        eprintln!("failed to create {}: {error}", path.display());
        return 1;
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // `$EDITOR` often carries arguments (e.g. `code --wait`), so split on
    // whitespace rather than treating the whole string as a program path.
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    match ProcessCommand::new(program).args(parts).arg(path).status() {
        // A signal-terminated child reports no exit code; surface that as a
        // failure rather than masking it as success.
        Ok(status) if status.success() => 0,
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("failed to launch editor {editor:?}: {error}");
            1
        }
    }
}
