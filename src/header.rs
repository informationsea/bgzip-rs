//! Gzip header parser

use std::io;

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub struct BGzHeader {
    pub compression_method: u8,
    pub flags: u8,
    pub mtime: u32,
    pub extra_flag: u8,
    pub os: u8,
    pub xlen: u16,
    pub extra: Option<Vec<BGzExtra>>,
    pub filename: Option<Vec<u8>>,
    pub comment: Option<Vec<u8>>,
    pub crc16: Option<u16>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BGzExtra {
    pub si1: u8,
    pub si2: u8,
    pub data: Vec<u8>,
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

#[cfg(test)]
mod test {
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
    use std::fs;
    use std::io;

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
