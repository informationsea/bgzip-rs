//! .gzi index support

use std::convert::TryInto;

use crate::{BGZFError, BinaryReader};

/// Represents .gzi index file
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BGZFIndex {
    pub(crate) entries: Vec<BGZFIndexEntry>,
}

impl BGZFIndex {
    pub(crate) fn new() -> Self {
        BGZFIndex::default()
    }

    /// List of index entries
    pub fn entries(&self) -> &[BGZFIndexEntry] {
        &self.entries
    }

    /// Load .gzi index file from `reader`
    pub fn from_reader<R: std::io::Read>(mut reader: R) -> std::io::Result<Self> {
        let num_entries = reader.read_le_u64()?;
        let mut result = BGZFIndex::default();
        for _ in 0..num_entries {
            let compressed_offset = reader.read_le_u64()?;
            let uncompressed_offset = reader.read_le_u64()?;
            result.entries.push(BGZFIndexEntry {
                compressed_offset,
                uncompressed_offset,
            })
        }
        Ok(result)
    }

    /// Write .gzi index file into `writer`
    pub fn write<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()> {
        let entries: u64 = self.entries.len().try_into().unwrap();
        writer.write_all(&entries.to_le_bytes())?;
        for one in &self.entries {
            writer.write_all(&one.compressed_offset.to_le_bytes())?;
            writer.write_all(&one.uncompressed_offset.to_le_bytes())?;
        }
        Ok(())
    }

    /// Convert uncompressed position to bgzf virtual position
    pub fn uncompressed_pos_to_bgzf_pos(&self, pos: u64) -> Result<u64, BGZFError> {
        let i = self
            .entries
            .partition_point(|x| x.uncompressed_offset <= pos);
        let entry = match i {
            0 => BGZFIndexEntry {
                compressed_offset: 0,
                uncompressed_offset: 0,
            },
            i => self.entries[i - 1].clone(),
        };
        // eprintln!(
        //     "[{}/{}] {} / {} ",
        //     i,
        //     self.entries().len(),
        //     pos,
        //     entry.uncompressed_offset
        // );
        Ok((entry.compressed_offset << 16) + ((pos - entry.uncompressed_offset) & ((1 << 16) - 1)))
    }

    /// Convert bgzf virtual position to uncompressed position
    pub fn bgzf_pos_to_uncompressed_pos(&self, bgzf_pos: u64) -> Result<u64, BGZFError> {
        let compressed_pos = bgzf_pos >> 16;
        if compressed_pos == 0 {
            return Ok(bgzf_pos);
        }
        let i = self
            .entries
            .binary_search_by(|x| x.compressed_offset.cmp(&compressed_pos))
            .map_err(|_| BGZFError::Other("Invalid BGZF position"))?;
        Ok(self.entries[i].uncompressed_offset + (bgzf_pos & ((1 << 16) - 1)))
    }
}

/// One entry of .gzi
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BGZFIndexEntry {
    pub compressed_offset: u64,
    pub uncompressed_offset: u64,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{BGZFWriter, Compression};
    use std::fs;
    use std::io::prelude::*;

    #[test]
    fn test_index_read_write() -> anyhow::Result<()> {
        let data = fs::read("testfiles/generated.bed.gz.gzi")?;
        let index = BGZFIndex::from_reader(&data[..])?;
        assert_eq!(index.entries.len(), 295);
        let mut generated_data = Vec::new();
        index.write(&mut generated_data)?;
        assert_eq!(data, generated_data);

        Ok(())
    }

    #[test]
    fn test_index_position_convert() -> anyhow::Result<()> {
        let mut data_reader = std::io::BufReader::new(flate2::read::MultiGzDecoder::new(
            fs::File::open("testfiles/generated.bed.gz")?,
        ));
        let mut line = String::new();
        let mut line_list = Vec::new();
        let mut writer = BGZFWriter::new(
            fs::File::create("tmp/test_index_position_convert.bed.gz")?,
            Compression::default(),
        );

        loop {
            let bgzf_pos = writer.bgzf_pos();
            let uncompressed_pos = writer.pos();
            line.clear();
            let size = data_reader.read_line(&mut line)?;
            if size == 0 {
                break;
            }
            writer.write_all(&line.as_bytes())?;
            line_list.push((bgzf_pos, uncompressed_pos, line.clone()));
        }
        let index = writer.close()?.unwrap();

        for (bgzf_pos, uncompressed_pos, _) in &line_list {
            assert_eq!(
                index.bgzf_pos_to_uncompressed_pos(*bgzf_pos)?,
                *uncompressed_pos
            );
            assert_eq!(
                index.uncompressed_pos_to_bgzf_pos(*uncompressed_pos)?,
                *bgzf_pos
            );
        }

        Ok(())
    }
}
