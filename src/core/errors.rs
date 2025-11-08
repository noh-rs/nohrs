use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("other error: {0}")]
    Other(String),
}
