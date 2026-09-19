//! Where the daemon listens, and how exactly one of them gets to.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The socket a daemon listens on and the lock file beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The unix socket clients connect to.
    pub socket: PathBuf,
    /// The file whose lock says who the daemon is.
    pub lock: PathBuf,
}

impl Endpoint {
    /// The endpoint for this user's session.
    pub fn for_session() -> Self {
        Self::under(nohrs_core::config::paths::runtime_dir())
    }

    /// An endpoint under `directory`, for tests and for a caller with its own
    /// layout.
    pub fn under(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref();
        Self {
            socket: directory.join("indexd.sock"),
            lock: directory.join("indexd.lock"),
        }
    }

    /// Makes sure the directory exists and is the user's alone.
    ///
    /// A socket inherits the traversability of its directory, and what travels
    /// over this one is a request to rewrite the index of someone's home
    /// directory. `0700` is what keeps that to them.
    pub fn prepare(&self) -> Result<()> {
        let directory = self
            .socket
            .parent()
            .context("the endpoint has no directory")?;
        std::fs::create_dir_all(directory)
            .with_context(|| format!("cannot prepare {}", directory.display()))?;
        restrict(directory)
    }
}

#[cfg(unix)]
fn restrict(directory: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(directory, permissions)
        .with_context(|| format!("cannot restrict {}", directory.display()))
}

#[cfg(not(unix))]
fn restrict(_directory: &Path) -> Result<()> {
    // Windows has no mode to set; the per-user profile's ACL is what applies,
    // as it is for the log directory (`docs/logging.md` §1.1).
    Ok(())
}

/// The exclusive right to be the daemon, held for as long as this process is.
///
/// An advisory lock rather than a pid file, because the kernel releases it when
/// the holder dies — however it dies. A pid file outlives a `SIGKILL` and has
/// to be second-guessed; this cannot lie.
#[derive(Debug)]
pub struct Claim {
    // Holding the file holds the lock. Dropping it releases it, and so does
    // the process ending.
    _file: File,
}

impl Claim {
    /// Takes the claim, or returns `None` when another process holds it.
    #[cfg(unix)]
    pub fn take(path: &Path) -> Result<Option<Self>> {
        use rustix::fs::{FlockOperation, flock};

        let file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Self { _file: file })),
            // Held by a live daemon. Not an error: the caller talks to it.
            Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot lock {}", path.display())),
        }
    }

    /// Windows has no `flock`, and no unix socket to guard either; the daemon
    /// does not run there and callers fall back to indexing in-process.
    #[cfg(not(unix))]
    pub fn take(_path: &Path) -> Result<Option<Self>> {
        Ok(None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_keeps_its_socket_and_lock_together() {
        let endpoint = Endpoint::under("/run/user/1000/nohrs");

        assert_eq!(
            endpoint.socket,
            PathBuf::from("/run/user/1000/nohrs/indexd.sock")
        );
        assert_eq!(
            endpoint.lock,
            PathBuf::from("/run/user/1000/nohrs/indexd.lock")
        );
    }

    #[cfg(unix)]
    #[test]
    fn preparing_the_endpoint_leaves_the_directory_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::under(directory.path().join("nested"));
        endpoint.prepare().unwrap();

        let mode = std::fs::metadata(directory.path().join("nested"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "the directory is readable by others");
    }

    #[cfg(unix)]
    #[test]
    fn only_one_process_at_a_time_can_claim_to_be_the_daemon() {
        let directory = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::under(directory.path());
        endpoint.prepare().unwrap();

        let first = Claim::take(&endpoint.lock).unwrap();
        assert!(first.is_some(), "the first claim was refused");

        // A second claim from this same process must be refused as well:
        // `flock` is per open file description, and each `take` opens its own.
        let second = Claim::take(&endpoint.lock).unwrap();
        assert!(second.is_none(), "two processes both became the daemon");

        drop(first);
        let after = Claim::take(&endpoint.lock).unwrap();
        assert!(after.is_some(), "the claim outlived its holder");
    }
}
