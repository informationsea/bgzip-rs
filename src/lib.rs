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
pub mod read;
pub mod write;

use std::io;

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

pub(crate) fn bytes_le_u32(data: u32) -> [u8; 4] {
    [
        (data & 0xff) as u8,
        ((data >> 8) & 0xff) as u8,
        ((data >> 16) & 0xff) as u8,
        ((data >> 24) & 0xff) as u8,
    ]
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
}
