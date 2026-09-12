//! `noh`: the terminal side of nohrs, exposing the same file operations
//! the GUI performs (`nohrs-services::fs::ops`) as shell commands.
//!
//! The first command is [`rm`], a `rm(1)` work-alike that moves its operands to
//! the trash by default. It can be called by name (`noh rm notes.txt`) or
//! installed *in front of* the system `rm` by symlinking the binary under that
//! name earlier on `PATH`; [`Invocation`] picks the entry point from argv[0].

/// `doctor`: check that the pieces `noh rm` relies on are in place.
pub mod doctor;
/// Opening the trash ledger the CLI writes to and restores from.
pub mod ledger;
/// `log`: read back the rolling log file nohrs writes about itself.
pub mod log;
/// `rm`: trash-by-default removal.
pub mod rm;
/// `shim`: install and remove the symlinks that shadow a system command.
pub mod shim;
/// `trash` and `restore`: the recoverable half of `rm`.
pub mod trash;

use std::ffi::{OsStr, OsString};
use std::path::Path;

use clap::{CommandFactory, Parser, Subcommand};

/// The program name a symlink must have to be treated as the `rm` applet.
const RM_APPLET: &str = "rm";

/// The Windows spelling of [`RM_APPLET`].
const RM_APPLET_EXE: &str = "rm.exe";

/// Command-line entry point when the binary is called by its own name.
#[derive(Parser, Debug)]
#[command(
    name = "noh",
    version,
    about = "Terminal companion to the nohrs file workspace"
)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands `noh` understands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Remove files and directories, moving them to the trash unless
    /// `--permanent` is given.
    Rm(rm::Args),
    /// Put trashed items back where they came from.
    Restore(trash::RestoreArgs),
    /// Inspect and empty the trash.
    #[command(subcommand)]
    Trash(trash::Command),
    /// Show what nohrs recorded about itself, including how long each
    /// operation took.
    #[command(subcommand)]
    Log(log::Command),
    /// Check that the pieces `noh rm` relies on are in place.
    Doctor,
    /// Install or remove the symlinks that put `noh` in front of a system
    /// command.
    #[command(subcommand)]
    Shim(shim::Command),
    /// Print a shell completion script for `noh`.
    Completions {
        /// The shell to generate for.
        #[arg(value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

/// Entry point for a binary invoked through an `rm` symlink, where there is no
/// subcommand to name: every argument belongs to `rm`.
#[derive(Parser, Debug)]
#[command(
    name = RM_APPLET,
    version,
    about = "Remove files, moving them to the trash unless --permanent is given"
)]
pub struct RmCli {
    /// The `rm` operands and flags.
    #[command(flatten)]
    pub args: rm::Args,
}

/// Which entry point an argument vector selects.
#[derive(Debug)]
pub enum Invocation {
    /// Called by its own name: `noh <command> ...`.
    Direct(Cli),
    /// Called through an `rm` symlink: `rm -rf build/`.
    Rm(RmCli),
}

impl Invocation {
    /// Parse `args` (an argv, program name first), choosing the entry point from
    /// the program name.
    ///
    /// Installing a `rm` symlink to this binary earlier on `PATH` than
    /// `/bin/rm` is the point of the command: `rm -rf build/` then runs through
    /// here and lands in the trash instead of being unlinked.
    pub fn try_parse_from<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = OsString>,
    {
        let args: Vec<OsString> = args.into_iter().collect();
        if args.first().is_some_and(|program| is_rm_applet(program)) {
            RmCli::try_parse_from(args).map(Invocation::Rm)
        } else {
            Cli::try_parse_from(args).map(Invocation::Direct)
        }
    }

    /// Parse `args`, or print clap's usage message and exit, as a `main` would.
    pub fn parse_from<I>(args: I) -> Self
    where
        I: IntoIterator<Item = OsString>,
    {
        match Self::try_parse_from(args) {
            Ok(invocation) => invocation,
            Err(error) => error.exit(),
        }
    }

    /// Whether this command reads the log rather than adding to it.
    ///
    /// `noh log` is the reader, so a caller must not open the file sink before
    /// running it: `show` would report on a file it had just created itself,
    /// and `clear` would be handed the file its own process is appending to.
    pub fn reads_the_log(&self) -> bool {
        matches!(
            self,
            Invocation::Direct(Cli {
                command: Command::Log(_)
            })
        )
    }

    /// The clap command tree for `noh`, for generating completions.
    pub fn command() -> clap::Command {
        Cli::command()
    }
}

/// Render an error for a terminal user.
///
/// [`nohrs_core::errors::Error::Other`] displays itself as `other error: ...`,
/// which is useful in a log line and noise in a diagnostic the user reads: the
/// message it carries is already a complete sentence.
pub fn message(error: &nohrs_core::errors::Error) -> String {
    match error {
        nohrs_core::errors::Error::Other(message) => message.clone(),
        error => error.to_string(),
    }
}

/// Whether argv[0] names the `rm` applet. The file name has to be exactly `rm`
/// (or `rm.exe`): a copy or backup of the binary called `rm.old` is not a
/// request to behave as `rm`.
fn is_rm_applet(program: &OsStr) -> bool {
    Path::new(program)
        .file_name()
        .is_some_and(|name| name == OsStr::new(RM_APPLET) || name == OsStr::new(RM_APPLET_EXE))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn argv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    fn rm_args(parts: &[&str]) -> rm::Args {
        match Invocation::try_parse_from(argv(parts)).unwrap() {
            Invocation::Direct(Cli {
                command: Command::Rm(args),
            })
            | Invocation::Rm(RmCli { args }) => args,
            other => panic!("{parts:?} did not parse as rm: {other:?}"),
        }
    }

    #[test]
    fn only_the_log_command_is_a_reader_of_the_log() {
        let reads = |parts: &[&str]| {
            Invocation::try_parse_from(argv(parts))
                .unwrap()
                .reads_the_log()
        };
        for command in [
            vec!["noh", "log", "show"],
            vec!["noh", "log", "path"],
            vec!["noh", "log", "clear", "--force"],
        ] {
            assert!(reads(&command), "{command:?} writes to the log it reads");
        }
        assert!(!reads(&["noh", "rm", "notes.txt"]));
        assert!(!reads(&["rm", "-rf", "build"]));
    }

    #[test]
    fn an_rm_symlink_takes_the_rm_entry_point() {
        for program in ["rm", "/usr/local/bin/rm", "rm.exe"] {
            let invocation = Invocation::try_parse_from(argv(&[program, "-rf", "build"])).unwrap();
            assert!(
                matches!(invocation, Invocation::Rm(_)),
                "{program} did not dispatch to rm"
            );
        }
    }

    #[test]
    fn other_program_names_keep_the_subcommand_entry_point() {
        let invocation = Invocation::try_parse_from(argv(&["noh", "rm", "notes.txt"])).unwrap();
        assert!(matches!(invocation, Invocation::Direct(_)));

        // Names that merely start with or contain `rm` are not the applet: a
        // copy of the binary must not silently take over for `rm`.
        for program in ["/opt/bin/rmdir", "rm.backup", "rm.", "trm"] {
            assert!(
                !is_rm_applet(OsStr::new(program)),
                "{program} was treated as the rm applet"
            );
        }
    }

    #[test]
    fn an_rm_symlink_with_no_operands_still_parses() {
        let args = rm_args(&["rm"]);
        assert!(args.paths.is_empty());
    }

    #[test]
    fn posix_flags_parse_the_way_rm_spells_them() {
        let args = rm_args(&["rm", "-rf", "build", "dist"]);
        assert!(args.recursive);
        assert!(args.force);
        assert!(!args.permanent);
        assert_eq!(
            args.paths,
            vec![PathBuf::from("build"), PathBuf::from("dist")]
        );

        let args = rm_args(&["rm", "-Rdiv", "cache"]);
        assert!(args.recursive, "-R is accepted as an alias of -r");
        assert!(args.directory);
        assert!(args.interactive);
        assert!(args.verbose);
    }

    #[test]
    fn permanent_has_a_short_flag_and_a_no_trash_alias() {
        for spelling in ["-P", "--permanent", "--no-trash"] {
            let args = rm_args(&["noh", "rm", spelling, "secret.key"]);
            assert!(args.permanent, "{spelling} did not request a real delete");
        }
    }

    #[test]
    fn a_double_dash_protects_names_that_look_like_flags() {
        let args = rm_args(&["rm", "--", "-rf"]);
        assert!(!args.recursive);
        assert_eq!(args.paths, vec![PathBuf::from("-rf")]);
    }

    #[test]
    fn an_unknown_flag_is_rejected() {
        assert!(Invocation::try_parse_from(argv(&["noh", "rm", "--yolo", "x"])).is_err());
        assert!(Invocation::try_parse_from(argv(&["rm", "--yolo", "x"])).is_err());
    }

    #[test]
    fn the_help_text_names_the_program_the_user_typed() {
        let error = Invocation::try_parse_from(argv(&["rm", "--help"])).unwrap_err();
        let rendered = error.render().to_string();
        assert!(
            rendered.contains("Usage: rm [OPTIONS]"),
            "unexpected help: {rendered}"
        );
    }
}
