//! Random accessible and compressed [`Read`] stream.
//!
//! [`Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
//!
//! # Example
//! ```
//! use bgzip::read::BGzReader;
//! use std::fs;
//! use std::io;
//! use std::io::prelude::*;
//!
//! # fn main() { let _ = run(); }
//! # fn run() -> io::Result<()> {
//! let mut reader = BGzReader::new(fs::File::open("./testfiles/common_all_20180418_half.vcf.gz")?)?;
//! reader.seek(io::SeekFrom::Start(100))?;
//! let mut data = [0; 17];
//! assert_eq!(17, reader.read(&mut data)?);
//! assert_eq!(b"#phasing=partial\n", &data);
//! # Ok(())
//! # }
//! ```

use flate2;
use std::cmp::min;
use std::collections::BTreeMap;
use std::io;
use std::io::prelude::*;
use std::ops::Range;
use std::usize;

use super::header::*;
use super::*;

const CACHE_NUM: usize = 100;

#[derive(Debug)]
pub struct BGzReader<R: io::Read + io::Seek> {
    headers: Vec<BGzBlock>,
    reader: R,
    current_pos: u64,
    current_block: usize,
    cache_queue: Vec<usize>,
    current_data: BTreeMap<usize, Vec<u8>>,
    pos_in_block: u64,
    compressed_pos_to_block: BTreeMap<u64, usize>,
}

#[derive(Debug, PartialEq, Eq)]
struct BGzBlock {
    header: BGzHeader,
    uncompressed_range: Range<u64>,
    block_start: u64,
}

impl super::Region for BGzBlock {
    type T = u64;

    fn start(&self) -> u64 {
        self.uncompressed_range.start
    }

    fn end(&self) -> u64 {
        self.uncompressed_range.end
    }
}

impl<R: io::Read + io::Seek> io::BufRead for BGzReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let copyable = self.current_data[&self.current_block].len() - self.pos_in_block as usize;

        if copyable == 0 {
            let current_block = self.current_block;
            let current_pos = self.current_pos;
            //println!("seeking {} {}", current_block + 1, current_pos);
            let result = self.seek_helper(current_block + 1, current_pos);
            if let Err(e) = result {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(&[0u8; 0][..]);
                } else {
                    return Err(e);
                }
            }
        }

        Ok(&self.current_data[&self.current_block][(self.pos_in_block as usize)..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos_in_block += amt as u64;
        self.current_pos += amt as u64;
    }
}

impl<R: io::Read + io::Seek> Read for BGzReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_len = 0usize;
        let mut buf_pos = 0usize;

        while buf_pos < buf.len() {
            let copyable = min(
                buf.len() - buf_pos,
                self.current_data[&self.current_block].len() - self.pos_in_block as usize,
            );

            if copyable == 0 {
                let current_block = self.current_block;
                let current_pos = self.current_pos;
                //println!("seeking {} {}", current_block + 1, current_pos);
                let result = self.seek_helper(current_block + 1, current_pos);
                if let Err(e) = result {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(read_len);
                    } else {
                        return Err(e);
                    }
                }
            } else {
                for i in 0..copyable {
                    buf[buf_pos + i] =
                        self.current_data[&self.current_block][i + (self.pos_in_block as usize)];
                }
                read_len += copyable;
                buf_pos += copyable;
                self.pos_in_block += copyable as u64;
                self.current_pos += copyable as u64;
            }
        }

        Ok(read_len)
    }
}

impl<R: io::Read + io::Seek> Seek for BGzReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let end = (self.headers[self.headers.len() - 1].uncompressed_range.end) as i64;

        let mut new_pos: i64 = match pos {
            io::SeekFrom::Start(x) => x as i64,
            io::SeekFrom::Current(x) => (self.current_pos as i64 + x),
            io::SeekFrom::End(x) => end + x,
        };

        if new_pos > end {
            new_pos = end;
        }
        if new_pos < 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "invalid position"));
        }
        let new_pos = new_pos as u64;
        let new_block = super::search_region(&self.headers, new_pos).unwrap_or(self.headers.len());

        self.seek_helper(new_block, new_pos)?;

        Ok(new_pos)
    }
}

impl<R: io::Read + io::Seek> BGzReader<R> {
    fn seek_helper(&mut self, new_block: usize, new_pos: u64) -> io::Result<()> {
        if self.headers.len() <= new_block {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of bgzip"));
        }

        self.current_pos = new_pos;
        let current_block = &self.headers[new_block];
        self.current_block = new_block;

        if !self.current_data.contains_key(&self.current_block) {
            if self.current_data.len() >= CACHE_NUM {
                let to_remove = self.cache_queue.remove(0);
                self.current_data.remove(&to_remove);
            }
            //println!("load block {}/{}", new_block, self.headers.len());
            //println!("seeking");
            let _pos = self
                .reader
                .seek(io::SeekFrom::Start(current_block.block_start))?;
            //println!("seeked {} {}", _pos, current_block.block_start);

            let compressed_size = current_block.header.compressed_size().unwrap() as usize;
            let mut compressed_data = Vec::with_capacity(compressed_size);
            for _ in 0..compressed_size {
                compressed_data.push(0u8);
            }
            self.reader.read_exact(&mut compressed_data)?;

            //println!("decompressing");
            //self.current_data.clear();

            let mut data = Vec::new();
            let mut deflater = flate2::read::DeflateDecoder::new(&compressed_data[..]);
            deflater.read_to_end(&mut data)?;
            self.current_data.insert(self.current_block, data);

            self.cache_queue.push(new_block);
        } else {
            //self.cache_queue.remove_item(new_block);
        }

        self.pos_in_block = new_pos - current_block.uncompressed_range.start;

        Ok(())
    }

    pub fn seek_block(&mut self, block_start: u64, pos_in_block: u64) -> io::Result<()> {
        let target_block_index =
            *(self
                .compressed_pos_to_block
                .get(&block_start)
                .ok_or(io::Error::new(
                    io::ErrorKind::Other,
                    "Invalid block start position",
                ))?);
        let pos = self.headers[target_block_index].uncompressed_range.start + pos_in_block;
        self.seek_helper(target_block_index, pos)
    }

    pub fn seek_virtual_file_offset(&mut self, virtual_offset: u64) -> io::Result<()> {
        let block_start = virtual_offset >> 16;
        let block_pos = virtual_offset & 0xffff;
        self.seek_block(block_start, block_pos)
    }

    pub fn tell_block(&self) -> (u64, u64) {
        (
            self.headers[self.current_block].block_start
                - self.headers[self.current_block].header.header_size(),
            self.pos_in_block,
        )
    }

    pub fn tell_virtual_file_offset(&self) -> u64 {
        let (block_start, pos_in_block) = self.tell_block();
        block_start << 16 | (pos_in_block & 0xffff)
    }

    pub fn new(mut reader: R) -> io::Result<BGzReader<R>> {
        let mut headers = Vec::new();
        let mut uncompressed_pos = 0;
        let mut compressed_pos_to_block = BTreeMap::new();
        loop {
            let new_header = BGzHeader::read(&mut reader)?;
            if !new_header.is_bgzip() {
                return Err(io::Error::new(io::ErrorKind::Other, "not bgzip"));
            }
            let compressed_size = new_header.compressed_size().unwrap() as i64;
            if compressed_size == 2 {
                break;
            }

            let pos = reader.seek(io::SeekFrom::Current(compressed_size + 4))?;
            let uncompressed_size = read_le_u32(&mut reader)?;

            // TODO: fix here. should be gzip block start
            compressed_pos_to_block.insert(
                pos - (compressed_size + 4 + new_header.header_size() as i64) as u64,
                headers.len(),
            );
            headers.push(BGzBlock {
                uncompressed_range: uncompressed_pos..(uncompressed_size as u64 + uncompressed_pos),
                block_start: pos - (compressed_size + 4) as u64,
                header: new_header,
            });
            uncompressed_pos += uncompressed_size as u64;
        }

        let mut reader = BGzReader {
            headers,
            reader,
            current_pos: 0,
            current_block: usize::MAX,
            current_data: BTreeMap::new(),
            pos_in_block: 0,
            compressed_pos_to_block,
            cache_queue: Vec::new(),
        };

        //reader.seek(io::SeekFrom::Start(0))?;
        reader.seek_helper(0, 0)?;

        Ok(reader)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::io::prelude::*;

    #[test]
    fn read_bgzip3() {
        let f = io::BufReader::new(
            fs::File::open("testfiles/common_all_20180418_half.vcf.gz").unwrap(),
        );
        let mut reader = super::BGzReader::new(f).unwrap();

        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "##fileformat=VCFv4.0\n");
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "##fileDate=20180418\n");

        reader.seek(io::SeekFrom::Start(65270)).unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line,
            "1549439,0.53140927624872579,0.00014334862385321,0.00007167431192660\n"
        );

        reader.seek_virtual_file_offset(928252229).unwrap();
        assert_eq!(928252229, reader.tell_virtual_file_offset());
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "1\t8951233\trs57849116\tAG\tA\t.\t.\tRS=57849116;RSPOS=8951234;dbSNPBuildID=129;SSR=0;SAO=0;VP=0x05000008000517003e000200;GENEINFO=CA6:765;WGT=1;VC=DIV;INT;ASP;VLD;G5A;G5;KGPhase1;KGPhase3;CAF=0.5054,0.4946;COMMON=1;TOPMED=0.63554408766564729,0.36445591233435270\n");
    }

    #[test]
    fn read_bgzip2() {
        let f = io::BufReader::new(
            fs::File::open("testfiles/common_all_20180418_half.vcf.gz").unwrap(),
        );
        let mut reader = super::BGzReader::new(f).unwrap();

        let mut data = [0; 10];
        reader.seek(io::SeekFrom::Start(200)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"ield_lates", &data);

        // end of block
        reader.seek(io::SeekFrom::Start(65270)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"1549439,0.", &data);

        // start of block
        reader.seek(io::SeekFrom::Start(65280)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"5314092762", &data);

        // inter-block
        reader.seek(io::SeekFrom::Start(65275)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"39,0.53140", &data);

        // inter-block
        reader.seek(io::SeekFrom::Start(195835)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"0.41874522", &data);

        // inter-block
        reader.seek(io::SeekFrom::Start(65270)).unwrap();
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"1549439,0.", &data);
        assert_eq!(10, reader.read(&mut data).unwrap());
        assert_eq!(b"5314092762", &data);

        // end of bgzip
        reader.seek(io::SeekFrom::Start(17229634)).unwrap();
        assert_eq!(5, reader.read(&mut data).unwrap());
        assert_eq!(&b"ON=1\n"[..], &data[..5]);

        let eof = reader.read(&mut data);
        assert_eq!(0, eof.unwrap());
        //assert_eq!(eof.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);

        //println!("reader : {:?}", reader);
    }
}
