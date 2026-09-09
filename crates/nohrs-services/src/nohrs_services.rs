//! Application services: filesystem listing, full-text/regex search, and
//! optional syntax highlighting.

/// Synchronous filesystem listing services.
pub mod fs;
/// Full-text and regex file search services.
pub mod search;
/// Syntax highlighting backed by `syntect`, mapped to GPUI colors.
#[cfg(feature = "gui")]
pub mod syntax;
