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
    #[error("Too large compress unit")]
    TooLargeCompressUnit,
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Utf8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("Failed to convert native path to UTF-8")]
    PathConvertionError,
    #[error("Compression Error: {0}")]
    CompressionError(#[from] crate::deflate::CompressError),
    #[error("Decompression Error: {0}")]
    DecompressionError(#[from] crate::deflate::DecompressError),
    #[error("Invalid Compression Level")]
    InvalidCompressionLevel,
    #[error("Error: {0}")]
    Other(&'static str),
}

impl Into<std::io::Error> for BGZFError {
    fn into(self) -> std::io::Error {
        match self {
            BGZFError::IoError(e) => e,
            other => std::io::Error::new(std::io::ErrorKind::Other, other),
        }
    }
}

impl BGZFError {
    pub fn into_io_error(self) -> std::io::Error {
        self.into()
    }
}
