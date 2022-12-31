use crate::*;
use std::io;
use std::u32;

pub const GZIP_ID1: u8 = 31;
pub const GZIP_ID2: u8 = 139;

pub const BGZIP_HEADER_SIZE: u16 = 20 + 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraField {
    sub_field_id1: u8,
    sub_field_id2: u8,
    data: Vec<u8>,
}

impl ExtraField {
    pub fn new(id1: u8, id2: u8, data: Vec<u8>) -> Self {
        ExtraField {
            sub_field_id1: id1,
            sub_field_id2: id2,
            data,
        }
    }

    pub fn id1(&self) -> u8 {
        self.sub_field_id1
    }

    pub fn id2(&self) -> u8 {
        self.sub_field_id2
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn field_len(&self) -> u16 {
        self.data.len() as u16 + 4
    }

    pub fn write(&self, writer: &mut impl io::Write) -> io::Result<()> {
        writer.write_all(&[self.sub_field_id1, self.sub_field_id2])?;
        writer.write_all(&(self.data.len() as u16).to_le_bytes())?;
        writer.write_all(&self.data)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BGZFHeader {
    /// Compress Method Field.
    ///
    /// Must be [`DEFLATE`]
    pub compression_method: u8,

    /// Flags
    ///
    /// Combination of [`FLAG_FTEXT`], [`FLAG_FHCRC`], [`FLAG_FEXTRA`], [`FLAG_FNAME`] and [`FLAG_FCOMMENT`].
    pub flags: u8,

    /// Modified date in unix epoch
    ///
    /// Set `0` if unknown.
    pub modified_time: u32,

    /// Extra flags
    pub extra_flags: u8,

    /// Operation System
    ///
    /// Common values are [`FILESYSTEM_UNKNOWN`], [`FILESYSTEM_FAT`], [`FILESYSTEM_NTFS`] and [`FILESYSTEM_UNIX`].
    pub operation_system: u8,

    /// Length of extra field
    pub extra_field_len: Option<u16>,

    /// Extra field content
    pub extra_field: Vec<ExtraField>,

    /// Original filename
    pub file_name: Option<Vec<u8>>,

    /// Comment
    pub comment: Option<Vec<u8>>,

    /// CRC16 in header
    pub crc16: Option<u16>,
}

pub const DEFLATE: u8 = 8;
pub const FLAG_FTEXT: u8 = 1;
pub const FLAG_FHCRC: u8 = 2;
pub const FLAG_FEXTRA: u8 = 4;
pub const FLAG_FNAME: u8 = 8;
pub const FLAG_FCOMMENT: u8 = 16;

pub const FILESYSTEM_FAT: u8 = 0;
pub const FILESYSTEM_UNIX: u8 = 3;
pub const FILESYSTEM_NTFS: u8 = 11;
pub const FILESYSTEM_UNKNOWN: u8 = 255;

impl BGZFHeader {
    pub fn new(fast: bool, modified_time: u32, compressed_len: u16) -> Self {
        let block_size = compressed_len + BGZIP_HEADER_SIZE;
        let bgzf_field = ExtraField::new(66, 67, (block_size - 1).to_le_bytes().to_vec());

        BGZFHeader {
            compression_method: DEFLATE,
            flags: FLAG_FEXTRA,
            modified_time,
            extra_flags: if fast { 4 } else { 2 },
            operation_system: FILESYSTEM_UNKNOWN,
            extra_field_len: Some(bgzf_field.field_len()),
            extra_field: vec![bgzf_field],
            file_name: None,
            comment: None,
            crc16: None,
        }
    }

    pub fn block_size(&self) -> Result<u16, BGZFError> {
        self.extra_field
            .iter()
            .find(|x| x.sub_field_id1 == 66 && x.sub_field_id2 == 67 && x.data.len() == 2)
            .map(|x| {
                let mut bytes: [u8; 2] = [0, 0];
                bytes.copy_from_slice(&x.data[0..2]);
                u16::from_le_bytes(bytes)
            })
            .ok_or(BGZFError::NotBGZF)
    }

    pub fn header_size(&self) -> u64 {
        10u64
            + self.extra_field_len.map(|x| (x + 2).into()).unwrap_or(0)
            + self
                .file_name
                .as_ref()
                .map(|x| x.len() as u64 + if x.ends_with(&[0]) { 0 } else { 1 })
                .unwrap_or(0)
            + self
                .comment
                .as_ref()
                .map(|x| x.len() as u64 + if x.ends_with(&[0]) { 0 } else { 1 })
                .unwrap_or(0)
            + self.crc16.map(|_| 2).unwrap_or(0)
    }

    pub fn from_reader<R: io::BufRead>(reader: &mut R) -> Result<Self, BGZFError> {
        let id1 = reader.read_le_u8()?;
        let id2 = reader.read_le_u8()?;
        if id1 != GZIP_ID1 || id2 != GZIP_ID2 {
            return Err(BGZFError::NotGzip);
        }
        let compression_method = reader.read_le_u8()?;
        if compression_method != DEFLATE {
            return Err(BGZFError::Other {
                message: "Unsupported compression method",
            });
        }
        let flags = reader.read_le_u8()?;
        if flags | 0x1f != 0x1f {
            return Err(BGZFError::Other {
                message: "Unsupported flag",
            });
        }
        let modified_time = reader.read_le_u32()?;
        let extra_flags = reader.read_le_u8()?;
        let operation_system = reader.read_le_u8()?;
        let (extra_field_len, extra_field) = if flags & FLAG_FEXTRA != 0 {
            let len = reader.read_le_u16()?;
            let mut remain_bytes = len;
            let mut fields = Vec::new();
            while remain_bytes > 4 {
                let sub_field_id1 = reader.read_le_u8()?;
                let sub_field_id2 = reader.read_le_u8()?;
                let sub_field_len = reader.read_le_u16()?;
                let mut buf: Vec<u8> = vec![0; sub_field_len as usize];
                reader.read_exact(&mut buf)?;
                fields.push(ExtraField {
                    sub_field_id1,
                    sub_field_id2,
                    data: buf,
                });
                remain_bytes -= 4 + sub_field_len;
            }
            if remain_bytes != 0 {
                return Err(BGZFError::Other {
                    message: "Invalid extra field",
                });
            }

            (Some(len), fields)
        } else {
            (None, Vec::new())
        };

        let file_name = if flags & FLAG_FNAME != 0 {
            let mut buf = Vec::new();
            reader.read_until(0, &mut buf)?;
            Some(buf)
        } else {
            None
        };

        let comment = if flags & FLAG_FCOMMENT != 0 {
            let mut buf = Vec::new();
            reader.read_until(0, &mut buf)?;
            Some(buf)
        } else {
            None
        };

        let crc16 = if flags & FLAG_FHCRC != 0 {
            Some(reader.read_le_u16()?)
        } else {
            None
        };

        Ok(BGZFHeader {
            compression_method,
            flags,
            modified_time,
            extra_flags,
            operation_system,
            extra_field_len,
            extra_field,
            file_name,
            comment,
            crc16,
        })
    }

    pub fn write(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let mut calculated_flags = self.flags & FLAG_FTEXT;
        if self.file_name.is_some() {
            calculated_flags |= FLAG_FNAME;
        }
        if self.comment.is_some() {
            calculated_flags |= FLAG_FCOMMENT;
        }
        if self.crc16.is_some() {
            calculated_flags |= FLAG_FHCRC;
        }
        if self.extra_field_len.is_some() {
            calculated_flags |= FLAG_FEXTRA;
        }
        if calculated_flags != self.flags {
            return Err(io::Error::new(io::ErrorKind::Other, "Invalid bgzip flag"));
        }

        writer.write_all(&[
            GZIP_ID1,
            GZIP_ID2,
            self.compression_method,
            calculated_flags,
        ])?;
        writer.write_all(&self.modified_time.to_le_bytes())?;
        writer.write_all(&[self.extra_flags, self.operation_system])?;
        if let Some(extra_field_len) = self.extra_field_len {
            let total_xlen: u16 = self.extra_field.iter().map(|x| x.field_len()).sum();
            if total_xlen != extra_field_len {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Invalid bgzip extra field length",
                ));
            }

            writer.write_all(&extra_field_len.to_le_bytes())?;

            for extra in self.extra_field.iter() {
                extra.write(writer)?;
            }
        }
        if let Some(file_name) = self.file_name.as_ref() {
            writer.write_all(file_name)?;
            if !file_name.ends_with(&[0]) {
                writer.write_all(&[0])?;
            }
        }
        if let Some(comment) = self.comment.as_ref() {
            writer.write_all(comment)?;
            if !comment.ends_with(&[0]) {
                writer.write_all(&[0])?;
            }
        }
        if let Some(crc16) = self.crc16 {
            writer.write_all(&crc16.to_le_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn load_header() -> Result<(), BGZFError> {
        let mut reader =
            io::BufReader::new(File::open("testfiles/common_all_20180418_half.vcf.gz")?);
        let header = BGZFHeader::from_reader(&mut reader)?;
        assert_eq!(header.operation_system, FILESYSTEM_UNKNOWN);
        assert_eq!(header.compression_method, 8);
        assert_eq!(header.flags, 4);
        assert_eq!(header.extra_field_len, Some(6));
        assert_eq!(header.extra_field[0].data.len(), 2);
        let mut buf: Vec<u8> = Vec::new();
        header.write(&mut buf)?;
        assert_eq!(buf.len(), header.header_size() as usize);
        Ok(())
    }

    #[test]
    fn load_header2() -> Result<(), BGZFError> {
        let mut reader = io::BufReader::new(File::open(
            "testfiles/common_all_20180418_half.vcf.nobgzip.gz",
        )?);
        let header = BGZFHeader::from_reader(&mut reader)?;
        assert_eq!(header.operation_system, FILESYSTEM_UNIX);
        assert_eq!(header.compression_method, 8);
        assert_eq!(header.flags, FLAG_FNAME);
        assert_eq!(header.extra_field_len, None);
        assert_eq!(
            header.file_name,
            Some(b"common_all_20180418_half.vcf.nobgzip\0".to_vec())
        );
        let mut buf: Vec<u8> = Vec::new();
        header.write(&mut buf)?;
        assert_eq!(buf.len(), header.header_size() as usize);
        Ok(())
    }
}
