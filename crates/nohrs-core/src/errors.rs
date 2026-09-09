use thiserror::Error;

/// Convenience alias for results that fail with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by `nohrs-core`.
#[derive(Debug, Error)]
pub enum Error {
    /// A requested feature has not been implemented yet.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// An underlying I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A failure that does not fit the other variants.
    #[error("other error: {0}")]
    Other(String),
}
