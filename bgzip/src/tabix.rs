use crate::*;
use std::collections::HashMap;
use std::convert::TryInto;
use std::i32;
use std::io::{self, Read};

#[derive(Debug, Clone, PartialEq)]
pub struct TabixChunk {
    pub begin: u64,
    pub end: u64,
}

impl TabixChunk {
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> io::Result<Self> {
        let begin = reader.read_le_u64()?;
        let end = reader.read_le_u64()?;
        Ok(TabixChunk { begin, end })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabixBin {
    pub bin: u32,
    pub number_of_chunk: i32,
    pub chunks: Vec<TabixChunk>,
}

impl TabixBin {
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> io::Result<Self> {
        let bin = reader.read_le_u32()?;
        let number_of_chunk = reader.read_le_i32()?;
        let mut chunks = Vec::new();
        for _ in 0..number_of_chunk {
            chunks.push(TabixChunk::from_reader(reader)?);
        }

        Ok(TabixBin {
            bin,
            number_of_chunk,
            chunks,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabixSequence {
    pub number_of_distinct_bin: i32,
    pub bins: HashMap<u32, TabixBin>,
    pub number_of_intervals: i32,
    pub intervals: Vec<u64>,
}

impl TabixSequence {
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> io::Result<Self> {
        let number_of_distinct_bin = reader.read_le_i32()?;
        let mut bins = HashMap::new();
        for _ in 0..number_of_distinct_bin {
            let one_bin = TabixBin::from_reader(reader)?;
            bins.insert(one_bin.bin, one_bin);
        }

        let number_of_intervals = reader.read_le_i32()?;

        let mut intervals = Vec::new();
        for _ in 0..number_of_intervals {
            intervals.push(reader.read_le_u64()?);
        }
        Ok(TabixSequence {
            number_of_distinct_bin,
            bins,
            number_of_intervals,
            intervals,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tabix {
    pub number_of_references: i32,
    pub format: i32,
    pub column_for_sequence: i32,
    pub column_for_begin: i32,
    pub column_for_end: i32,
    pub meta: [u8; 4],
    pub skip: i32,
    pub length_of_concatenated_sequence_names: i32,
    pub names: Vec<Vec<u8>>,
    pub sequences: Vec<TabixSequence>,
}

impl Tabix {
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, crate::BGZFError> {
        let mut reader = io::BufReader::new(crate::read::BGZFReader::new(reader)?);

        let mut buf: [u8; 4] = [0, 0, 0, 0];
        reader.read_exact(&mut buf)?;
        if buf != [b'T', b'B', b'I', 1] {
            return Err(BGZFError::Other("Not Tabix format"));
        }
        let number_of_references = reader.read_le_i32()?;
        let format = reader.read_le_i32()?;
        let column_for_sequence = reader.read_le_i32()?;
        let column_for_begin = reader.read_le_i32()?;
        let column_for_end = reader.read_le_i32()?;
        reader.read_exact(&mut buf)?;
        let meta = buf;
        let skip = reader.read_le_i32()?;
        let length_of_concatenated_sequence_names = reader.read_le_i32()?;
        let mut name_bytes: Vec<u8> =
            vec![0; length_of_concatenated_sequence_names.try_into().unwrap()];
        reader.read_exact(&mut name_bytes)?;
        let names = split_names(&name_bytes);

        let mut sequences = Vec::new();
        for _ in 0..number_of_references {
            sequences.push(TabixSequence::from_reader(&mut reader)?);
        }

        Ok(Tabix {
            number_of_references,
            format,
            column_for_sequence,
            column_for_begin,
            column_for_end,
            meta,
            skip,
            length_of_concatenated_sequence_names,
            names,
            sequences,
        })
    }
}

fn split_names(data: &[u8]) -> Vec<Vec<u8>> {
    let mut reader = io::BufReader::new(data);
    let mut result = Vec::new();

    loop {
        let mut buf = Vec::new();
        let l = reader.read_until(0, &mut buf).unwrap();
        if l == 0 {
            break;
        }
        result.push(buf);
    }

    result
}

const MIN_SHIFT: u32 = 14;
const DEPTH: u32 = 5;

/// calculate the list of bins that may overlap with region [beg,end) (zero-based)
pub fn reg2bin(beg: u32, end: u32) -> u32 {
    crate::csi::reg2bin(beg.into(), end.into(), MIN_SHIFT, DEPTH)
}

/// calculate the list of bins that may overlap with region [beg,end) (zero-based)
pub fn reg2bins(beg: u32, end: u32) -> Vec<u32> {
    crate::csi::reg2bins(beg.into(), end.into(), MIN_SHIFT, DEPTH)
}

#[cfg(test)]
mod test {
    use anyhow::anyhow;

    use super::*;
    use std::fs::File;
    use std::str;

    #[test]
    fn test_tabix_read() -> anyhow::Result<()> {
        let mut reader = File::open("testfiles/common_all_20180418_half.vcf.gz.tbi")?;
        let tabix = Tabix::from_reader(&mut reader)?;
        //println!("{:?}", tabix);

        let mut chunks_writer = csv::Writer::from_path("tmp/sequence.csv")?;
        chunks_writer.write_record(&[
            "sequence name",
            "bin index",
            "bin number",
            "chunk index",
            "chunk begin",
            "chunk end",
        ])?;

        for (i, one_seq) in tabix.sequences.iter().enumerate() {
            for (j, (_, one_bin)) in one_seq.bins.iter().enumerate() {
                for (k, one_chunk) in one_bin.chunks.iter().enumerate() {
                    chunks_writer.write_record(&[
                        str::from_utf8(&tabix.names[i]).unwrap().to_string(),
                        format!("{}", j),
                        format!("{}", one_bin.bin),
                        format!("{}", k),
                        format!("{}", one_chunk.begin),
                        format!("{}", one_chunk.end),
                    ])?;
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_bins() -> anyhow::Result<()> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .quoting(false)
            .from_reader(flate2::read::MultiGzDecoder::new(File::open(
                "testfiles/bins.tsv.gz",
            )?));
        for row in reader.records() {
            let row = row?;
            let start: u64 = row.get(1).ok_or_else(|| anyhow!("No Start"))?.parse()?;
            let end: u64 = row.get(2).ok_or_else(|| anyhow!("No End"))?.parse()?;
            let bin: u32 = row.get(3).ok_or_else(|| anyhow!("No Bin"))?.parse()?;
            let mut bins: Vec<u32> = row
                .get(4)
                .ok_or_else(|| anyhow!("No Bins"))?
                .split(',')
                .map(|x| x.parse().expect("Invalid bin"))
                .collect();

            let calculated_bin = reg2bin(
                start.try_into().expect("Too large start"),
                end.try_into().expect("Too large end"),
            );
            let mut calculated_bins = reg2bins(
                start.try_into().expect("Too large start"),
                end.try_into().expect("Too large end"),
            );
            bins.sort();
            calculated_bins.sort();

            assert_eq!(
                bin, calculated_bin,
                "Start: {} / End: {} / Calculated bin: {} / Expected bin: {}",
                start, end, calculated_bin, bin,
            );
            assert_eq!(
                bins, calculated_bins,
                "Start: {} / End: {} / Calculated bins: {:?} / Expected bins: {:?}",
                start, end, calculated_bins, bins,
            );
        }
        Ok(())
    }
}
