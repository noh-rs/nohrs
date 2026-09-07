//! `noh doctor` — a check that the pieces `noh rm` depends on are actually in
//! place.
//!
//! Standing in front of `/bin/rm` means a silent misconfiguration is expensive:
//! a shim that is not first on `PATH` quietly stops protecting anything, and a
//! ledger that cannot be written makes deletions unrecoverable on macOS without
//! anything failing at the time. `doctor` is where those go from invisible to
//! obvious, before they cost someone a file.

use std::io::{self, Write};
use std::path::PathBuf;

use nohrs_core::config::paths;
use nohrs_services::fs::trash;
use nohrs_services::fs::trash_ledger::{self, TrashLedger};

use crate::shim;

/// How a single check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Working as intended.
    Ok,
    /// Usable, but not doing what the user probably wants.
    Warn,
    /// Broken: something that should work does not.
    Error,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Error => "error",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What was checked.
    pub name: &'static str,
    /// How it came out.
    pub status: Status,
    /// What was found, in the user's terms.
    pub detail: String,
}

impl Check {
    fn new(name: &'static str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            name,
            status,
            detail: detail.into(),
        }
    }
}

/// Everything the checks inspect, gathered up so they can run against a
/// temporary directory under test instead of the developer's own machine.
#[derive(Debug, Clone)]
pub struct Environment {
    /// The running binary, if the OS will say.
    pub current_exe: Option<PathBuf>,
    /// `PATH`, split into directories in search order.
    pub path_entries: Vec<PathBuf>,
    /// Where `noh shim install` puts its links.
    pub shim_dir: PathBuf,
    /// The trash directory items from the home volume land in.
    pub trash_dir: Option<PathBuf>,
    /// The record of what nohrs trashed.
    pub ledger: TrashLedger,
    /// Whether restores on this platform go through the ledger rather than an
    /// OS trash index. Injected rather than read from the target so both cases
    /// can be checked on any machine.
    pub ledger_in_use: bool,
    /// `config.toml`.
    pub config_file: PathBuf,
}

impl Environment {
    /// Read the environment from this machine. Anything that cannot be resolved
    /// becomes a failing check rather than a failure to run at all.
    pub fn detect() -> Self {
        let ledger_in_use = !trash::OS_INDEX_AVAILABLE;
        Self {
            current_exe: std::env::current_exe().ok(),
            path_entries: shim::path_entries(),
            shim_dir: shim::default_dir().unwrap_or_default(),
            trash_dir: trash_ledger::home_trash_dir().ok(),
            // Opened (which creates the data directory) only where the ledger is
            // actually used; otherwise the path is named but nothing is made.
            // A failure to open leaves the path too, and `check_ledger` reports
            // the read failure that follows.
            ledger: match ledger_in_use.then(TrashLedger::open_default) {
                Some(Ok(ledger)) => ledger,
                Some(Err(_)) | None => TrashLedger::at(TrashLedger::default_path()),
            },
            ledger_in_use,
            config_file: paths::config_file(),
        }
    }
}

/// Run every check.
pub fn check(environment: &Environment) -> Vec<Check> {
    let mut checks = vec![check_binary(environment)];
    checks.extend(shim_checks(environment));
    checks.push(check_trash(environment));
    checks.push(check_ledger(environment));
    checks.push(check_config(environment));
    checks
}

/// Print the checks and return the process exit code: `1` if any of them failed.
pub fn report(checks: &[Check], output: &mut dyn Write) -> io::Result<u8> {
    let width = checks
        .iter()
        .map(|check| check.name.len())
        .max()
        .unwrap_or_default();
    let mut failed = false;
    for check in checks {
        failed |= check.status == Status::Error;
        writeln!(
            output,
            "{:<5} {:<width$}  {}",
            check.status.label(),
            check.name,
            check.detail
        )?;
    }
    Ok(u8::from(failed))
}

fn check_binary(environment: &Environment) -> Check {
    match &environment.current_exe {
        Some(exe) => Check::new("binary", Status::Ok, exe.display().to_string()),
        None => Check::new(
            "binary",
            Status::Error,
            "the running program's own path could not be determined",
        ),
    }
}

/// One check per applet: is the shim installed, and does it win on `PATH`?
fn shim_checks(environment: &Environment) -> Vec<Check> {
    shim::APPLETS
        .iter()
        .map(|applet| {
            let name = "shim";
            let link = environment.shim_dir.join(applet);
            let found = shim::resolve_on_path(applet, &environment.path_entries);
            let installed = link.is_symlink();
            match (installed, found) {
                (_, None) => Check::new(
                    name,
                    Status::Warn,
                    format!("no `{applet}` on PATH at all, which is unexpected"),
                ),
                (true, Some(found)) if found == link => Check::new(
                    name,
                    Status::Ok,
                    format!("`{applet}` runs {} (this binary)", found.display()),
                ),
                (true, Some(found)) => Check::new(
                    name,
                    Status::Warn,
                    format!(
                        "`{applet}` still runs {}; put {} earlier in PATH",
                        found.display(),
                        environment.shim_dir.display()
                    ),
                ),
                (false, Some(found)) => Check::new(
                    name,
                    Status::Warn,
                    format!(
                        "`{applet}` runs {}; run `noh shim install {applet}` to put noh in front of it",
                        found.display()
                    ),
                ),
            }
        })
        .collect()
}

fn check_trash(environment: &Environment) -> Check {
    let Some(trash_dir) = &environment.trash_dir else {
        return Check::new(
            "trash",
            Status::Error,
            "the trash directory could not be located",
        );
    };
    let index = if environment.ledger_in_use {
        "no OS trash index here, so restores use the nohrs ledger"
    } else {
        "the OS trash index is available"
    };
    if trash_dir.is_dir() {
        Check::new(
            "trash",
            Status::Ok,
            format!("{} ({index})", trash_dir.display()),
        )
    } else {
        // Not an error: the directory is created the first time something is
        // trashed, so a machine that has never deleted anything is fine.
        Check::new(
            "trash",
            Status::Warn,
            format!("{} does not exist yet ({index})", trash_dir.display()),
        )
    }
}

fn check_ledger(environment: &Environment) -> Check {
    if !environment.ledger_in_use {
        // Nothing writes it here, so its state says nothing about whether a
        // restore will work (see `nohrs_services::fs::ops::trash_path`).
        return Check::new(
            "ledger",
            Status::Ok,
            "not used on this platform: the OS trash index records where items came from",
        );
    }
    match environment.ledger.records() {
        Ok(records) => Check::new(
            "ledger",
            Status::Ok,
            format!(
                "{} ({} {})",
                environment.ledger.path().display(),
                records.len(),
                if records.len() == 1 {
                    "item recorded"
                } else {
                    "items recorded"
                }
            ),
        ),
        Err(error) => Check::new(
            "ledger",
            Status::Error,
            format!(
                "{} cannot be read ({error}); `noh restore` will not find anything",
                environment.ledger.path().display()
            ),
        ),
    }
}

fn check_config(environment: &Environment) -> Check {
    if !environment.config_file.exists() {
        // The defaults are complete, so no config file is a normal state.
        return Check::new(
            "config",
            Status::Ok,
            format!(
                "{} does not exist; defaults are in use",
                environment.config_file.display()
            ),
        );
    }
    let (_, diagnostics) = nohrs_core::config::load_from_path(&environment.config_file);
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.level == nohrs_core::config::DiagnosticLevel::Error)
        .count();
    if errors == 0 {
        Check::new(
            "config",
            Status::Ok,
            environment.config_file.display().to_string(),
        )
    } else {
        Check::new(
            "config",
            Status::Error,
            format!(
                "{} has {errors} error(s); run `nohrs config check` for details",
                environment.config_file.display()
            ),
        )
    }
}

#[cfg(test)]
// The fixtures build real directories and config files, so they need the
// synchronous filesystem calls that app code routes through `nohrs-services`.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::fs;

    use tempfile::{TempDir, tempdir};

    use super::*;

    struct Fixture {
        root: TempDir,
        environment: Environment,
    }

    impl Fixture {
        /// A machine with a `noh` binary, a system `rm`, a trash directory, and
        /// no shim installed yet.
        fn new() -> Self {
            let root = tempdir().unwrap();
            let exe = root.path().join("noh");
            fs::write(&exe, "#!/bin/sh\n").unwrap();
            make_executable(&exe);
            let system_bin = root.path().join("bin");
            fs::create_dir(&system_bin).unwrap();
            let system_rm = system_bin.join("rm");
            fs::write(&system_rm, "#!/bin/sh\n").unwrap();
            make_executable(&system_rm);
            let shim_dir = root.path().join("local-bin");
            fs::create_dir(&shim_dir).unwrap();
            let trash_dir = root.path().join("Trash");
            fs::create_dir(&trash_dir).unwrap();

            let environment = Environment {
                current_exe: Some(exe),
                path_entries: vec![shim_dir.clone(), system_bin],
                shim_dir,
                trash_dir: Some(trash_dir),
                ledger: TrashLedger::at(root.path().join("ledger.jsonl")),
                // The macOS case, where restores go through the ledger; the
                // other one has its own test.
                ledger_in_use: true,
                config_file: root.path().join("config.toml"),
            };
            Self { root, environment }
        }

        fn install_shim(&self) {
            let link = self.environment.shim_dir.join("rm");
            #[cfg(unix)]
            std::os::unix::fs::symlink(self.environment.current_exe.as_ref().unwrap(), &link)
                .unwrap();
            #[cfg(not(unix))]
            fs::write(&link, "shim").unwrap();
        }

        fn checks(&self) -> Vec<Check> {
            check(&self.environment)
        }
    }

    fn make_executable(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        match checks.iter().find(|check| check.name == name) {
            Some(check) => check,
            None => panic!("no {name} check in the report"),
        }
    }

    fn rendered(checks: &[Check]) -> (String, u8) {
        let mut output = Vec::new();
        let code = report(checks, &mut output).unwrap();
        (String::from_utf8(output).unwrap(), code)
    }

    #[test]
    fn a_healthy_machine_without_a_shim_only_warns() {
        let fixture = Fixture::new();

        let checks = fixture.checks();

        assert_eq!(find(&checks, "binary").status, Status::Ok);
        assert_eq!(find(&checks, "trash").status, Status::Ok);
        assert_eq!(find(&checks, "ledger").status, Status::Ok);
        assert_eq!(find(&checks, "config").status, Status::Ok);
        let shim = find(&checks, "shim");
        assert_eq!(shim.status, Status::Warn);
        assert!(shim.detail.contains("noh shim install"), "{}", shim.detail);
        // A warning is advice, not a failure, so the command still succeeds.
        assert_eq!(rendered(&checks).1, 0);
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_shim_that_wins_on_path_reports_ok() {
        let fixture = Fixture::new();
        fixture.install_shim();

        assert_eq!(find(&fixture.checks(), "shim").status, Status::Ok);
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_shim_behind_the_system_command_is_called_out() {
        let fixture = Fixture::new();
        fixture.install_shim();
        let mut environment = fixture.environment.clone();
        environment.path_entries.reverse();

        let checks = check(&environment);

        let shim = find(&checks, "shim");
        assert_eq!(shim.status, Status::Warn);
        assert!(shim.detail.contains("earlier in PATH"), "{}", shim.detail);
    }

    #[test]
    fn a_missing_binary_path_is_an_error_and_sets_the_exit_code() {
        let fixture = Fixture::new();
        let mut environment = fixture.environment.clone();
        environment.current_exe = None;

        let checks = check(&environment);

        assert_eq!(find(&checks, "binary").status, Status::Error);
        let (output, code) = rendered(&checks);
        assert_eq!(code, 1);
        assert!(output.contains("error binary"), "{output}");
    }

    #[test]
    fn an_unreadable_ledger_is_an_error() {
        let fixture = Fixture::new();
        let mut environment = fixture.environment.clone();
        // A directory where the ledger file should be: opening it fails with
        // something other than "not found".
        let path = fixture.root.path().join("ledger-dir");
        fs::create_dir(&path).unwrap();
        environment.ledger = TrashLedger::at(path);

        let ledger = check(&environment);
        let ledger = find(&ledger, "ledger");

        assert_eq!(ledger.status, Status::Error);
        assert!(ledger.detail.contains("noh restore"), "{}", ledger.detail);
    }

    #[test]
    fn where_the_os_keeps_its_own_index_the_ledger_is_not_consulted() {
        let fixture = Fixture::new();
        let mut environment = fixture.environment.clone();
        environment.ledger_in_use = false;
        // Unreadable, and it does not matter: nothing writes it here.
        let path = fixture.root.path().join("ledger-dir");
        fs::create_dir(&path).unwrap();
        environment.ledger = TrashLedger::at(path);

        let checks = check(&environment);

        assert_eq!(find(&checks, "ledger").status, Status::Ok);
        assert!(
            find(&checks, "trash").detail.contains("OS trash index"),
            "{}",
            find(&checks, "trash").detail
        );
    }

    #[test]
    fn a_missing_trash_directory_is_only_a_warning() {
        let fixture = Fixture::new();
        let mut environment = fixture.environment.clone();
        environment.trash_dir = Some(fixture.root.path().join("never-used"));

        assert_eq!(find(&check(&environment), "trash").status, Status::Warn);
    }

    #[test]
    fn a_broken_config_file_is_an_error() {
        let fixture = Fixture::new();
        fs::write(
            &fixture.environment.config_file,
            "schema_version = \"nope\"\n",
        )
        .unwrap();

        let config = fixture.checks();
        let config = find(&config, "config");

        assert_eq!(config.status, Status::Error);
        assert!(config.detail.contains("error"), "{}", config.detail);
    }

    #[test]
    fn a_valid_config_file_reports_ok() {
        let fixture = Fixture::new();
        fs::write(&fixture.environment.config_file, "").unwrap();

        assert_eq!(find(&fixture.checks(), "config").status, Status::Ok);
    }

    #[test]
    fn the_report_lines_up_and_names_every_check() {
        let fixture = Fixture::new();

        let (output, code) = rendered(&fixture.checks());

        assert_eq!(code, 0);
        for name in ["binary", "shim", "trash", "ledger", "config"] {
            assert!(output.contains(name), "{name} missing from:\n{output}");
        }
    }
}
