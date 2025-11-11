use std::path::PathBuf;
use thiserror::Error;

use tantivy::TantivyError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("search error: {0}")]
    Search(#[from] TantivyError),
    #[error("invalid search scope outside of index root: {0}")]
    InvalidScope(PathBuf),
    #[error("other error: {0}")]
    Other(String),
}
