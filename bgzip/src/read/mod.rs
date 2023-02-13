//! BGZF reader

#[cfg(feature = "rayon")]
mod thread;

#[cfg(feature = "rayon")]
pub use thread::BGZFMultiThreadReader;

use crate::deflate::*;
use crate::index::BGZFIndex;
use crate::{header::BGZFHeader, BGZFError};
use std::convert::TryInto;
use std::io::{self, prelude::*};

/// Load single block from reader.
///
/// This function is useful when writing your own parallelized BGZF reader.
/// Loaded buffer can be decompress with [`decompress_block`] function.
pub fn load_block<R: Read>(mut reader: R, buffer: &mut Vec<u8>) -> Result<BGZFHeader, BGZFError> {
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
pub fn decompress_block(
    decompressed_data: &mut Vec<u8>,
    compressed_block: &[u8],
    decompress: &mut Decompress,
) -> Result<(), BGZFError> {
    let original_decompress_data_len = decompressed_data.len();
    let mut crc = Crc::new();

    let expected_len_data = [
        compressed_block[(compressed_block.len() - 4)],
        compressed_block[(compressed_block.len() - 3)],
        compressed_block[(compressed_block.len() - 2)],
        compressed_block[(compressed_block.len() - 1)],
    ];
    let expected_len: usize = u32::from_le_bytes(expected_len_data).try_into().unwrap();
    decompressed_data.resize(original_decompress_data_len + expected_len, 0);

    decompress.decompress(
        compressed_block,
        &mut decompressed_data[original_decompress_data_len..],
    )?;

    let expected_crc_data = [
        compressed_block[(compressed_block.len() - 8)],
        compressed_block[(compressed_block.len() - 7)],
        compressed_block[(compressed_block.len() - 6)],
        compressed_block[(compressed_block.len() - 5)],
    ];

    let expected_crc = u32::from_le_bytes(expected_crc_data);
    crc.update(&decompressed_data[original_decompress_data_len..]);
    if expected_crc != crc.sum() {
        return Err(BGZFError::Other("unmatched CRC32 of decompressed data"));
    }

    Ok(())
}

/// A BGZF reader
///
/// Decode BGZF file with seek support.
pub struct BGZFReader<R: Read> {
    reader: R,
    decompress: Decompress,
    compressed_buffer: Vec<u8>,
    current_buffer: Vec<u8>,
    current_block: u64,
    next_block: u64,
    current_position_in_block: usize,
    eof_pos: u64,
}

impl<R: Read + Seek> BGZFReader<R> {
    /// Seek BGZF with position. This position is not equal to real file offset,
    /// but equal to virtual file offset described in [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf).
    /// Please read "4.1.1 Random access" to learn more.
    pub fn bgzf_seek(&mut self, position: u64) -> Result<(), BGZFError> {
        self.next_block = position >> 16;
        self.reader.seek(io::SeekFrom::Start(self.next_block))?;
        self.load_next()?;
        self.current_position_in_block = (position & 0xffff) as usize;

        Ok(())
    }
}

impl<R: Read> BGZFReader<R> {
    /// Create a new BGZF reader from [`std::io::Read`]
    pub fn new(mut reader: R) -> Result<Self, BGZFError> {
        let mut decompress = Decompress::new();
        let mut compressed_buffer = Vec::new();
        load_block(&mut reader, &mut compressed_buffer)?;
        let mut buffer = Vec::new();
        decompress_block(&mut buffer, &compressed_buffer, &mut decompress)?;

        Ok(BGZFReader {
            reader,
            decompress,
            current_buffer: buffer,
            current_block: 0,
            next_block: compressed_buffer.len().try_into().unwrap(),
            current_position_in_block: 0,
            eof_pos: u64::MAX,
            compressed_buffer,
        })
    }

    /// Get BGZF virtual file offset. This position is not equal to real file offset,
    /// but equal to virtual file offset described in [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf).
    /// Please read "4.1.1 Random access" to learn more.    
    pub fn bgzf_pos(&self) -> u64 {
        self.current_block << 16 | (self.current_position_in_block & 0xffff) as u64
    }

    fn load_next(&mut self) -> Result<(), BGZFError> {
        if self.next_block >= self.eof_pos {
            return Ok(());
        }

        self.compressed_buffer.clear();
        let header = load_block(&mut self.reader, &mut self.compressed_buffer)?;
        let header_size = header.header_size();
        if self.compressed_buffer == crate::EOF_MARKER {
            self.eof_pos = self.next_block;
            self.current_buffer.clear();
            self.current_block = self.next_block;
            self.current_position_in_block = 0;
            return Ok(());
        }

        self.current_buffer.clear();
        decompress_block(
            &mut self.current_buffer,
            &self.compressed_buffer,
            &mut self.decompress,
        )?;
        self.current_block = self.next_block;
        let current_block_size: u64 = self.compressed_buffer.len().try_into().unwrap();
        self.next_block += current_block_size + header_size;
        self.current_position_in_block = 0;

        Ok(())
    }
}

impl<R: Read> BufRead for BGZFReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.current_position_in_block >= self.current_buffer.len() {
            self.load_next().map_err(|e| e.into_io_error())?;
        }

        let remain_bytes = self.current_buffer.len() - self.current_position_in_block;

        if remain_bytes > 0 {
            return Ok(&self.current_buffer[self.current_position_in_block..]);
        }
        Ok(&[])
    }

    fn consume(&mut self, amt: usize) {
        let remain_bytes = self.current_buffer.len() - self.current_position_in_block;
        if amt <= remain_bytes {
            self.current_position_in_block += amt;
        } else {
            unreachable!()
        }
    }
}

impl<R: Read> Read for BGZFReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        //eprintln!("read start: {}", buf.len());
        let internal_buf = self.fill_buf()?;
        let bytes_to_copy = buf.len().min(internal_buf.len());
        buf[0..bytes_to_copy].copy_from_slice(&internal_buf[0..bytes_to_copy]);
        self.consume(bytes_to_copy);
        //eprintln!("read end: {}", bytes_to_copy);
        Ok(bytes_to_copy)
    }
}

/// Seekable BGZF reader.
pub struct IndexedBGZFReader<R: Read + Seek> {
    reader: BGZFReader<R>,
    index: BGZFIndex,
    current_pos: u64,
    end_pos: u64,
}

impl<R: Read + Seek> IndexedBGZFReader<R> {
    /// Create new [`IndexedBGZFReader`] from [`BGZFReader`] and [`BGZFIndex`].
    pub fn new(mut reader: BGZFReader<R>, index: BGZFIndex) -> Result<Self, BGZFError> {
        let last_entry = index
            .entries
            .last()
            .ok_or(BGZFError::Other("Invalid index file"))?
            .clone();
        reader.bgzf_seek(last_entry.compressed_offset << 16)?;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        reader.bgzf_seek(0)?;
        std::mem::drop(last_entry);

        Ok(IndexedBGZFReader {
            reader,
            index,
            current_pos: 0,
            end_pos: last_entry.uncompressed_offset + TryInto::<u64>::try_into(buf.len()).unwrap(),
        })
    }
}

impl IndexedBGZFReader<std::fs::File> {
    /// Create new [`IndexedBGZFReader`] from file path.
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self, BGZFError> {
        let reader = BGZFReader::new(std::fs::File::open(path.as_ref())?)?;
        let index = BGZFIndex::from_reader(std::fs::File::open(
            path.as_ref()
                .to_str()
                .ok_or(BGZFError::PathConvertionError)?,
        )?)?;
        IndexedBGZFReader::new(reader, index)
    }
}

impl<R: Read + Seek> Seek for IndexedBGZFReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            io::SeekFrom::Current(p) => {
                TryInto::<u64>::try_into(p + TryInto::<i64>::try_into(self.current_pos).unwrap())
                    .unwrap()
            }
            io::SeekFrom::Start(p) => p,
            io::SeekFrom::End(p) => {
                TryInto::<u64>::try_into(TryInto::<i64>::try_into(self.end_pos).unwrap() + p)
                    .unwrap()
            }
        };
        self.reader
            .bgzf_seek(
                self.index
                    .uncompressed_pos_to_bgzf_pos(new_pos)
                    .map_err(|x| Into::<io::Error>::into(x))?,
            )
            .map_err(|x| Into::<io::Error>::into(x))?;
        Ok(new_pos)
    }
}

impl<R: Read + Seek> BufRead for IndexedBGZFReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt);
        self.current_pos += TryInto::<u64>::try_into(amt).unwrap();
    }
}

impl<R: Read + Seek> Read for IndexedBGZFReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let s = self.reader.read(buf)?;
        self.current_pos += TryInto::<u64>::try_into(s).unwrap();
        Ok(s)
    }
}

#[cfg(test)]
mod test {
    use flate2::Crc;

    use crate::BGZFWriter;

    use super::*;
    use rand::prelude::*;
    use std::fs::{self, File};

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
        let mut data_crc = Crc::new();
        let mut decompress = super::Decompress::new();
        let mut decompressed_data = Vec::with_capacity(crate::write::MAXIMUM_COMPRESS_UNIT_SIZE);

        loop {
            load_block(&mut reader, &mut block_data)?;
            if block_data == &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
                break;
            }

            decompressed_data.clear();
            decompress_block(&mut decompressed_data, &block_data, &mut decompress)?;

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
        let mut reader = BGZFReader::new(File::open("testfiles/common_all_20180418_half.vcf.gz")?)?;

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
            reader.read_exact(&mut buf1)?;
            expected_reader.read_exact(&mut buf2)?;
            //assert_eq!(read_len1, buf1.len());
            assert_eq!(&buf1[..], &buf2[..]);
        }

        let mut buffer = [0; 30];

        reader.bgzf_seek(0)?;
        assert_eq!(reader.bgzf_pos(), 0);

        reader.bgzf_seek(35973)?;
        assert_eq!(reader.bgzf_pos(), 35973);
        reader.read_exact(&mut buffer)?;
        assert!(
            buffer.starts_with(b"1\t4008153"),
            "{}",
            String::from_utf8_lossy(&buffer)
        );
        //reader.bgzf_seek(reader.cache.get(&0).unwrap().next_block_position() << 16)?;
        reader.bgzf_seek(4210818610)?;
        assert_eq!(reader.bgzf_pos(), 4210818610);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t72700625"));
        //eprintln!("data: {}", String::from_utf8_lossy(&buffer));
        reader.bgzf_seek(9618658636)?;
        assert_eq!(reader.bgzf_pos(), 9618658636);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"1\t"));
        reader.bgzf_seek(135183301012)?;
        assert_eq!(reader.bgzf_pos(), 135183301012);
        reader.read_exact(&mut buffer)?;
        assert!(buffer.starts_with(b"11\t"));

        let mut tmp_buf = vec![0u8; 391474];
        reader.bgzf_seek(0)?;
        reader.read_exact(&mut tmp_buf)?;
        //eprintln!("data: {}", String::from_utf8_lossy(&buffer));
        assert_eq!(reader.bgzf_pos(), 4210818610);
        reader.read_exact(&mut buffer)?;
        assert!(
            buffer.starts_with(b"1\t72700625"),
            "{}",
            String::from_utf8_lossy(&buffer)
        );

        Ok(())
    }

    #[test]
    fn test_read_all() -> anyhow::Result<()> {
        let mut expected_data_reader =
            flate2::read::MultiGzDecoder::new(File::open("testfiles/generated.bed.gz")?);
        let mut expected_data = Vec::new();
        expected_data_reader.read_to_end(&mut expected_data)?;

        let mut data_reader = crate::BGZFReader::new(File::open("testfiles/generated.bed.gz")?)?;
        let mut data = Vec::new();
        data_reader.read_to_end(&mut data)?;
        assert_eq!(data, expected_data);

        Ok(())
    }

    #[test]
    fn test_indexed_reader() -> anyhow::Result<()> {
        let mut data_reader = std::io::BufReader::new(flate2::read::MultiGzDecoder::new(
            fs::File::open("testfiles/generated.bed.gz")?,
        ));
        let mut line = String::new();
        let mut line_list = Vec::new();
        let mut writer = BGZFWriter::new(
            fs::File::create("tmp/test-indexed-reader.bed.gz")?,
            Compression::default(),
        );

        let mut total_len = 0;
        loop {
            let bgzf_pos = writer.bgzf_pos();
            let uncompressed_pos = writer.pos();
            line.clear();
            let size = data_reader.read_line(&mut line)?;
            if size == 0 {
                break;
            }
            writer.write_all(&line.as_bytes())?;
            total_len += line.as_bytes().len();
            line_list.push((bgzf_pos, uncompressed_pos, line.clone()));
        }
        let index = writer.close()?.unwrap();

        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(0x9387402456157523);
        let mut reader = IndexedBGZFReader::new(
            BGZFReader::new(fs::File::open("tmp/test-indexed-reader.bed.gz")?)?,
            index,
        )?;

        line.clear();
        reader.read_line(&mut line)?;
        assert_eq!(line, line_list[0].2);

        for _ in 0..300 {
            let i = rand.gen_range(0..line_list.len());
            reader.seek(std::io::SeekFrom::Start(line_list[i].1))?;
            line.clear();
            reader.read_line(&mut line)?;
            assert_eq!(line, line_list[i].2);
        }

        assert_eq!(TryInto::<u64>::try_into(total_len).unwrap(), reader.end_pos);

        Ok(())
    }
}
