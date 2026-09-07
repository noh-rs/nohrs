//! The `noh` binary: picks the entry point from the program name (see
//! [`nohrs_cli::Invocation`]) and runs the requested command.
//!
//! Everything here is process wiring — locking the real stdio, opening the real
//! trash, reading the real environment. The parsing and the command engines
//! themselves live in the library, where they are exercised against fakes.

use std::io::{self, Write};
use std::process::ExitCode;

use nohrs_cli::{Cli, Command, Invocation, RmCli, doctor, ledger, rm, shim, trash};

fn main() -> ExitCode {
    match run(&Invocation::parse_from(std::env::args_os())) {
        Ok(code) => ExitCode::from(code),
        Err(error) => report_failure(&error),
    }
}

/// Run the parsed command against the real filesystem, returning the process
/// exit code. The error case is a failure to write to stdout/stderr.
fn run(invocation: &Invocation) -> io::Result<u8> {
    match invocation {
        Invocation::Rm(RmCli { args }) => run_rm(args),
        Invocation::Direct(Cli { command }) => run_command(command),
    }
}

fn run_command(command: &Command) -> io::Result<u8> {
    match command {
        Command::Rm(args) => run_rm(args),
        Command::Restore(args) => run_trash(|session| session.restore(args)),
        Command::Trash(trash::Command::List(args)) => run_trash(|session| session.list(args)),
        Command::Trash(trash::Command::Purge(args)) => run_trash(|session| session.purge(args)),
        Command::Trash(trash::Command::Empty(args)) => {
            let args = args.as_purge();
            run_trash(|session| session.purge(&args))
        }
        Command::Doctor => {
            let checks = doctor::check(&doctor::Environment::detect());
            doctor::report(&checks, &mut io::stdout().lock())
        }
        Command::Shim(command) => run_shim(command),
        Command::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut Invocation::command(),
                "noh",
                &mut io::stdout().lock(),
            );
            Ok(0)
        }
    }
}

fn run_rm(args: &rm::Args) -> io::Result<u8> {
    let mut output = io::stdout().lock();
    let mut errors = io::stderr().lock();
    // A ledger that will not open is not a reason to refuse to delete — `rm`
    // has to work — but it does cost the ability to restore, so say so instead
    // of quietly dropping the record.
    let ledger = match ledger::open_if_needed() {
        Ok(ledger) => ledger,
        Err(error) => {
            writeln!(
                errors,
                "noh rm: the trash ledger could not be opened ({}); these deletions will not be restorable",
                nohrs_cli::message(&error)
            )?;
            None
        }
    };
    let mut backend = rm::OsBackend::new(ledger);
    let mut confirm = rm::StdinConfirm;
    let summary =
        rm::Session::new(args, &mut backend, &mut confirm, &mut output, &mut errors).run()?;
    Ok(summary.exit_code())
}

/// Open the platform's trash store and run one command against it. Opening is
/// what can fail here (an unreadable data directory); the command itself reports
/// per-item failures through its own summary.
fn run_trash<F>(command: F) -> io::Result<u8>
where
    F: FnOnce(trash::Session<'_>) -> io::Result<trash::Summary>,
{
    let mut output = io::stdout().lock();
    let mut errors = io::stderr().lock();
    let mut store = match nohrs_services::fs::trash::default_store(ledger::open) {
        Ok(store) => store,
        Err(error) => {
            writeln!(errors, "noh: {}", nohrs_cli::message(&error))?;
            return Ok(1);
        }
    };
    let mut confirm = rm::StdinConfirm;
    let session = trash::Session::new(store.as_mut(), &mut confirm, &mut output, &mut errors);
    Ok(command(session)?.exit_code())
}

fn run_shim(command: &shim::Command) -> io::Result<u8> {
    let mut output = io::stdout().lock();
    let mut errors = io::stderr().lock();
    let dir = match command {
        shim::Command::Install(args) => args.dir.clone(),
        shim::Command::Uninstall(args) => args.dir.clone(),
        shim::Command::Status(args) => args.dir.clone(),
    };
    let context = match shim::Context::detect(dir) {
        Ok(context) => context,
        Err(error) => {
            writeln!(errors, "noh shim: {}", nohrs_cli::message(&error))?;
            return Ok(1);
        }
    };
    let summary = match command {
        shim::Command::Install(args) => shim::install(&context, args, &mut output, &mut errors)?,
        shim::Command::Uninstall(args) => {
            shim::uninstall(&context, args, &mut output, &mut errors)?
        }
        shim::Command::Status(_) => shim::status(&context, &mut output)?,
    };
    Ok(summary.exit_code())
}

// `run` only fails when writing its own report failed — a closed pipe
// (`noh rm -v ... | head`), typically. Announcing that can fail for the very
// same reason, so the diagnostic is best effort and the exit code is what
// actually carries the outcome.
fn report_failure(error: &io::Error) -> ExitCode {
    match writeln!(io::stderr(), "noh: {error}") {
        Ok(()) | Err(_) => ExitCode::FAILURE,
    }
}
