use crate::header::BGZFHeader;
use crate::*;
use std::collections::HashMap;
use std::io;
use std::io::prelude::*;

struct BGZFCache {
    position: u64,
    header: BGZFHeader,
    buffer: Vec<u8>,
}

impl BGZFCache {
    fn next_block_position(&self) -> u64 {
        let block_size: u64 = self.header.block_size().unwrap().into();
        self.position + block_size + 1
    }
}

/// A BGZF reader
///
/// Decode BGZF file with seek support.
///
pub struct BGZFReader<R: Read + Seek> {
    reader: io::BufReader<R>,
    cache: HashMap<u64, BGZFCache>,
    cache_order: Vec<u64>,
    cache_limit: usize,
    current_block: u64,
    current_position_in_block: usize,
}

impl<R: Read + Seek> BGZFReader<R> {
    pub fn new(reader: R) -> Self {
        BGZFReader::with_buf_reader(io::BufReader::new(reader))
    }
}

const DEFAULT_CACHE_LIMIT: usize = 20;

impl<R: Read + Seek> BGZFReader<R> {
    pub fn with_buf_reader(reader: io::BufReader<R>) -> Self {
        BGZFReader {
            reader,
            cache: HashMap::new(),
            cache_order: Vec::with_capacity(DEFAULT_CACHE_LIMIT),
            current_block: 0,
            cache_limit: DEFAULT_CACHE_LIMIT,
            current_position_in_block: 0,
        }
    }

    pub fn bgzf_seek(&mut self, position: u64) -> Result<(), BGZFError> {
        self.current_block = position >> 16;
        self.current_position_in_block = (position & 0xffff) as usize;
        println!(
            "Seek {} {} {}",
            position, self.current_block, self.current_position_in_block
        );
        self.load_cache(self.current_block)?;
        Ok(())
    }

    fn load_cache(&mut self, block_position: u64) -> Result<(), BGZFError> {
        if self.cache.contains_key(&block_position) {
            return Ok(());
        }
        if self.cache_limit <= self.cache_order.len() {
            let remove_block = self.cache_order.remove(0);
            self.cache.remove(&remove_block);
        }
        self.reader.seek(io::SeekFrom::Start(block_position))?;

        let header = BGZFHeader::from_reader(&mut self.reader)?;
        let mut buffer: Vec<u8> = Vec::with_capacity(1024 * 32);
        let loaded_crc32 = {
            let mut inflate =
                flate2::CrcReader::new(flate2::bufread::DeflateDecoder::new(&mut self.reader));
            inflate.read_to_end(&mut buffer)?;
            inflate.crc().sum()
        };

        let crc32 = self.reader.read_le_u32()?;
        let raw_length = self.reader.read_le_u32()?;
        if raw_length != buffer.len() as u32 {
            return Err(BGZFErrorKind::Other("Unmatched length").into());
        }
        if crc32 != loaded_crc32 {
            return Err(BGZFErrorKind::Other("Unmatched CRC32").into());
        }
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

impl<R: Read + Seek> Read for BGZFReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.cache.contains_key(&self.current_block) {
            self.load_cache(self.current_block)
                .map_err(|x| io::Error::new(io::ErrorKind::Other, format!("{}", x)))?;
        }
        let block = self.cache.get(&self.current_block).unwrap();
        let load_size = buf
            .len()
            .min(block.buffer.len() - self.current_position_in_block);
        buf.copy_from_slice(
            &block.buffer
                [self.current_position_in_block..(self.current_position_in_block + load_size)],
        );
        let extra_read_size = if load_size != buf.len() {
            self.current_block = block.next_block_position();
            self.current_position_in_block = 0;
            self.read(&mut buf[load_size..])?
        } else {
            0
        };

        Ok(load_size + extra_read_size)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    #[test]
    fn test_read() -> Result<(), BGZFError> {
        let mut reader = BGZFReader::new(File::open("testfiles/common_all_20180418_half.vcf.gz")?);
        let mut buffer: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
        reader.bgzf_seek(0)?;
        reader.bgzf_seek(reader.cache.get(&0).unwrap().next_block_position() << 16)?;
        reader.bgzf_seek(35973)?;
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(4210818610)?;
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(9618658636)?;
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(135183301012)?;
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"11\t"));
        Ok(())
    }
}
