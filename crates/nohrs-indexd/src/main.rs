//! The `nohrs-indexd` binary: process wiring only.
//!
//! Started by whoever wants the index kept up to date, not by the system. It
//! leaves the terminal it was started from, does its work, and exits once its
//! last client has been gone for the grace period.

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use nohrs_core::telemetry::logging::{FileLogConfig, init_logging_with_file};
use nohrs_indexd::server::Settings;
use nohrs_indexd::{Endpoint, lease::DEFAULT_GRACE};

/// Command-line entry point.
#[derive(Parser, Debug)]
#[command(
    name = "nohrs-indexd",
    version,
    about = "Keeps the nohrs search index level with the filesystem"
)]
struct Cli {
    /// Directory holding the socket and its lock file. Defaults to the
    /// session's runtime directory.
    #[arg(long, value_name = "DIR")]
    endpoint_dir: Option<PathBuf>,

    /// Seconds to stay up after the last client leaves.
    #[arg(long, value_name = "SECONDS")]
    grace: Option<u64>,

    /// The index to write. Defaults to the one the app reads.
    #[arg(long, value_name = "DIR", requires = "content_root")]
    index_dir: Option<PathBuf>,

    /// The tree to index. Defaults to the one the app covers.
    #[arg(long, value_name = "DIR", requires = "index_dir")]
    content_root: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // The same rolling file the app and the CLI write, so one `noh log show`
    // covers all three. There is no terminal to write to: this process is
    // started in the background and its stdio is closed.
    let _log_guard = init_logging_with_file(&FileLogConfig::default());

    let endpoint = match cli.endpoint_dir {
        Some(directory) => Endpoint::under(directory),
        None => Endpoint::for_session(),
    };
    let settings = Settings {
        grace: cli.grace.map_or(DEFAULT_GRACE, Duration::from_secs),
        index: cli.index_dir.zip(cli.content_root),
    };

    detach();
    match run(&endpoint, &settings) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("the index daemon stopped: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(unix)]
fn run(endpoint: &Endpoint, settings: &Settings) -> anyhow::Result<()> {
    use nohrs_indexd::server::{self, Outcome};

    match server::serve(endpoint, settings)? {
        Outcome::Served => tracing::info!("the index daemon has stopped"),
        // Losing the race to start is ordinary: two clients wanted a daemon at
        // the same moment and one of them got there first.
        Outcome::AlreadyRunning => tracing::debug!("another index daemon is already running"),
    }
    Ok(())
}

#[cfg(not(unix))]
fn run(_endpoint: &Endpoint, _settings: &Settings) -> anyhow::Result<()> {
    anyhow::bail!("the index daemon needs unix sockets; index in-process instead")
}

/// Leaves the session that started this process.
///
/// Without it the daemon keeps the terminal's controlling session, and a
/// `Ctrl-C` aimed at the command that happened to start it would stop the
/// daemon too. Best effort: failing means the daemon is already a session
/// leader, which is the state this asks for.
#[cfg(unix)]
fn detach() {
    if let Err(error) = rustix::process::setsid() {
        tracing::debug!("could not start a new session: {error}");
    }
}

#[cfg(not(unix))]
fn detach() {}
