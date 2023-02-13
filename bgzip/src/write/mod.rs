//! BGZF writer

#[cfg(feature = "rayon")]
mod thread;

#[cfg(feature = "rayon")]
pub use thread::BGZFMultiThreadWriter;

use crate::header::BGZFHeader;
use crate::index::{BGZFIndex, BGZFIndexEntry};
use crate::{deflate::*, BGZFError};
use std::convert::TryInto;
use std::io::{self, Write};

// #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
// pub struct BGZFWritePos {
//     block_index: u64,
//     wrote_bytes: u64,
//     position_in_block: u64,
//     block_position: Option<u64>,
// }

/// A BGZF writer
pub struct BGZFWriter<W: io::Write> {
    writer: W,
    original_data: Vec<u8>,
    compressed_buffer: Vec<u8>,
    compress: Compress,
    compress_unit_size: usize,
    closed: bool,
    current_compressed_pos: u64,
    current_uncompressed_pos: u64,
    bgzf_index: Option<BGZFIndex>,
}

/// Default BGZF compress unit size
pub const DEFAULT_COMPRESS_UNIT_SIZE: usize = 65280;

/// Maximum BGZF compress unit size
pub const MAXIMUM_COMPRESS_UNIT_SIZE: usize = 64 * 1024;

pub(crate) const EXTRA_COMPRESS_BUFFER_SIZE: usize = 200;

impl<W: io::Write> BGZFWriter<W> {
    /// Create new BGZF writer from [`std::io::Write`]
    pub fn new(writer: W, level: Compression) -> Self {
        Self::with_compress_unit_size(writer, level, DEFAULT_COMPRESS_UNIT_SIZE, true)
            .expect("Unreachable (BGZFWriter)")
    }

    /// Cerate new BGZF writer with compress unit size.
    ///
    /// Default value of compress unit size is 65280.
    pub fn with_compress_unit_size(
        writer: W,
        level: Compression,
        compress_unit_size: usize,
        create_index: bool,
    ) -> Result<Self, BGZFError> {
        if compress_unit_size >= crate::write::MAXIMUM_COMPRESS_UNIT_SIZE {
            return Err(BGZFError::TooLargeCompressUnit);
        }

        Ok(BGZFWriter {
            writer,
            original_data: Vec::with_capacity(compress_unit_size),
            compressed_buffer: Vec::with_capacity(compress_unit_size + EXTRA_COMPRESS_BUFFER_SIZE),
            compress_unit_size,
            compress: Compress::new(level),
            closed: false,
            current_uncompressed_pos: 0,
            current_compressed_pos: 0,
            bgzf_index: if create_index {
                Some(BGZFIndex::new())
            } else {
                None
            },
        })
    }

    /// Get BGZF virtual file offset. This position is not equal to real file offset,
    /// but equal to virtual file offset described in [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf).
    /// Please read "4.1.1 Random access" to learn more.       
    pub fn bgzf_pos(&self) -> u64 {
        self.current_compressed_pos << 16 | (self.original_data.len() & 0xffff) as u64
    }

    /// Current write position.
    pub fn pos(&self) -> u64 {
        self.current_uncompressed_pos + TryInto::<u64>::try_into(self.original_data.len()).unwrap()
    }

    fn write_block(&mut self) -> io::Result<()> {
        self.compressed_buffer.clear();
        write_block(
            &mut self.compressed_buffer,
            &self.original_data,
            &mut self.compress,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        self.writer.write_all(&self.compressed_buffer)?;

        self.current_uncompressed_pos +=
            TryInto::<u64>::try_into(self.original_data.len()).unwrap();
        self.current_compressed_pos +=
            TryInto::<u64>::try_into(self.compressed_buffer.len()).unwrap();

        if let Some(index) = self.bgzf_index.as_mut() {
            index.entries.push(BGZFIndexEntry {
                compressed_offset: self.current_compressed_pos,
                uncompressed_offset: self.current_uncompressed_pos,
            });
        }

        Ok(())
    }

    /// Write end-of-file marker and close BGZF.
    ///
    /// Explicitly call of this method is not required unless you need .gzi index.
    /// Drop trait will write end-of-file marker automatically.
    /// If you need to handle I/O errors while closing, please use this method.
    pub fn close(mut self) -> io::Result<Option<BGZFIndex>> {
        if !self.closed {
            self.flush()?;
            self.writer.write_all(&crate::EOF_MARKER)?;
            self.closed = true;
        }

        if let Some(index) = self.bgzf_index.as_mut() {
            index.entries.pop();
        }

        Ok(self.bgzf_index.take())
    }
}

impl<W: io::Write> io::Write for BGZFWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut process_start_pos = 0;
        loop {
            //eprintln!("process start pos: {}", process_start_pos);
            let to_write_bytes = (buf.len() - process_start_pos)
                .min(self.compress_unit_size - self.original_data.len());
            if to_write_bytes == 0 {
                break;
            }
            self.original_data
                .extend_from_slice(&buf[process_start_pos..(process_start_pos + to_write_bytes)]);
            if self.original_data.len() >= self.compress_unit_size {
                self.write_block()?;
                self.original_data.clear();
            }
            process_start_pos += to_write_bytes;
        }

        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if !self.original_data.is_empty() {
            self.write_block()?;
        }
        Ok(())
    }
}

impl<W: io::Write> Drop for BGZFWriter<W> {
    fn drop(&mut self) {
        if !self.closed {
            self.flush().unwrap();
            self.writer.write_all(&crate::EOF_MARKER).unwrap();
            self.closed = true;
        }
    }
}

const FOOTER_SIZE: usize = 8;

/// Write single BGZF block to writer.
///
/// This function is useful when writing your own parallelized BGZF writer.
pub fn write_block(
    compressed_data: &mut Vec<u8>,
    original_data: &[u8],
    compress: &mut Compress,
) -> Result<usize, CompressError> {
    //eprintln!("write block : {} ", original_data.len());
    let original_compressed_data_size = compressed_data.len();
    let mut header = BGZFHeader::new(false, 0, 0);
    let header_size: usize = header.header_size().try_into().unwrap();
    compressed_data.resize(
        original_compressed_data_size
            + original_data.len()
            + EXTRA_COMPRESS_BUFFER_SIZE
            + header_size
            + FOOTER_SIZE,
        0,
    );

    let compressed_len = compress.compress(
        original_data,
        &mut compressed_data[(original_compressed_data_size + header_size)..],
    )?;
    compressed_data.truncate(original_compressed_data_size + header_size + compressed_len);

    let mut crc = Crc::new();
    crc.update(original_data);

    compressed_data.extend_from_slice(&crc.sum().to_le_bytes());
    compressed_data.extend_from_slice(&(original_data.len() as u32).to_le_bytes());

    let block_size = compressed_data.len() - original_compressed_data_size;
    //eprintln!("block size: {} / {}", block_size, original_data.len());
    header
        .update_block_size(block_size.try_into().unwrap())
        .expect("Unreachable");

    header
        .write(
            &mut compressed_data
                [original_compressed_data_size..(header_size + original_compressed_data_size)],
        )
        .expect("Failed to write header");

    Ok(block_size)
}

#[cfg(test)]
mod test {
    use crate::BGZFReader;
    use crate::{deflate::Compression, BinaryReader};
    use rand::prelude::*;

    use super::*;
    use std::fs::{self, File};
    use std::io::{BufRead, BufReader, Read, Write};

    #[test]
    fn test_vcf() -> anyhow::Result<()> {
        let mut data = Vec::new();
        let mut reader = flate2::read::MultiGzDecoder::new(fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?);
        reader.read_to_end(&mut data)?;

        let output_path = "tmp/test.vcf.gz";
        let mut writer = BGZFWriter::new(fs::File::create(output_path)?, Compression::default());
        writer.write_all(&data)?;
        writer
            .close()?
            .unwrap()
            .write(fs::File::create(format!("{}.gzi", output_path))?)?;

        let mut reader = flate2::read::MultiGzDecoder::new(fs::File::open(output_path)?);
        let mut wrote_data = Vec::new();
        reader.read_to_end(&mut wrote_data)?;
        assert_eq!(wrote_data.len(), data.len());

        Ok(())
    }

    #[test]
    fn test_simple() -> anyhow::Result<()> {
        let output_path = "tmp/simple1.txt.gz";
        let mut writer = BGZFWriter::new(fs::File::create(output_path)?, Compression::default());
        writer.write_all(b"1234")?;
        std::mem::drop(writer);
        let mut reader = flate2::read::MultiGzDecoder::new(std::fs::File::open(output_path)?);
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        assert_eq!(data, b"1234");
        Ok(())
    }

    #[test]
    fn test_write_bed() -> anyhow::Result<()> {
        const TEST_OUTPUT_PATH: &str = "tmp/test.bed.gz";

        let mut writer =
            BGZFWriter::new(fs::File::create(TEST_OUTPUT_PATH)?, Compression::default());

        let mut all_data = Vec::new();
        let mut data_reader =
            flate2::read::MultiGzDecoder::new(fs::File::open("testfiles/generated.bed.gz")?);
        data_reader.read_to_end(&mut all_data)?;
        writer.write_all(&all_data)?;

        std::mem::drop(data_reader);
        std::mem::drop(writer);

        let mut result_data = Vec::new();
        let mut result_reader =
            flate2::read::MultiGzDecoder::new(BufReader::new(File::open(TEST_OUTPUT_PATH)?));
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

    #[test]
    fn test_bgzf_pos() -> anyhow::Result<()> {
        let mut data_reader = std::io::BufReader::new(flate2::read::MultiGzDecoder::new(
            fs::File::open("testfiles/generated.bed.gz")?,
        ));
        let mut line = String::new();
        let mut line_list = Vec::new();
        let mut writer = BGZFWriter::new(
            fs::File::create("tmp/write-pos.bed.gz")?,
            Compression::default(),
        );

        loop {
            let pos = writer.bgzf_pos();
            line.clear();
            let size = data_reader.read_line(&mut line)?;
            if size == 0 {
                break;
            }
            writer.write_all(&line.as_bytes())?;
            line_list.push((pos, line.clone()));
        }
        writer.close()?;

        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(0x9387402456157523);
        let mut reader = BGZFReader::new(fs::File::open("tmp/write-pos.bed.gz")?)?;
        for _ in 0..300 {
            let i = rand.gen_range(0..line_list.len());
            reader.bgzf_seek(line_list[i].0)?;
            line.clear();
            reader.read_line(&mut line)?;
            assert_eq!(line, line_list[i].1);
        }

        Ok(())
    }
}
