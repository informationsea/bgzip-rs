//! [`Write`] stream to create bgzip format file.
//!
//! [`Write`]: https://doc.rust-lang.org/std/io/trait.Write.html
//!
//! # Example
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

use flate2::write::DeflateEncoder;
use flate2::{Compression, CrcWriter};
use std::io;
use std::io::prelude::*;

const DEFAULT_BUFFER: usize = 65280;

#[derive(Debug)]
pub struct BGzWriter<R: io::Write> {
    writer: R,
    buffer: Vec<u8>,
}

impl<R: io::Write> BGzWriter<R> {
    pub fn new(writer: R) -> BGzWriter<R> {
        BGzWriter {
            writer,
            buffer: Vec::with_capacity(DEFAULT_BUFFER),
        }
    }
}

impl<R: io::Write> io::Write for BGzWriter<R> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for x in buf {
            self.buffer.push(*x);
            if self.buffer.len() >= DEFAULT_BUFFER {
                self.flush()?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut compressed = Vec::new();
        let crc = {
            let mut encoder =
                CrcWriter::new(DeflateEncoder::new(&mut compressed, Compression::best()));
            encoder.write(&self.buffer)?;
            encoder.crc().sum()
        };
        let compressed_len = compressed.len() + 19 + 6;

        let buflen = self.buffer.len();
        let header = [
            0x1f,
            0x8b,
            0x08,
            0x04,
            0x00,
            0x00,
            0x00,
            0x00,
            0x02,
            0xff,
            0x06,
            0x00,
            66,
            67,
            0x02,
            0x00,
            (compressed_len & 0xff) as u8,
            ((compressed_len >> 8) & buflen) as u8,
        ];
        let wrote_bytes = self.writer.write(&header[..])?;
        if wrote_bytes != header.len() {
            return Err(io::Error::new(io::ErrorKind::Other, "Cannot write header"));
        }

        let wrote_bytes = self.writer.write(&compressed[..])?;
        if wrote_bytes != compressed.len() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Cannot write compressed data",
            ));
        }

        let crc_bytes = super::bytes_le_u32(crc);
        let wrote_bytes = self.writer.write(&crc_bytes[..])?;
        if wrote_bytes != crc_bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Cannot write CRC data",
            ));
        }

        let buflen_bytes = super::bytes_le_u32(self.buffer.len() as u32);
        let wrote_bytes = self.writer.write(&buflen_bytes[..])?;
        if wrote_bytes != buflen_bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Cannot write CRC data",
            ));
        }

        self.buffer.clear();
        Ok(())
    }
}

impl<R: io::Write> Drop for BGzWriter<R> {
    fn drop(&mut self) {
        self.flush().unwrap();
        let eof_bytes = [
            0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43,
            0x02, 0x00, 0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        self.writer.write(&eof_bytes[..]).unwrap();
        self.writer.flush().unwrap();
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io;
    use std::io::prelude::*;

    #[test]
    fn test_writer() -> io::Result<()> {
        {
            let data = b"0123456789ABCDEF";
            let mut writer = super::BGzWriter::new(fs::File::create("tmp/test.gz")?);

            for _ in 0..30000 {
                writer.write(&data[..])?;
            }
        }

        {
            let mut f = io::BufReader::new(fs::File::open("tmp/test.gz").unwrap());
            let mut reader = ::read::BGzReader::new(f).unwrap();

            let mut data = [0; 10];
            reader.seek(io::SeekFrom::Start(100)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"456789ABCD", &data);

            // end of block
            reader.seek(io::SeekFrom::Start(65270)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"6789ABCDEF", &data);

            // start of block
            reader.seek(io::SeekFrom::Start(65280)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"0123456789", &data);

            // inter-block
            reader.seek(io::SeekFrom::Start(65275)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"BCDEF01234", &data);

            // inter-block
            reader.seek(io::SeekFrom::Start(195835)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"BCDEF01234", &data);

            // inter-block
            reader.seek(io::SeekFrom::Start(65270)).unwrap();
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"6789ABCDEF", &data);
            assert_eq!(10, reader.read(&mut data).unwrap());
            assert_eq!(b"0123456789", &data);

            // end of bgzip
            reader.seek(io::SeekFrom::Start(479995)).unwrap();
            assert_eq!(5, reader.read(&mut data).unwrap());
            assert_eq!(&b"BCDEF"[..], &data[..5]);

            let eof = reader.read(&mut data);
            assert_eq!(eof.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        }

        Ok(())
    }
}
