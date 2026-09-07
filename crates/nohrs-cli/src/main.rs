//! The `noh` binary: picks the entry point from the program name (see
//! `nohrs_cli::Invocation`) and runs the requested command.

use std::io::{self, Write};
use std::process::ExitCode;

use nohrs_cli::Invocation;

fn main() -> ExitCode {
    match Invocation::parse_from(std::env::args_os()).run() {
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
