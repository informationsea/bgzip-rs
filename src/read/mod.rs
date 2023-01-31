//! BGZF reader

#[cfg(feature = "rayon")]
mod thread;

use crate::header::BGZFHeader;
use crate::write::DEFAULT_COMPRESS_UNIT_SIZE;
use crate::*;
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::prelude::*;
use std::io::{self, BufReader};

/// Load single block from reader.
///
/// This function is useful when writing your own parallelized BGZF reader.
/// Loaded buffer can be decompress with [`decompress_block`] function.
pub fn load_block<R: BufRead>(
    mut reader: R,
    buffer: &mut Vec<u8>,
) -> Result<BGZFHeader, BGZFError> {
    let header = BGZFHeader::from_reader(&mut reader)?;
    let block_size: u64 = header.block_size()?.into();
    buffer.clear();
    buffer.resize((block_size - header.header_size()).try_into().unwrap(), 0);
    reader.read_exact(buffer)?;

    Ok(header)
}

/// Decompress single BGZF block from buffer. The buffer should be loaded with [`load_block`] function.
///
/// This function is useful when writing your own parallelized BGZF reader.
/// `uncompressed_data`, `decompress` and `crc` will be cleared before using them.
/// `uncompressed_data` must be reserved enough size to store uncompressed data.
pub fn decompress_block(
    decompressed_data: &mut Vec<u8>,
    compressed_block: &[u8],
    decompress: &mut flate2::Decompress,
    crc: &mut flate2::Crc,
) -> Result<(), BGZFError> {
    let deflate_data = &compressed_block[0..(compressed_block.len() - 8)];
    decompressed_data.clear();
    decompress.reset(false);
    crc.reset();
    match decompress.decompress_vec(
        deflate_data,
        decompressed_data,
        flate2::FlushDecompress::Finish,
    ) {
        Ok(flate2::Status::StreamEnd) => (),
        Ok(_) => {
            return Err(BGZFError::Other {
                message: "deflate stream is not finished",
            })
        }
        Err(_e) => {
            return Err(BGZFError::Other {
                message: "Unknown inflate error",
            })
        }
    }

    let expected_crc_data = [
        compressed_block[(compressed_block.len() - 8)],
        compressed_block[(compressed_block.len() - 7)],
        compressed_block[(compressed_block.len() - 6)],
        compressed_block[(compressed_block.len() - 5)],
    ];
    let expected_len_data = [
        compressed_block[(compressed_block.len() - 4)],
        compressed_block[(compressed_block.len() - 3)],
        compressed_block[(compressed_block.len() - 2)],
        compressed_block[(compressed_block.len() - 1)],
    ];

    let expected_len = u32::from_le_bytes(expected_len_data);
    if expected_len != decompressed_data.len().try_into().unwrap() {
        return Err(BGZFError::Other {
            message: "unmatched length of decompressed data",
        });
    }

    let expected_crc = u32::from_le_bytes(expected_crc_data);
    crc.update(&decompressed_data);
    if expected_crc != crc.sum() {
        return Err(BGZFError::Other {
            message: "unmatched CRC32 of decompressed data",
        });
    }

    Ok(())
}

struct BGZFCache {
    position: u64,
    header: BGZFHeader,
    buffer: Vec<u8>,
}

impl BGZFCache {
    fn next_block_position(&self) -> u64 {
        let block_size: u64 = self.header.block_size().unwrap().into();
        self.position + block_size
    }
}

/// A BGZF reader
///
/// Decode BGZF file with seek support.
pub struct BGZFReader<R: BufRead + Seek> {
    reader: R,
    cache: HashMap<u64, BGZFCache>,
    cache_order: Vec<u64>,
    cache_limit: usize,
    current_block: u64,
    current_position_in_block: usize,
    eof_pos: u64,
}

const DEFAULT_CACHE_LIMIT: usize = 10;

impl<R: Read + Seek> BGZFReader<BufReader<R>> {
    /// Create a new BGZF reader from [`std::io::Read`]
    pub fn new(reader: R) -> Self {
        BGZFReader::with_buf_reader(io::BufReader::new(reader))
    }
}

impl<R: BufRead + Seek> BGZFReader<R> {
    /// Create a new BGZF reader from [`std::io::BufRead`]
    pub fn with_buf_reader(reader: R) -> Self {
        BGZFReader {
            reader,
            cache: HashMap::new(),
            cache_order: Vec::with_capacity(DEFAULT_CACHE_LIMIT),
            current_block: 0,
            cache_limit: DEFAULT_CACHE_LIMIT,
            current_position_in_block: 0,
            eof_pos: u64::MAX,
        }
    }

    /// Seek BGZF with position. This position is not equal to real file offset,
    /// but equal to virtual file offset described in [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf).
    /// Please read "4.1.1 Random access" to learn more.
    pub fn bgzf_seek(&mut self, position: u64) -> Result<(), BGZFError> {
        self.current_block = position >> 16;
        self.current_position_in_block = (position & 0xffff) as usize;
        self.load_cache(self.current_block)?;
        Ok(())
    }

    /// Get BGZF virtual file offset. This position is not equal to real file offset,
    /// but equal to virtual file offset described in [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf).
    /// Please read "4.1.1 Random access" to learn more.    
    pub fn bgzf_pos(&self) -> u64 {
        self.current_block << 16 | (self.current_position_in_block & 0xffff) as u64
    }

    fn load_cache(&mut self, block_position: u64) -> Result<(), BGZFError> {
        if self.cache.contains_key(&block_position) {
            return Ok(());
        }
        if block_position >= self.eof_pos {
            return Ok(());
        }
        if self.cache_limit <= self.cache_order.len() {
            let remove_block = self.cache_order.remove(0);
            self.cache.remove(&remove_block);
        }
        self.reader.seek(io::SeekFrom::Start(block_position))?;

        let header = match BGZFHeader::from_reader(&mut self.reader) {
            Ok(header) => header,
            Err(BGZFError::IoError(e)) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    #[cfg(feature = "log")]
                    log::warn!("Unexpected EOF: no EOF marker at the end");
                    return Ok(());
                }
                return Err(BGZFError::IoError(e));
            }
            Err(e) => return Err(e),
        };
        let mut buffer: Vec<u8> = Vec::with_capacity(DEFAULT_COMPRESS_UNIT_SIZE);
        let loaded_crc32 = {
            let mut inflate =
                flate2::CrcReader::new(flate2::bufread::DeflateDecoder::new(&mut self.reader));
            inflate.read_to_end(&mut buffer)?;
            inflate.crc().sum()
        };

        let crc32 = self.reader.read_le_u32()?;
        let raw_length = self.reader.read_le_u32()?;
        if raw_length != buffer.len() as u32 {
            return Err(BGZFError::Other {
                message: "Unmatched length",
            });
        }
        if crc32 != loaded_crc32 {
            return Err(BGZFError::Other {
                message: "Unmatched CRC32",
            });
        }

        // EOF marker
        if raw_length == 0 {
            self.eof_pos = block_position;
        }

        self.cache_order.push(block_position);
        self.cache.insert(
            block_position,
            BGZFCache {
                position: block_position,
                header,
                buffer,
            },
        );

        Ok(())
    }
}

impl<R: BufRead + Seek> BufRead for BGZFReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if !self.cache.contains_key(&self.current_block) {
            self.load_cache(self.current_block)
                .map_err(|x| io::Error::new(io::ErrorKind::Other, format!("{}", x)))?;
        }

        let block = match self.cache.get(&self.current_block) {
            Some(value) => value,
            None => return Ok(&[]),
        };
        let remain_bytes = block.buffer.len() - self.current_position_in_block;

        if remain_bytes > 0 {
            return Ok(&block.buffer[self.current_position_in_block..]);
        }
        Ok(&[])
    }

    fn consume(&mut self, amt: usize) {
        let block = self.cache.get(&self.current_block).expect("No cache data");
        let remain_bytes = block.buffer.len() - self.current_position_in_block;
        if amt <= remain_bytes {
            self.current_position_in_block += amt;
            if self.current_position_in_block == block.buffer.len() {
                self.current_block = block.next_block_position();
                self.current_position_in_block = 0;
            }
        } else {
            unreachable!()
        }
    }
}

impl<R: BufRead + Seek> Read for BGZFReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.cache.contains_key(&self.current_block) {
            self.load_cache(self.current_block)
                .map_err(|x| io::Error::new(io::ErrorKind::Other, format!("{}", x)))?;
        }
        let block = match self.cache.get(&self.current_block) {
            Some(v) => v,
            None => return Ok(0),
        };
        let load_size = buf
            .len()
            .min(block.buffer.len() - self.current_position_in_block);
        buf[..load_size].copy_from_slice(
            &block.buffer
                [self.current_position_in_block..(self.current_position_in_block + load_size)],
        );
        let extra_read_size = if load_size != buf.len() {
            //println!("additional load");
            self.current_block = block.next_block_position();
            self.current_position_in_block = 0;
            self.read(&mut buf[load_size..])?
        } else {
            //println!("OK");
            self.current_position_in_block += load_size;
            if self.current_position_in_block == block.buffer.len() {
                // println!("Prepare");
                self.current_block = block.next_block_position();
                self.current_position_in_block = 0;
            }
            0
        };

        Ok(load_size + extra_read_size)
    }
}

#[cfg(test)]
mod test {
    use flate2::Crc;

    use super::*;
    use std::fs::File;

    #[test]
    fn test_load_block() -> Result<(), BGZFError> {
        let mut crc = Crc::new();
        let mut expected_reader = io::BufReader::new(flate2::read::MultiGzDecoder::new(
            File::open("testfiles/common_all_20180418_half.vcf.gz")?,
        ));
        let mut buf = [0u8; 1024 * 100];
        loop {
            let read_bytes = expected_reader.read(&mut buf[..])?;
            if read_bytes == 0 {
                break;
            }
            crc.update(&buf[0..read_bytes]);
        }
        let original_crc = crc.sum();

        let mut reader =
            io::BufReader::new(File::open("testfiles/common_all_20180418_half.vcf.gz")?);

        let mut block_data = Vec::new();
        let mut decompress = flate2::Decompress::new(false);
        let mut data_crc = flate2::Crc::new();
        let mut decompressed_data = Vec::with_capacity(crate::write::MAXIMUM_COMPRESS_UNIT_SIZE);

        loop {
            load_block(&mut reader, &mut block_data)?;
            if block_data == &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
                break;
            }

            decompress_block(
                &mut decompressed_data,
                &block_data,
                &mut decompress,
                &mut crc,
            )?;

            data_crc.update(&decompressed_data);
        }

        assert_eq!(original_crc, data_crc.sum());

        Ok(())
    }

    #[test]
    fn test_read() -> Result<(), BGZFError> {
        let mut expected_reader = io::BufReader::new(flate2::read::MultiGzDecoder::new(
            File::open("testfiles/common_all_20180418_half.vcf.gz")?,
        ));
        let mut reader = BGZFReader::new(File::open("testfiles/common_all_20180418_half.vcf.gz")?);

        let mut line1 = String::new();
        let mut line2 = String::new();
        for _ in 0..1000 {
            line1.clear();
            line2.clear();
            reader.read_line(&mut line1)?;
            expected_reader.read_line(&mut line2)?;
            assert_eq!(line1, line2);
            //println!("line: {}", line);
        }
        for _ in 0..1000 {
            let mut buf1: [u8; 1000] = [0; 1000];
            let mut buf2: [u8; 1000] = [0; 1000];
            let read_len1 = reader.read(&mut buf1)?;
            expected_reader.read_exact(&mut buf2)?;
            assert_eq!(read_len1, buf1.len());
            assert_eq!(&buf1[..], &buf2[..]);
        }

        let mut buffer: [u8; 8] = [0; 8];
        reader.bgzf_seek(35973)?;
        assert_eq!(reader.bgzf_pos(), 35973);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(reader.cache.get(&0).unwrap().next_block_position() << 16)?;
        reader.bgzf_seek(4210818610)?;
        assert_eq!(reader.bgzf_pos(), 4210818610);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(9618658636)?;
        assert_eq!(reader.bgzf_pos(), 9618658636);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(135183301012)?;
        assert_eq!(reader.bgzf_pos(), 135183301012);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"11\t"));
        Ok(())
    }

    #[test]
    fn test_read_all() -> anyhow::Result<()> {
        let mut expected_data_reader =
            flate2::read::MultiGzDecoder::new(File::open("testfiles/generated.bed.gz")?);
        let mut expected_data = Vec::new();
        expected_data_reader.read_to_end(&mut expected_data)?;

        let mut data_reader = crate::BGZFReader::new(File::open("testfiles/generated.bed.gz")?);
        let mut data = Vec::new();
        data_reader.read_to_end(&mut data)?;
        assert_eq!(data, expected_data);

        Ok(())
    }
}
