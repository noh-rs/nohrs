//! The `noh` binary: picks the entry point from the program name (see
//! `nohrs_cli::Invocation`) and runs the requested command.

use std::io::{self, Write};
use std::process::ExitCode;

use nohrs_cli::Invocation;
use nohrs_core::telemetry::logging::{FileLogConfig, init_logging_with_file};

fn main() -> ExitCode {
    // Parsed before the subscriber is installed, because `noh log` is the
    // *reader* of the log and must not write one: opening the sink first makes
    // `show` report on a file it created a moment earlier, and hands `clear`
    // the file this very process is appending to. Every other command records
    // what it did, into the same rolling file the GUI writes, so `noh log`
    // shows both sides.
    let invocation = Invocation::parse_from(std::env::args_os());
    let config = FileLogConfig {
        enabled: !invocation.reads_the_log(),
        ..FileLogConfig::default()
    };
    // Bound to a name, not `_`: dropping the guard would stop the writer before
    // the command has run.
    let _log_guard = init_logging_with_file(&config);
    match invocation.run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => report_failure(&error),
    }
}

// `Invocation::run` only fails when writing its own report failed — a closed
// pipe (`noh rm -v ... | head`), typically. Announcing that can fail for
// the very same reason, so the diagnostic is best effort and the exit code is
// what actually carries the outcome.
fn report_failure(error: &io::Error) -> ExitCode {
    match writeln!(io::stderr(), "noh: {error}") {
        Ok(()) | Err(_) => ExitCode::FAILURE,
    }
}
