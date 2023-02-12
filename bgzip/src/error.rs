use thiserror::Error;

/// A BGZF error.
#[derive(Debug, Error)]
pub enum BGZFError {
    /// Failed to parse header
    #[error("Failed to parse header at position: {position}")]
    HeaderParseError { position: u64 },
    /// Not tabix format
    #[error("not tabix format")]
    NotTabix,
    /// Not BGZF format
    #[error("not BGZF format")]
    NotBGZF,
    /// Not gzip format
    #[error("not gzip format")]
    NotGzip,
    /// Too larget compress unit. A compress unit must be smaller than 64k bytes.
    #[error("Too large compress unit")]
    TooLargeCompressUnit,
    /// I/O Error
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),
    /// UTF-8 Error
    #[error("Utf8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    /// Failed to convert native path to UTF-8
    #[error("Failed to convert native path to UTF-8")]
    PathConvertionError,
    /// Deflate compresssion error
    #[error("Compression Error: {0}")]
    CompressionError(#[from] crate::deflate::CompressError),
    /// Inflate decompression error
    #[error("Decompression Error: {0}")]
    DecompressionError(#[from] crate::deflate::DecompressError),
    /// Invalid compression level
    #[error("Invalid Compression Level")]
    InvalidCompressionLevel,
    /// Other error
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
