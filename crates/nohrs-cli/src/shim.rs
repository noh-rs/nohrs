//! `noh shim` — installing and removing the symlinks that put `noh` in front of
//! a system command.
//!
//! `noh rm` only shadows `/bin/rm` if a symlink named `rm` sits earlier on
//! `PATH`, and until now `docs/cli.md` asked users to create it by hand with
//! `ln -sf`. A mistyped `ln` there can overwrite a real system binary, so the
//! command that knows which names are safe does it instead:
//!
//! * only names the binary actually answers to ([`APPLETS`]) are installed,
//! * an existing *regular file* is never replaced, `--force` or not — only a
//!   symlink is, and
//! * uninstall removes a link only after confirming it points back at this
//!   binary, so `noh shim uninstall rm` can never unlink `/bin/rm`.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use nohrs_core::errors::{Error, Result};

/// The program names this binary answers to when invoked through a symlink.
pub const APPLETS: &[&str] = &["rm"];

/// The `noh shim` subcommands.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Create the symlinks that put `noh` in front of the system commands.
    Install(InstallArgs),
    /// Remove symlinks created by `noh shim install`.
    Uninstall(UninstallArgs),
    /// Report which shims are installed and whether they come first on PATH.
    Status(StatusArgs),
}

/// Operands and flags for `noh shim install`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct InstallArgs {
    /// Commands to shadow. Defaults to every command `noh` can stand in for.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,

    /// Directory to create the links in (default: `~/.local/bin`).
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Replace an existing symlink that points somewhere else.
    #[arg(short, long)]
    pub force: bool,
}

/// Operands and flags for `noh shim uninstall`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct UninstallArgs {
    /// Commands to stop shadowing. Defaults to all of them.
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,

    /// Directory the links were created in (default: `~/.local/bin`).
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

/// Flags for `noh shim status`.
#[derive(clap::Args, Debug, Default, Clone)]
pub struct StatusArgs {
    /// Directory the links live in (default: `~/.local/bin`).
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

/// Where the shims go, what this binary is, and what `PATH` currently says.
#[derive(Debug, Clone)]
pub struct Context {
    /// The binary the shims should point at.
    pub current_exe: PathBuf,
    /// The directory the shims live in.
    pub dir: PathBuf,
    /// `PATH`, split into directories, in search order.
    pub path_entries: Vec<PathBuf>,
}

impl Context {
    /// Read the context from the running process.
    pub fn detect(dir: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            current_exe: std::env::current_exe()?,
            dir: match dir {
                Some(dir) => dir,
                None => default_dir()?,
            },
            path_entries: path_entries(),
        })
    }
}

/// The conventional per-user bin directory. Not an XDG base directory, so it is
/// resolved from the home directory rather than through `nohrs-core`'s paths.
pub fn default_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| Error::Other("could not determine the home directory".to_string()))?;
    Ok(home.join(".local").join("bin"))
}

/// `PATH`, split into directories in search order.
pub fn path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

/// What a run of one of these commands did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Links created or removed.
    pub changed: usize,
    /// Names that were already in the requested state.
    pub unchanged: usize,
    /// Names that could not be handled.
    pub failed: usize,
}

impl Summary {
    /// The process exit code for this run: `1` if anything failed, else `0`.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.failed > 0)
    }
}

/// Create the requested shims.
pub fn install(
    context: &Context,
    args: &InstallArgs,
    output: &mut dyn Write,
    errors: &mut dyn Write,
) -> io::Result<Summary> {
    let mut summary = Summary::default();
    for name in requested(&args.names, errors, &mut summary)? {
        let link = context.dir.join(&name);
        match install_one(context, &link, args.force) {
            Ok(Outcome::Created) => {
                summary.changed += 1;
                writeln!(
                    output,
                    "installed {} -> {}",
                    link.display(),
                    context.current_exe.display()
                )?;
                if let Some(shadowing) = shadowing(context, &name) {
                    writeln!(
                        errors,
                        "noh shim: {} is still found first on PATH; put {} before {} in PATH",
                        shadowing.display(),
                        context.dir.display(),
                        parent_of(&shadowing)
                    )?;
                }
            }
            Ok(Outcome::AlreadyCorrect) => {
                summary.unchanged += 1;
                writeln!(output, "{} is already installed", link.display())?;
            }
            Err(error) => {
                summary.failed += 1;
                writeln!(
                    errors,
                    "noh shim: {}: {}",
                    link.display(),
                    crate::message(&error)
                )?;
            }
        }
    }
    Ok(summary)
}

/// Remove the requested shims, but only where they really are ours.
pub fn uninstall(
    context: &Context,
    args: &UninstallArgs,
    output: &mut dyn Write,
    errors: &mut dyn Write,
) -> io::Result<Summary> {
    let mut summary = Summary::default();
    for name in requested(&args.names, errors, &mut summary)? {
        let link = context.dir.join(&name);
        match uninstall_one(context, &link) {
            Ok(true) => {
                summary.changed += 1;
                writeln!(output, "removed {}", link.display())?;
            }
            Ok(false) => {
                summary.unchanged += 1;
                writeln!(output, "{} is not installed", link.display())?;
            }
            Err(error) => {
                summary.failed += 1;
                writeln!(
                    errors,
                    "noh shim: {}: {}",
                    link.display(),
                    crate::message(&error)
                )?;
            }
        }
    }
    Ok(summary)
}

/// Report, per applet, what `PATH` currently resolves the name to.
pub fn status(context: &Context, output: &mut dyn Write) -> io::Result<Summary> {
    let mut summary = Summary::default();
    for name in APPLETS {
        let link = context.dir.join(name);
        let installed = link_target(&link).is_some_and(|target| is_current_exe(context, &target));
        match (installed, shadowing(context, name)) {
            (true, None) => {
                summary.unchanged += 1;
                writeln!(output, "{name}: active ({})", link.display())?;
            }
            (true, Some(other)) => {
                summary.failed += 1;
                writeln!(
                    output,
                    "{name}: installed at {} but {} comes first on PATH",
                    link.display(),
                    other.display()
                )?;
            }
            (false, _) => {
                writeln!(
                    output,
                    "{name}: not installed (run `noh shim install {name}`)"
                )?;
            }
        }
    }
    Ok(summary)
}

/// What installing one shim did.
enum Outcome {
    Created,
    AlreadyCorrect,
}

fn install_one(context: &Context, link: &Path, force: bool) -> Result<Outcome> {
    match std::fs::symlink_metadata(link) {
        Ok(metadata) if metadata.is_symlink() => {
            let target = link_target(link);
            if target.is_some_and(|target| is_current_exe(context, &target)) {
                return Ok(Outcome::AlreadyCorrect);
            }
            if !force {
                return Err(Error::Other(
                    "a symlink to something else is already there (pass --force to replace it)"
                        .to_string(),
                ));
            }
            std::fs::remove_file(link)?;
        }
        // Never unlink a real file here: that is how a system binary gets
        // destroyed by a typo. Removing it is the user's decision to make.
        Ok(_) => {
            return Err(Error::Other(
                "a file is already there, and it is not a symlink; remove it yourself if you mean to replace it".to_string(),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::Io(error)),
    }
    std::fs::create_dir_all(&context.dir)?;
    symlink(&context.current_exe, link)?;
    Ok(Outcome::Created)
}

fn uninstall_one(context: &Context, link: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(link) {
        Ok(metadata) if metadata.is_symlink() => {
            let target = link_target(link);
            if !target.is_some_and(|target| is_current_exe(context, &target)) {
                return Err(Error::Other(
                    "points at another program, so it was not created by `noh shim install`"
                        .to_string(),
                ));
            }
            std::fs::remove_file(link)?;
            Ok(true)
        }
        Ok(_) => Err(Error::Other(
            "is a real file, not a shim; leaving it alone".to_string(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Error::Io(error)),
    }
}

/// The names to act on, rejecting anything this binary cannot stand in for.
fn requested(
    names: &[String],
    errors: &mut dyn Write,
    summary: &mut Summary,
) -> io::Result<Vec<String>> {
    if names.is_empty() {
        return Ok(APPLETS.iter().map(|name| (*name).to_string()).collect());
    }
    let mut requested = Vec::new();
    for name in names {
        if APPLETS.contains(&name.as_str()) {
            requested.push(name.clone());
        } else {
            summary.failed += 1;
            writeln!(
                errors,
                "noh shim: {name}: noh has no such command (it can shadow: {})",
                APPLETS.join(", ")
            )?;
        }
    }
    Ok(requested)
}

/// The program `PATH` finds for `name` when it is *not* our shim — that is, the
/// one still winning over it.
fn shadowing(context: &Context, name: &str) -> Option<PathBuf> {
    let found = resolve_on_path(name, &context.path_entries)?;
    let target = link_target(&found).unwrap_or_else(|| found.clone());
    if is_current_exe(context, &target) {
        None
    } else {
        Some(found)
    }
}

/// The first executable named `name` in these directories, the way a shell
/// would find it.
pub fn resolve_on_path(name: &str, path_entries: &[PathBuf]) -> Option<PathBuf> {
    path_entries
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => false,
        #[cfg(unix)]
        Ok(metadata) => {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Where a symlink points, resolved against its own directory. `None` when the
/// path is not a symlink.
fn link_target(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    if target.is_absolute() {
        return Some(target);
    }
    Some(link.parent()?.join(target))
}

/// Whether `link` is a shim this binary installed: a symlink whose target is
/// `program`.
///
/// A name existing at the shim path proves nothing on its own — a hand-made
/// `~/.local/bin/rm -> /bin/rm` is a symlink at exactly that path — so callers
/// that report a shim as active have to resolve it.
pub fn points_at(link: &Path, program: &Path) -> bool {
    link_target(link).is_some_and(|target| same_program(&target, program))
}

/// Whether two paths name the same program. Both sides are canonicalized so a
/// binary reached through a symlinked directory still compares equal; if that
/// fails (a dangling link), the raw paths are compared instead.
fn same_program(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn is_current_exe(context: &Context, path: &Path) -> bool {
    same_program(path, &context.current_exe)
}

fn parent_of(path: &Path) -> String {
    path.parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(unix)]
fn symlink(original: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(original, link)?;
    Ok(())
}

#[cfg(not(unix))]
fn symlink(_original: &Path, _link: &Path) -> Result<()> {
    // Windows symlinks need either developer mode or elevation, so there is no
    // safe unattended equivalent to offer here.
    Err(Error::Other(
        "installing a shim is only supported on Unix; add the binary's directory to PATH instead"
            .to_string(),
    ))
}

#[cfg(test)]
// The fixtures build real link farms, so they need the synchronous filesystem
// calls that app code routes through `nohrs-services` instead.
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::fs;

    use tempfile::{TempDir, tempdir};

    use super::*;

    struct Fixture {
        root: TempDir,
        context: Context,
    }

    impl Fixture {
        /// A fake `noh` binary, a shim directory, and a `/bin` holding a real
        /// `rm` that the shim has to win over.
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
            let dir = root.path().join("local-bin");
            Self {
                context: Context {
                    current_exe: exe,
                    dir: dir.clone(),
                    // The shim directory first, the way an installed setup looks.
                    path_entries: vec![dir, system_bin],
                },
                root,
            }
        }

        fn system_bin_first(&self) -> Context {
            let mut context = self.context.clone();
            context.path_entries.reverse();
            context
        }

        fn link(&self, name: &str) -> PathBuf {
            self.context.dir.join(name)
        }
    }

    fn make_executable(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    struct Run {
        summary: Summary,
        stdout: String,
        stderr: String,
    }

    fn execute<F>(command: F) -> Run
    where
        F: FnOnce(&mut Vec<u8>, &mut Vec<u8>) -> io::Result<Summary>,
    {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let summary = command(&mut stdout, &mut stderr).unwrap();
        Run {
            summary,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
        }
    }

    fn install_all(context: &Context, force: bool) -> Run {
        let args = InstallArgs {
            force,
            ..InstallArgs::default()
        };
        execute(|stdout, stderr| install(context, &args, stdout, stderr))
    }

    #[cfg(unix)]
    #[test]
    fn install_creates_a_link_to_this_binary() {
        let fixture = Fixture::new();

        let run = install_all(&fixture.context, false);

        assert_eq!(run.summary.changed, 1);
        assert_eq!(run.summary.exit_code(), 0);
        assert_eq!(
            fs::read_link(fixture.link("rm")).unwrap(),
            fixture.context.current_exe
        );
        assert!(run.stdout.contains("installed"), "{}", run.stdout);
        assert!(run.stderr.is_empty(), "{}", run.stderr);
    }

    #[cfg(unix)]
    #[test]
    fn installing_twice_is_not_an_error() {
        let fixture = Fixture::new();
        install_all(&fixture.context, false);

        let run = install_all(&fixture.context, false);

        assert_eq!(run.summary.changed, 0);
        assert_eq!(run.summary.unchanged, 1);
        assert!(run.stdout.contains("already installed"), "{}", run.stdout);
    }

    #[cfg(unix)]
    #[test]
    fn install_warns_when_the_system_command_still_comes_first() {
        let fixture = Fixture::new();

        let run = install_all(&fixture.system_bin_first(), false);

        assert_eq!(run.summary.changed, 1);
        assert!(
            run.stderr.contains("still found first on PATH"),
            "{}",
            run.stderr
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_never_replaces_a_real_file_even_with_force() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.context.dir).unwrap();
        let occupied = fixture.link("rm");
        fs::write(&occupied, "a real program").unwrap();

        let run = install_all(&fixture.context, true);

        assert_eq!(run.summary.failed, 1);
        assert_eq!(run.summary.exit_code(), 1);
        assert!(run.stderr.contains("not a symlink"), "{}", run.stderr);
        assert_eq!(
            fs::read_to_string(&occupied).unwrap(),
            "a real program",
            "the file that was there must survive"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_another_program_needs_force() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.context.dir).unwrap();
        let elsewhere = fixture.root.path().join("bin").join("rm");
        std::os::unix::fs::symlink(&elsewhere, fixture.link("rm")).unwrap();

        let run = install_all(&fixture.context, false);
        assert_eq!(run.summary.failed, 1);
        assert!(run.stderr.contains("--force"), "{}", run.stderr);

        let run = install_all(&fixture.context, true);
        assert_eq!(run.summary.changed, 1);
        assert_eq!(
            fs::read_link(fixture.link("rm")).unwrap(),
            fixture.context.current_exe
        );
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_removes_only_our_own_link() {
        let fixture = Fixture::new();
        install_all(&fixture.context, false);

        let args = UninstallArgs::default();
        let run = execute(|stdout, stderr| uninstall(&fixture.context, &args, stdout, stderr));

        assert_eq!(run.summary.changed, 1);
        assert!(!fixture.link("rm").exists());
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_refuses_to_unlink_a_system_binary() {
        let fixture = Fixture::new();
        // Point the shim directory straight at the system one, so `rm` there is
        // the real thing rather than our link.
        let mut context = fixture.context.clone();
        context.dir = fixture.root.path().join("bin");

        let args = UninstallArgs::default();
        let run = execute(|stdout, stderr| uninstall(&context, &args, stdout, stderr));

        assert_eq!(run.summary.failed, 1);
        assert!(run.stderr.contains("not a shim"), "{}", run.stderr);
        assert!(
            context.dir.join("rm").exists(),
            "`noh shim uninstall` must never remove /bin/rm"
        );
    }

    #[cfg(unix)]
    #[test]
    fn uninstalling_what_is_not_there_is_not_an_error() {
        let fixture = Fixture::new();

        let args = UninstallArgs::default();
        let run = execute(|stdout, stderr| uninstall(&fixture.context, &args, stdout, stderr));

        assert_eq!(run.summary.exit_code(), 0);
        assert_eq!(run.summary.unchanged, 1);
        assert!(run.stdout.contains("not installed"), "{}", run.stdout);
    }

    #[test]
    fn an_unknown_name_is_refused() {
        let fixture = Fixture::new();
        let args = InstallArgs {
            names: vec!["sudo".to_string()],
            ..InstallArgs::default()
        };

        let run = execute(|stdout, stderr| install(&fixture.context, &args, stdout, stderr));

        assert_eq!(run.summary.failed, 1);
        assert!(run.stderr.contains("no such command"), "{}", run.stderr);
        assert!(!fixture.link("sudo").exists());
    }

    #[cfg(unix)]
    #[test]
    fn status_distinguishes_active_from_shadowed_from_absent() {
        let fixture = Fixture::new();

        let run = execute(|stdout, _| status(&fixture.context, stdout));
        assert!(run.stdout.contains("not installed"), "{}", run.stdout);

        install_all(&fixture.context, false);
        let run = execute(|stdout, _| status(&fixture.context, stdout));
        assert!(run.stdout.contains("rm: active"), "{}", run.stdout);
        assert_eq!(run.summary.exit_code(), 0);

        let shadowed = fixture.system_bin_first();
        let run = execute(|stdout, _| status(&shadowed, stdout));
        assert!(run.stdout.contains("comes first on PATH"), "{}", run.stdout);
        assert_eq!(run.summary.exit_code(), 1);
    }

    #[test]
    fn path_resolution_finds_the_first_executable_only() {
        let fixture = Fixture::new();
        let found = resolve_on_path("rm", &fixture.context.path_entries).unwrap();
        assert_eq!(found, fixture.root.path().join("bin").join("rm"));

        assert!(resolve_on_path("nonesuch", &fixture.context.path_entries).is_none());
        // A directory of the right name is not a program.
        assert!(resolve_on_path("bin", &[fixture.root.path().to_path_buf()]).is_none());
    }
}
