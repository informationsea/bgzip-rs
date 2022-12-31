use thiserror::Error;

/// A BGZF error.
#[derive(Debug, Error)]
pub enum BGZFError {
    #[error("Failed to parse header at position: {position}")]
    HeaderParseError { position: u64 },
    #[error("not tabix format")]
    NotTabix,
    #[error("not BGZF format")]
    NotBGZF,
    #[error("not gzip format")]
    NotGzip,
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Utf8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("Error: {message:}")]
    Other { message: &'static str },
}
