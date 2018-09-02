extern crate flate2;

use std::cmp::min;
use std::io;
use std::io::prelude::*;

#[derive(Debug)]
pub struct BGzReader<R: io::Read + io::Seek> {
    headers: Vec<BGzBlock>,
    reader: R,
    current_pos: u64,
    current_block: usize,
    current_data: Vec<u8>,
    pos_in_block: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BGzBlock {
    header: BGzHeader,
    uncompressed_size: u64,
    uncompressed_start: u64,
    block_start: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BGzHeader {
    compression_method: u8,
    flags: u8,
    mtime: u32,
    extra_flag: u8,
    os: u8,
    xlen: u16,
    extra: Option<Vec<BGzExtra>>,
    filename: Option<Vec<u8>>,
    comment: Option<Vec<u8>>,
    crc16: Option<u16>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BGzExtra {
    si1: u8,
    si2: u8,
    data: Vec<u8>,
}

impl<R: io::Read + io::Seek> Read for BGzReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_len = 0usize;
        let mut buf_pos = 0usize;

        while buf_pos < buf.len() {
            let copyable = min(
                buf.len() - buf_pos,
                self.current_data.len() - self.pos_in_block as usize,
            );
            //println!(
            //    "{} {} {} {} {} {}",
            //    buf_pos,
            //    buf.len(),
            //    self.pos_in_block,
            //    self.current_data.len(),
            //    self.current_pos,
            //    copyable
            //);

            if copyable == 0 {
                let current_block = self.current_block;
                let current_pos = self.current_pos;
                //println!("seeking {} {}", current_block + 1, current_pos);
                let result = self.seek_helper(current_block + 1, current_pos);
                if let Err(e) = result {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        if read_len == 0 {
                            return Err(e);
                        } else {
                            return Ok(read_len);
                        }
                    } else {
                        return Err(e);
                    }
                }
            } else {
                for i in 0..copyable {
                    buf[buf_pos + i] = self.current_data[i + (self.pos_in_block as usize)];
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
        let end = (self.headers[self.headers.len() - 1].uncompressed_size
            + self.headers[self.headers.len() - 1].uncompressed_start) as i64;

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

        // TODO: use binary search
        let mut new_block = self.headers.len();
        for (i, one) in self.headers.iter().enumerate() {
            if one.uncompressed_start <= new_pos
                && new_pos < one.uncompressed_start + one.uncompressed_size
            {
                new_block = i;
            }
        }

        self.seek_helper(new_block, new_pos)?;

        Ok(new_pos)
    }
}

impl<R: io::Read + io::Seek> BGzReader<R> {
    fn seek_helper(&mut self, new_block: usize, new_pos: u64) -> io::Result<()> {
        if self.headers.len() <= new_block {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of bgzip"));
        }

        self.current_block = new_block;
        self.current_pos = new_pos;

        let current_block = &self.headers[self.current_block];

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
        self.current_data.clear();

        let mut deflater = flate2::read::DeflateDecoder::new(&compressed_data[..]);
        deflater.read_to_end(&mut self.current_data)?;

        self.pos_in_block = new_pos - current_block.uncompressed_start;

        Ok(())
    }

    pub fn new(mut reader: R) -> io::Result<BGzReader<R>> {
        let mut headers = Vec::new();
        let mut uncompressed_pos = 0;
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

            headers.push(BGzBlock {
                uncompressed_start: uncompressed_pos,
                uncompressed_size: uncompressed_size as u64,
                block_start: pos - (compressed_size + 4) as u64,
                header: new_header,
            });
            uncompressed_pos += uncompressed_size as u64;
        }

        let mut reader = BGzReader {
            headers,
            reader,
            current_pos: 0,
            current_block: 0,
            current_data: Vec::new(),
            pos_in_block: 0,
        };

        //reader.seek(io::SeekFrom::Start(0))?;
        reader.seek_helper(0, 0)?;

        Ok(reader)
    }
}

impl BGzHeader {
    pub fn read<R: io::Read>(mut reader: R) -> io::Result<BGzHeader> {
        let fileid = read_le_u16(&mut reader)?;
        if fileid != 0x8b1f {
            return Err(io::Error::new(io::ErrorKind::Other, "not gzip"));
        }
        let compression_method = read_le_u8(&mut reader)?;
        let flags = read_le_u8(&mut reader)?;
        let mtime = read_le_u32(&mut reader)?;
        let extra_flag = read_le_u8(&mut reader)?;
        let os = read_le_u8(&mut reader)?;

        let mut xlen = 0;
        let extra = if (flags & 0x04) > 0 {
            xlen = read_le_u16(&mut reader)?;
            let mut remain = xlen as i32;
            let mut extra = Vec::new();

            while remain > 0 {
                let si1 = read_le_u8(&mut reader)?;
                let si2 = read_le_u8(&mut reader)?;
                let len = read_le_u16(&mut reader)?;
                let mut extra_data = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    extra_data.push(0);
                }
                reader.read_exact(&mut extra_data)?;
                extra.push(BGzExtra {
                    si1: si1,
                    si2: si2,
                    data: extra_data,
                });
                remain -= (4 + len) as i32;
            }
            Some(extra)
        } else {
            None
        };

        let filename = if (flags & 8) > 0 {
            Some(read_until(&mut reader, 0)?)
        } else {
            None
        };

        let comment = if (flags & 0x10) > 0 {
            Some(read_until(&mut reader, 0)?)
        } else {
            None
        };

        let crc16 = if (flags & 0x01) > 0 {
            Some(read_le_u16(&mut reader)?)
        } else {
            None
        };

        Ok(BGzHeader {
            compression_method,
            flags,
            mtime,
            extra_flag,
            os,
            xlen,
            extra,
            filename,
            comment,
            crc16,
        })
    }

    pub fn is_bgzip(&self) -> bool {
        if self.compression_method != 8 {
            return false;
        }
        if self.flags != 4 {
            return false;
        }
        if let Some(extra) = &self.extra {
            for one in extra {
                if one.si1 == 66 && one.si2 == 67 {
                    return true;
                }
            }
        }
        false
    }

    pub fn header_size(&self) -> u64 {
        self.xlen as u64
            + self.filename.as_ref().map_or(0, |x| x.len()) as u64
            + self.comment.as_ref().map_or(0, |x| x.len()) as u64
            + 11
    }

    pub fn compressed_size(&self) -> Option<u64> {
        if let Some(extra) = &self.extra {
            for one in extra {
                if one.si1 == 66 && one.si2 == 67 {
                    let size = one.data[0] as u64 | (one.data[1] as u64) << 8;
                    return Some(size - self.header_size() - 8);
                }
            }
        }
        None
    }
}

fn read_until<R: io::Read>(mut reader: R, end: u8) -> io::Result<Vec<u8>> {
    let mut name = Vec::new();
    loop {
        let newbyte = read_le_u8(&mut reader)?;
        if newbyte == end {
            break;
        }
        name.push(newbyte);
    }
    Ok(name)
}

fn read_le_u8<R: io::Read>(mut reader: R) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf[..])?;
    Ok(buf[0])
}

fn read_le_u16<R: io::Read>(mut reader: R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf[..])?;
    let value: u16 = buf[0] as u16 | ((buf[1] as u16) << 8);
    Ok(value)
}

fn read_le_u32<R: io::Read>(mut reader: R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf[..])?;
    let value: u32 = (buf[0] as u32
        | ((buf[1] as u32) << 8)
        | ((buf[2] as u32) << 16)
        | ((buf[3] as u32) << 24))
        .into();
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::io::prelude::*;

    #[test]
    fn binary_read() {
        let data = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(super::read_le_u8(&data[..]).unwrap(), 1);
        assert_eq!(super::read_le_u16(&data[..]).unwrap(), 0x0201);
        assert_eq!(super::read_le_u32(&data[..]).unwrap(), 0x04030201);
    }

    #[test]
    fn read_bgzip() {
        let mut f = io::BufReader::new(
            fs::File::open("testfiles/common_all_20180418_half.vcf.gz").unwrap(),
        );
        let header = super::BGzHeader::read(&mut f).unwrap();
        let expected = super::BGzHeader {
            compression_method: 0x08,
            flags: 0x04,
            mtime: 0,
            extra_flag: 0,
            os: 0xff,
            xlen: 6,
            extra: Some(vec![super::BGzExtra {
                si1: 66,
                si2: 67,
                data: vec![0x53, 0x37],
            }]),
            filename: None,
            comment: None,
            crc16: None,
        };

        assert_eq!(header, expected);
        assert_eq!(true, header.is_bgzip());
        assert_eq!(Some(0x3753 - 6 - 19), header.compressed_size());
    }

    #[test]
    fn read_bgzip2() {
        let mut f = io::BufReader::new(
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
        assert_eq!(eof.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);

        //println!("reader : {:?}", reader);
    }

    #[test]
    fn read_gzip() {
        let mut f = io::BufReader::new(
            fs::File::open("testfiles/common_all_20180418_half-normal.vcf.gz").unwrap(),
        );
        let header = super::BGzHeader::read(&mut f).unwrap();
        let expected = super::BGzHeader {
            compression_method: 0x08,
            flags: 0x08,
            mtime: 1535813382,
            extra_flag: 0,
            os: 3,
            extra: None,
            xlen: 0,
            filename: Some(b"common_all_20180418_half-normal.vcf".to_vec()),
            comment: None,
            crc16: None,
        };

        assert_eq!(header, expected);
        assert_eq!(false, header.is_bgzip());
        assert_eq!(None, header.compressed_size());
    }
}
