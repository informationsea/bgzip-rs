//! Rust implementation of bgzip
//!
//! # Example
//!
//! ## Reader example
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
//!
//! ## Writer Example
//! ```
//! use bgzip::write::BGzWriter;
//! use std::fs;
//! use std::io;
//! use std::io::prelude::*;
//!
//! # fn main() { let _ = run(); }
//! # fn run() -> io::Result<()> {
//! let data = b"0123456789ABCDEF";
//! let mut writer = BGzWriter::new(fs::File::create("tmp/test2.gz")?);
//!
//! for _ in 0..30000 {
//!     writer.write(&data[..])?;
//! }
//! # Ok(())
//! # }
//! ```

extern crate flate2;

pub mod header;
pub mod index;
pub mod read;
pub mod write;

use std::cmp::PartialOrd;
use std::io;
use std::ops::Sub;

pub(crate) trait Region {
    type T: Sub<Output = Self::T> + PartialOrd;

    fn start(&self) -> Self::T;
    fn end(&self) -> Self::T;

    fn length(&self) -> Self::T {
        self.end() - self.start()
    }

    fn contains(&self, pos: Self::T) -> bool {
        self.start() <= pos && pos < self.end()
    }
}

pub(crate) fn search_region<U: Sub<Output = U> + PartialOrd + Copy, T: Region<T = U>>(
    regions: &Vec<T>,
    pos: U,
) -> Option<usize> {
    /*
    for (i, one) in regions.iter().enumerate() {
        if one.contains(pos) {
            return Some(i);
        }
    }
    None
    */

    let mut start = 0;
    let mut end = regions.len();
    while start + 1 < end {
        let mid = (start + end) / 2;

        if regions[mid].start() <= pos {
            start = mid;
        } else {
            end = mid;
        }
    }

    if regions[start].contains(pos) {
        Some(start)
    } else {
        None
    }
}

pub(crate) fn read_until<R: io::Read>(mut reader: R, end: u8) -> io::Result<Vec<u8>> {
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

pub(crate) fn read_le_u8<R: io::Read>(mut reader: R) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf[..])?;
    Ok(buf[0])
}

pub(crate) fn read_le_u16<R: io::Read>(mut reader: R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf[..])?;
    let value: u16 = buf[0] as u16 | ((buf[1] as u16) << 8);
    Ok(value)
}

pub(crate) fn read_le_u32<R: io::Read>(mut reader: R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf[..])?;
    let value: u32 = (buf[0] as u32
        | ((buf[1] as u32) << 8)
        | ((buf[2] as u32) << 16)
        | ((buf[3] as u32) << 24))
        .into();
    Ok(value)
}

pub(crate) fn read_le_u64<R: io::Read>(mut reader: R) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf[..])?;
    let value: u64 = (buf[0] as u64
        | ((buf[1] as u64) << 8)
        | ((buf[2] as u64) << 8 * 2)
        | ((buf[3] as u64) << 8 * 3)
        | ((buf[4] as u64) << 8 * 4)
        | ((buf[5] as u64) << 8 * 5)
        | ((buf[6] as u64) << 8 * 6)
        | ((buf[7] as u64) << 8 * 7))
        .into();
    Ok(value)
}

pub(crate) fn bytes_le_u32(data: u32) -> [u8; 4] {
    [
        (data & 0xff) as u8,
        ((data >> 8) & 0xff) as u8,
        ((data >> 16) & 0xff) as u8,
        ((data >> 24) & 0xff) as u8,
    ]
}

pub(crate) fn sized_vec<T: Copy>(data: T, size: usize) -> Vec<T> {
    let mut v = Vec::new();
    for _ in 0..size {
        v.push(data);
    }
    v
}

#[cfg(test)]
mod test {
    #[test]
    fn binary_read() {
        let data = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(super::read_le_u8(&data[..]).unwrap(), 1);
        assert_eq!(super::read_le_u16(&data[..]).unwrap(), 0x0201);
        assert_eq!(super::read_le_u32(&data[..]).unwrap(), 0x04030201);
    }

    #[derive(Debug, PartialEq)]
    struct RegionImpl {
        start: u64,
        end: u64,
    }

    impl super::Region for RegionImpl {
        type T = u64;
        fn start(&self) -> u64 {
            self.start
        }

        fn end(&self) -> u64 {
            self.end
        }
    }

    #[test]
    fn search_region() {
        let mut regions = Vec::new();
        for i in 0..100 {
            regions.push(RegionImpl {
                start: i * 100,
                end: i * 100 + 20,
            });
            regions.push(RegionImpl {
                start: i * 100 + 25,
                end: (i + 1) * 100,
            });
        }

        assert_eq!(super::search_region(&regions, 0), Some(0));
        assert_eq!(super::search_region(&regions, 10), Some(0));
        assert_eq!(super::search_region(&regions, 19), Some(0));
        assert_eq!(super::search_region(&regions, 20), None);
        assert_eq!(super::search_region(&regions, 24), None);
        assert_eq!(super::search_region(&regions, 25), Some(1));
        assert_eq!(super::search_region(&regions, 99), Some(1));
        assert_eq!(super::search_region(&regions, 100), Some(2));
        assert_eq!(super::search_region(&regions, 1000), Some(20));
        assert_eq!(super::search_region(&regions, 2021), None);
    }
}
