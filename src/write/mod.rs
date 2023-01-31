//! BGZF writer

#[cfg(feature = "rayon")]
mod thread;

#[cfg(feature = "rayon")]
pub use thread::BGZFMultiThreadWriter;

use crate::header;
use flate2::{Compress, Crc};
use std::convert::TryInto;
use std::io::{self, Write};

/// A BGZF writer
pub struct BGZFWriter<W: io::Write> {
    writer: W,
    buffer: Vec<u8>,
    compressed_buffer: Vec<u8>,
    compress: Compress,
    compress_unit_size: usize,
    closed: bool,
}

/// Default BGZF compress unit size
pub const DEFAULT_COMPRESS_UNIT_SIZE: usize = 65280;

/// Maximum BGZF compress unit size
pub const MAXIMUM_COMPRESS_UNIT_SIZE: usize = 64 * 1024;

pub(crate) const EXTRA_COMPRESS_BUFFER_SIZE: usize = 500;

impl<W: io::Write> BGZFWriter<W> {
    /// Create new BGZF writer from [`std::io::Write`]
    pub fn new(writer: W, level: flate2::Compression) -> Self {
        Self::with_compress_unit_size(writer, level, DEFAULT_COMPRESS_UNIT_SIZE)
    }

    /// Cerate new BGZF writer with compress unit size.
    pub fn with_compress_unit_size(
        writer: W,
        level: flate2::Compression,
        compress_unit_size: usize,
    ) -> Self {
        let mut compressed_buffer = Vec::new();
        compressed_buffer.reserve(compress_unit_size + EXTRA_COMPRESS_BUFFER_SIZE);
        BGZFWriter {
            writer,
            buffer: Vec::new(),
            compressed_buffer,
            compress_unit_size,
            compress: Compress::new(level, false),
            closed: false,
        }
    }

    fn write_block(&mut self) -> io::Result<()> {
        let uncompressed_block_size = self.compress_unit_size.min(self.buffer.len());
        write_block(
            &mut self.writer,
            &self.buffer[..uncompressed_block_size],
            &mut self.compressed_buffer,
            &mut self.compress,
        )?;

        self.buffer.drain(..uncompressed_block_size);

        Ok(())
    }

    /// Write end-of-file marker and close BGZF.
    ///
    /// Explicitly call of this method is not required. Drop trait will write end-of-file marker automatically.
    /// If you need to handle I/O errors while closing, please use this method.
    pub fn close(mut self) -> io::Result<()> {
        if !self.closed {
            self.flush()?;
            self.writer.write_all(EOF_MARKER)?;
            self.closed = true;
        }
        Ok(())
    }
}

impl<W: io::Write> io::Write for BGZFWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        while self.compress_unit_size < self.buffer.len() {
            self.write_block()?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        while !self.buffer.is_empty() {
            self.write_block()?;
        }
        Ok(())
    }
}

/// End-of-file maker.
///
/// This marker should be written at end of the BGZF files.
pub const EOF_MARKER: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
    0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl<W: io::Write> Drop for BGZFWriter<W> {
    fn drop(&mut self) {
        if !self.closed {
            self.flush().unwrap();
            self.writer.write_all(EOF_MARKER).unwrap();
            self.closed = true;
        }
    }
}

/// Write single BGZF block to writer.
///
/// This function is useful when writing your own parallelized BGZF writer.
/// `temporary_buffer` and `compress` will be cleared before using them.
/// `temporary_buffer` must be reserved enough size to store compressed data.
/// `compress` must be initialized without zlib_header flag.
pub fn write_block<W: io::Write>(
    mut writer: W,
    data: &[u8],
    temporary_buffer: &mut Vec<u8>,
    compress: &mut flate2::Compress,
) -> io::Result<()> {
    temporary_buffer.clear();
    compress.reset();
    let status = compress
        .compress_vec(data, temporary_buffer, flate2::FlushCompress::Finish)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                e.message().unwrap_or("Compression Error").to_string(),
            )
        })?;

    if status != flate2::Status::StreamEnd {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Compression stream is not closed",
        ));
    }

    let mut crc = Crc::new();
    crc.update(data);

    let header = header::BGZFHeader::new(true, 0, temporary_buffer.len().try_into().unwrap());
    header.write(&mut writer)?;
    writer.write_all(&temporary_buffer)?;
    writer.write_all(&crc.sum().to_le_bytes())?;
    writer.write_all(&(data.len() as u32).to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{BGZFReader, BinaryReader};

    use super::*;
    use std::fs::{self, File};
    use std::io::{BufReader, Read, Write};

    #[test]
    fn test_vcf() -> io::Result<()> {
        let mut writer = BGZFWriter::new(
            fs::File::create("target/test.vcf.gz")?,
            flate2::Compression::default(),
        );
        let mut reader = flate2::read::MultiGzDecoder::new(fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?);
        io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    #[test]
    fn test_simple() -> io::Result<()> {
        let mut writer = BGZFWriter::new(
            fs::File::create("target/simple1.txt.gz")?,
            flate2::Compression::default(),
        );
        writer.write_all(b"1234")?;
        Ok(())
    }

    #[test]
    fn test_write_bed() -> anyhow::Result<()> {
        const TEST_OUTPUT_PATH: &str = "target/test.bed.gz";

        let mut writer = BGZFWriter::new(
            fs::File::create(TEST_OUTPUT_PATH)?,
            flate2::Compression::default(),
        );

        let mut all_data = Vec::new();
        let mut data_reader =
            flate2::read::MultiGzDecoder::new(fs::File::open("testfiles/generated.bed.gz")?);
        data_reader.read_to_end(&mut all_data)?;
        writer.write_all(&all_data)?;

        std::mem::drop(data_reader);
        std::mem::drop(writer);

        let mut result_data = Vec::new();
        let mut result_reader = BGZFReader::new(BufReader::new(File::open(TEST_OUTPUT_PATH)?));
        result_reader.read_to_end(&mut result_data)?;
        assert_eq!(result_data, all_data);

        let mut result_reader = BufReader::new(File::open(TEST_OUTPUT_PATH)?);
        let mut decompress = flate2::Decompress::new(false);

        loop {
            let header = crate::header::BGZFHeader::from_reader(&mut result_reader)?;
            assert_eq!(header.comment, None);
            assert_eq!(header.file_name, None);
            assert_eq!(header.modified_time, 0);
            let block_size = header.block_size()?;
            let compressed_data_len = block_size as i64 - 20 - 6;
            let mut compressed_data = vec![0u8; compressed_data_len as usize];
            result_reader.read_exact(&mut compressed_data)?;
            let crc32 = result_reader.read_le_u32()?;
            let uncompressed_data_len = result_reader.read_le_u32()?;
            if uncompressed_data_len == 0 {
                break;
            }
            let mut decompressed_data = vec![0u8; (uncompressed_data_len) as usize];
            decompress.reset(false);
            assert_eq!(
                decompress.decompress(
                    &compressed_data,
                    &mut decompressed_data,
                    flate2::FlushDecompress::None,
                )?,
                flate2::Status::StreamEnd
            );
            assert_eq!(decompressed_data.len(), uncompressed_data_len as usize);
            let mut crc = flate2::Crc::new();
            crc.update(&decompressed_data);
            assert_eq!(crc.sum(), crc32);
        }

        let mut buf = vec![0u8; 100];
        assert_eq!(result_reader.read(&mut buf)?, 0);

        Ok(())
    }
}
