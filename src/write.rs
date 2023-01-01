use crate::header;
use flate2::write::DeflateEncoder;
use flate2::Crc;
use std::convert::TryInto;
use std::io::{self, Write};

/// A BGZF writer
pub struct BGZFWriter<W: io::Write> {
    writer: W,
    buffer: Vec<u8>,
    compressed_buffer: Vec<u8>,
    compress_block_unit: usize,
    level: flate2::Compression,
    closed: bool,
}

/// Default BGZF block size.
pub const DEFAULT_COMPRESS_BLOCK_UNIT: usize = 65280;

impl<W: io::Write> BGZFWriter<W> {
    /// Create new BGZF writer from [`std::io::Write`]
    pub fn new(writer: W, level: flate2::Compression) -> Self {
        BGZFWriter {
            writer,
            buffer: Vec::new(),
            compressed_buffer: Vec::new(),
            compress_block_unit: DEFAULT_COMPRESS_BLOCK_UNIT,
            level,
            closed: false,
        }
    }

    /// Cerate new BGZF writer with block size.
    pub fn with_block_size(writer: W, level: flate2::Compression, block_size: usize) -> Self {
        BGZFWriter {
            writer,
            buffer: Vec::new(),
            compressed_buffer: Vec::new(),
            compress_block_unit: block_size,
            level,
            closed: false,
        }
    }

    fn write_block(&mut self) -> io::Result<()> {
        self.compressed_buffer.clear();
        let uncompressed_block_size = self.compress_block_unit.min(self.buffer.len());
        let mut encoder = DeflateEncoder::new(&mut self.compressed_buffer, self.level);
        encoder.write_all(&self.buffer[..uncompressed_block_size])?;
        encoder.finish()?;

        let mut crc = Crc::new();
        crc.update(&self.buffer[..uncompressed_block_size]);

        let header =
            header::BGZFHeader::new(true, 0, self.compressed_buffer.len().try_into().unwrap());
        header.write(&mut self.writer)?;
        self.writer.write_all(&self.compressed_buffer)?;
        self.buffer.drain(..uncompressed_block_size);
        self.writer.write_all(&crc.sum().to_le_bytes())?;
        self.writer
            .write_all(&(uncompressed_block_size as u32).to_le_bytes())?;

        Ok(())
    }

    /// Write end-of-file marker and close BGZF.
    ///
    /// Explicitly call of this method is not required. Drop trait will write end-of-file marker automatically.
    /// If you need to handle I/O errors while closing, please use this method.
    pub fn close(mut self) -> io::Result<()> {
        if !self.closed {
            self.flush()?;
            self.writer.write_all(FOOTER_BYTES)?;
            self.closed = true;
        }
        Ok(())
    }
}

impl<W: io::Write> io::Write for BGZFWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        while self.compress_block_unit < self.buffer.len() {
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

const FOOTER_BYTES: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
    0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl<W: io::Write> Drop for BGZFWriter<W> {
    fn drop(&mut self) {
        if !self.closed {
            self.flush().unwrap();
            self.writer.write_all(FOOTER_BYTES).unwrap();
            self.closed = true;
        }
    }
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
            let compressed_data_len = block_size as i64 - 19 - 6;
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
