use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(env_filter).try_init();
}
