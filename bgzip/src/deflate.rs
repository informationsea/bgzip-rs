//! Binding to DEFLATE library.
//!
//! [libdeflater](https://crates.io/crates/libdeflater) or [flate2](https://crates.io/crates/flate2) is used to compress/decompress data.

use std::convert::TryInto;
use thiserror::Error;

#[cfg(not(feature = "libdeflater"))]
use flate2::Status;

#[cfg(not(feature = "libdeflater"))]
pub use flate2::Crc;

#[cfg(feature = "libdeflater")]
pub use libdeflater::Crc;

use crate::BGZFError;

/// Compression Level
#[cfg(not(feature = "libdeflater"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compression(flate2::Compression);

/// Compression Level
#[cfg(feature = "libdeflater")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compression(libdeflater::CompressionLvl);

#[cfg(not(feature = "libdeflater"))]
impl Compression {
    pub const fn new(level: u32) -> Result<Self, BGZFError> {
        Ok(Compression(flate2::Compression::new(level)))
    }

    pub const fn best() -> Self {
        Compression(flate2::Compression::best())
    }

    pub const fn fast() -> Self {
        Compression(flate2::Compression::fast())
    }
}

#[cfg(not(feature = "libdeflater"))]
impl From<flate2::Compression> for Compression {
    fn from(value: flate2::Compression) -> Self {
        Compression(value)
    }
}

#[cfg(feature = "libdeflater")]
impl Compression {
    pub fn new(level: u32) -> Result<Self, BGZFError> {
        Ok(Compression(
            libdeflater::CompressionLvl::new(level.try_into().unwrap()).map_err(|e| match e {
                libdeflater::CompressionLvlError::InvalidValue => {
                    BGZFError::InvalidCompressionLevel
                }
            })?,
        ))
    }

    pub fn best() -> Self {
        Compression(libdeflater::CompressionLvl::best())
    }

    pub fn fast() -> Self {
        Compression(libdeflater::CompressionLvl::fastest())
    }
}

#[cfg(not(feature = "libdeflater"))]
impl Default for Compression {
    fn default() -> Self {
        Compression(flate2::Compression::default())
    }
}

#[cfg(feature = "libdeflater")]
impl Default for Compression {
    fn default() -> Self {
        Compression(libdeflater::CompressionLvl::default())
    }
}

/// Compression Error
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CompressError {
    #[error("Insufficient spcae")]
    InsufficientSpace,
    #[error("Other: {0}")]
    Other(String),
}

/// flate2 based compressor
#[cfg(not(feature = "libdeflater"))]
#[derive(Debug)]
pub struct Compress(flate2::Compress);

#[cfg(not(feature = "libdeflater"))]
impl Compress {
    pub fn new(level: Compression) -> Self {
        Compress(flate2::Compress::new(level.0, false))
    }

    pub fn compress(
        &mut self,
        original_data: &[u8],
        compressed_data: &mut [u8],
    ) -> Result<usize, CompressError> {
        self.0.reset();
        let status = self
            .0
            .compress(
                original_data,
                compressed_data,
                flate2::FlushCompress::Finish,
            )
            .map_err(|e| CompressError::Other(e.message().unwrap_or("Unkown error").to_string()))?;
        match status {
            flate2::Status::BufError => Err(CompressError::InsufficientSpace),
            flate2::Status::Ok => Err(CompressError::InsufficientSpace),
            flate2::Status::StreamEnd => Ok(self.0.total_out().try_into().unwrap()),
        }
    }
}

/// libdeflater based compressor
#[cfg(feature = "libdeflater")]
pub struct Compress(libdeflater::Compressor);

#[cfg(feature = "libdeflater")]
impl Compress {
    pub fn new(level: Compression) -> Self {
        Compress(libdeflater::Compressor::new(level.0))
    }

    pub fn compress(
        &mut self,
        original_data: &[u8],
        compressed_data: &mut [u8],
    ) -> Result<usize, CompressError> {
        self.0
            .deflate_compress(original_data, compressed_data)
            .map_err(|e| match e {
                libdeflater::CompressionError::InsufficientSpace => {
                    CompressError::InsufficientSpace
                }
            })
    }
}

/// Decompress Error
#[derive(Debug, Error, Clone, PartialEq)]
pub enum DecompressError {
    #[error("Decompress Error: Insufficient spcae")]
    InsufficientSpace,
    #[error("Decompress Error: Bad data")]
    BadData,
    #[error("Decompress Error: {0}")]
    Other(String),
}

/// flate2 based decompressor
#[cfg(not(feature = "libdeflater"))]
#[derive(Debug)]
pub struct Decompress(flate2::Decompress);

#[cfg(not(feature = "libdeflater"))]
impl Decompress {
    pub fn new() -> Self {
        Decompress(flate2::Decompress::new(false))
    }

    pub fn decompress(
        &mut self,
        compressed_data: &[u8],
        decompressed_data: &mut [u8],
    ) -> Result<usize, DecompressError> {
        self.0.reset(false);
        match self
            .0
            .decompress(
                compressed_data,
                decompressed_data,
                flate2::FlushDecompress::Finish,
            )
            .map_err(|e| {
                DecompressError::Other(e.message().unwrap_or("Unknown Error").to_string())
            })? {
            Status::StreamEnd => Ok(self.0.total_out().try_into().unwrap()),
            Status::Ok => Err(DecompressError::InsufficientSpace),
            Status::BufError => Err(DecompressError::InsufficientSpace),
        }
    }
}

/// libdeflater based decompressor
#[cfg(feature = "libdeflater")]
pub struct Decompress(libdeflater::Decompressor);

#[cfg(feature = "libdeflater")]
impl Decompress {
    pub fn new() -> Self {
        Decompress(libdeflater::Decompressor::new())
    }

    pub fn decompress(
        &mut self,
        compressed_data: &[u8],
        decompressed_data: &mut [u8],
    ) -> Result<usize, DecompressError> {
        self.0
            .deflate_decompress(compressed_data, decompressed_data)
            .map_err(|e| match e {
                libdeflater::DecompressionError::BadData => DecompressError::BadData,
                libdeflater::DecompressionError::InsufficientSpace => {
                    DecompressError::InsufficientSpace
                }
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::prelude::*;

    const BUF_SIZE: usize = 3000;

    #[test]
    fn test_deflate_inflate() -> anyhow::Result<()> {
        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(0x3874aef456157523);
        let mut original_data = vec![0; BUF_SIZE];
        rand.fill_bytes(&mut original_data);

        let mut compress = Compress::new(Compression::default());
        let mut small_buf = [0; 100];
        assert_eq!(
            compress.compress(&original_data, &mut small_buf),
            Err(CompressError::InsufficientSpace)
        );

        let mut decompress = Decompress::new();
        let mut deflated_data = vec![0; BUF_SIZE + 500];
        let deflate_size = compress.compress(&original_data, &mut deflated_data)?;
        let mut inflated_data = vec![0; BUF_SIZE];

        assert_eq!(
            decompress.decompress(&deflated_data[..deflate_size], &mut small_buf),
            Err(DecompressError::InsufficientSpace)
        );

        assert!(decompress
            .decompress(&deflated_data[..100], &mut inflated_data)
            .is_err());

        let inflate_size =
            decompress.decompress(&deflated_data[..deflate_size], &mut inflated_data)?;
        assert_eq!(inflate_size, original_data.len());
        assert_eq!(inflated_data, original_data);

        Ok(())
    }
}
