use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Logs go to stderr so machine-readable command output (e.g.
    // `nohrs config show`/`schema`) stays clean on stdout.
    let _ = fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .try_init();
}
