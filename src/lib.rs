//! bgzip-rs
//! ========
//! [![Build](https://github.com/informationsea/bgzip-rs/actions/workflows/build.yml/badge.svg)](https://github.com/informationsea/bgzip-rs/actions/workflows/build.yml)
//! [![Creates.io](http://meritbadge.herokuapp.com/bgzip)](https://crates.io/crates/bgzip)
//! [![doc-rs](https://docs.rs/bgzip/badge.svg)](https://docs.rs/bgzip)
//!
//! Rust implementation of [BGZF format](https://samtools.github.io/hts-specs/SAMv1.pdf)
//!
//! Feature flags
//! -------------
//!
//! * `rayon`: Enable [rayon](https://github.com/rayon-rs/rayon) based multi-threaded writer. This is default feature.
//! * `log`: Enable [log](https://github.com/rust-lang/log) crate to log warnings. This is default feature.
//! * `rust_backend`: use `miniz_oxide` crate for [flate2](https://github.com/rust-lang/flate2-rs) backend. This is default feature.
//! * `zlib`: use `zlib` for flate2 backend. Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
//! * `zlib-ng`: use `zlib-ng` for flate2 backend. Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
//! * `zlib-ng-compat`: Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
//! * `cloudflare_zlib`: Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
//!
//! Write Examples
//! --------
//! ```rust
//! use bgzip::{BGZFWriter, BGZFError};
//! use std::io::{self, Write};
//! fn main() -> Result<(), BGZFError> {
//!     let mut write_buffer = Vec::new();
//!     let mut writer = BGZFWriter::new(&mut write_buffer, flate2::Compression::default());
//!     writer.write_all(b"##fileformat=VCFv4.2\n")?;
//!     writer.write_all(b"#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")?;
//!     writer.close()?;
//!     Ok(())
//! }
//! ```
//!
//! Multi-thread support is available via [`BGZFMultiThreadWriter`]. `rayon` flag is required to use this feature.
//!
//! Read Examples
//! --------
//! ```rust
//! use bgzip::{BGZFReader, BGZFError};
//! use std::io::{self, BufRead};
//! use std::fs;
//! fn main() -> Result<(), BGZFError> {
//!     let mut reader =
//!     BGZFReader::new(fs::File::open("testfiles/common_all_20180418_half.vcf.gz")?);
//!     let mut line = String::new();
//!     reader.read_line(&mut line)?;
//!     assert_eq!("##fileformat=VCFv4.0\n", line);
//!     reader.bgzf_seek(4210818610)?;
//!     line.clear();
//!     reader.read_line(&mut line)?;
//!     assert_eq!("1\t72700625\trs12116859\tT\tA,C\t.\t.\tRS=12116859;RSPOS=72700625;dbSNPBuildID=120;SSR=0;SAO=0;VP=0x05010008000517053e000100;GENEINFO=LOC105378798:105378798;WGT=1;VC=SNV;SLO;INT;ASP;VLD;G5A;G5;HD;GNO;KGPhase1;KGPhase3;CAF=0.508,.,0.492;COMMON=1;TOPMED=0.37743692660550458,0.00608435270132517,0.61647872069317023\n", line);
//!
//!     Ok(())
//! }
//! ```

mod error;

pub(crate) mod csi;
/// BGZ header parser
pub mod header;
mod read;
/// Tabix file parser. (This module is alpha state.)
pub mod tabix;
#[cfg(feature = "rayon")]
mod thread;
mod write;

pub use error::BGZFError;
pub use read::BGZFReader;

#[cfg(feature = "rayon")]
pub use thread::BGZFMultiThreadWriter;
pub use write::BGZFWriter;

pub use flate2::Compression;

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

impl<R: io::Read> BinaryReader for R {}

#[cfg(test)]
mod test {
    use crate::BGZFError;
    use crate::BGZFReader;
    use crate::BGZFWriter;
    use std::fs;
    use std::io::{BufRead, Write};
    #[test]
    fn test_run() -> Result<(), BGZFError> {
        let mut write_buffer = Vec::new();
        let mut writer = BGZFWriter::new(&mut write_buffer, flate2::Compression::default());
        writer.write_all(b"##fileformat=VCFv4.2\n")?;
        writer.write_all(b"#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")?;
        writer.close()?;
        Ok(())
    }

    #[test]
    fn test_read() -> Result<(), BGZFError> {
        let mut reader =
            BGZFReader::new(fs::File::open("testfiles/common_all_20180418_half.vcf.gz")?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        assert_eq!("##fileformat=VCFv4.0\n", line);
        reader.bgzf_seek(4210818610)?;
        line.clear();
        reader.read_line(&mut line)?;
        assert_eq!("1\t72700625\trs12116859\tT\tA,C\t.\t.\tRS=12116859;RSPOS=72700625;dbSNPBuildID=120;SSR=0;SAO=0;VP=0x05010008000517053e000100;GENEINFO=LOC105378798:105378798;WGT=1;VC=SNV;SLO;INT;ASP;VLD;G5A;G5;HD;GNO;KGPhase1;KGPhase3;CAF=0.508,.,0.492;COMMON=1;TOPMED=0.37743692660550458,0.00608435270132517,0.61647872069317023\n", line);

        Ok(())
    }

    #[test]
    fn test_read_all() -> Result<(), BGZFError> {
        let reader = BGZFReader::new(fs::File::open("testfiles/common_all_20180418_half.vcf.gz")?);
        let expected_reader = std::io::BufReader::new(flate2::read::MultiGzDecoder::new(
            fs::File::open("testfiles/common_all_20180418_half.vcf.gz")?,
        ));
        for (line1, line2) in reader.lines().zip(expected_reader.lines()) {
            assert_eq!(line1?, line2?);
        }
        Ok(())
    }
}
