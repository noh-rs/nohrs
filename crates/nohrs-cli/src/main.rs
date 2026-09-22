//! The `noh` binary: picks the entry point from the program name (see
//! [`nohrs_cli::Invocation`]) and runs the requested command.
//!
//! Everything here is process wiring — locking the real stdio, opening the real
//! trash, reading the real environment. The parsing and the command engines
//! themselves live in the library, where they are exercised against fakes.

use std::io::{self, Write};
use std::process::ExitCode;

use nohrs_cli::{
    Cli, Command, Invocation, RmCli, doctor, index, ledger, log, rm, search, shim, trash,
};
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
    match run(&invocation) {
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
        Command::Search(args) => run_search(args),
        Command::Index(command) => run_index(command),
        Command::Log(command) => run_log(command),
        Command::Doctor => {
            let checks = doctor::check(&doctor::Environment::detect());
            doctor::report(&checks, &mut io::stdout().lock())
        }
        Command::Shim(command) => run_shim(command),
        Command::Completions { shell } => {
            // `clap_complete::generate` takes a `Write` but returns nothing, so
            // it can only panic on a failed write (`noh completions bash | head`
            // closes the pipe early). Buffer first, then write it ourselves so
            // that failure travels the same path as every other output error.
            let mut script = Vec::new();
            clap_complete::generate(*shell, &mut Invocation::command(), "noh", &mut script);
            io::stdout().lock().write_all(&script)?;
            Ok(0)
        }
    }
}

/// Read back the rolling log file. The directory is resolved here rather than
/// inside the session, which takes it injected so the tests can point it at a
/// temporary one.
fn run_log(command: &log::Command) -> io::Result<u8> {
    let directory = nohrs_core::config::paths::log_dir();
    let mut output = io::stdout().lock();
    let summary = log::Session::new(&directory, &mut output).run(command)?;
    Ok(summary.exit_code())
}

/// Walk the real filesystem looking for `args.query`. Everything the search
/// needs is in the operands, so there is no store or ledger to open here.
///
/// The streams are the *unlocked* handles, unlike every other command here:
/// see [`run_index`].
fn run_search(args: &search::Args) -> io::Result<u8> {
    let mut output = io::stdout();
    let mut errors = io::stderr();
    let backend = search::ServicesBackend;
    let summary = search::Session::new(&backend, &mut output, &mut errors).run(args)?;
    Ok(summary.exit_code())
}

/// Read or build the real index. `status` opens it for reading only, so it can
/// run beside the app; `build` takes the writer and reports being refused it.
///
/// Both streams are the unlocked handles, which lock per write. Holding a
/// `StderrLock` for the length of the command — what the other commands do —
/// deadlocks this one: the index does its work on tantivy's own threads, those
/// threads log, and the logger's stderr layer then waits for a lock this thread
/// is holding while it waits for them.
fn run_index(command: &index::Command) -> io::Result<u8> {
    let mut output = io::stdout();
    let mut errors = io::stderr();
    let backend = index::ServicesBackend;
    let summary = index::Session::new(&backend, &mut output, &mut errors).run(command)?;
    Ok(summary.exit_code())
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
