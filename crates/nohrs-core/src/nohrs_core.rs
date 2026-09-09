//! Shared foundations for the nohrs application: configuration, error types,
//! and telemetry, kept app-agnostic so they can be reused across binaries.

/// Configuration: UI layout constants and the user-facing `config.toml` system.
pub mod config;
/// Crate-wide error and `Result` types.
pub mod errors;
/// Logging and other telemetry setup.
pub mod telemetry;
