mod error;
pub mod header;
pub mod reader;
pub mod tabix;
pub mod writer;

pub use error::{BGZFError, BGZFErrorKind};

use std::io;

pub(crate) trait BinaryReader: io::Read {
    fn read_le_u8(&mut self) -> io::Result<u8> {
        let mut buf: [u8; 1] = [0];
        self.read_exact(&mut buf)?;
        Ok(u8::from_le_bytes(buf))
    }
    fn read_le_u16(&mut self) -> io::Result<u16> {
        let mut buf: [u8; 2] = [0, 0];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }
    fn read_le_u32(&mut self) -> io::Result<u32> {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }
    fn read_le_i32(&mut self) -> io::Result<i32> {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        self.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }
    fn read_le_u64(&mut self) -> io::Result<u64> {
        let mut buf: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

impl<R: io::Read> BinaryReader for io::BufReader<R> {}
