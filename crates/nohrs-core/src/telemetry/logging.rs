use super::LogErr;
use tracing_subscriber::{EnvFilter, fmt};

/// Install the global `tracing` subscriber, writing logs to stderr and honoring
/// `RUST_LOG` (defaulting to `info`). A no-op if a subscriber is already set.
pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Logs go to stderr so machine-readable command output (e.g.
    // `nohrs config show`/`schema`) stays clean on stdout. A failed `try_init`
    // just means a subscriber is already installed, which the existing
    // subscriber will surface.
    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .try_init()
        .log_err();
}

#[cfg(test)]
mod tests {
    use super::init_logging;

    #[test]
    fn init_logging_is_idempotent() {
        // Installs a global subscriber (or no-ops via `try_init` if one already
        // exists); calling it repeatedly must not panic.
        init_logging();
        init_logging();
    }
}
