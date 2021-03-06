use crate::*;
use std::collections::HashMap;
use std::convert::TryInto;
use std::i32;
use std::io::{self, BufRead, Read, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct TabixChunk {
    pub begin: u64,
    pub end: u64,
}

impl TabixChunk {
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> Result<Self> {
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
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> Result<Self> {
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
    fn from_reader<R: Read + BinaryReader>(reader: &mut R) -> Result<Self> {
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
    pub fn from_reader(reader: &mut impl Read) -> Result<Self> {
        let mut reader = io::BufReader::new(flate2::read::MultiGzDecoder::new(reader));

        let mut buf: [u8; 4] = [0, 0, 0, 0];
        reader.read_exact(&mut buf)?;
        if buf != [b'T', b'B', b'I', 1] {
            return Err(io::Error::new(io::ErrorKind::Other, "Not Tabix format"));
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

/* calculate bin given an alignment covering [beg,end) (zero-based, half-close-half-open) */
pub fn reg2bin(beg: i64, end: i64, min_shift: i32, depth: i32) -> i32 {
    let end = end - 1;
    let mut l = depth;
    let mut s = min_shift;
    let mut t = ((1 << (depth * 3)) - 1) / 7;

    while l > 0 {
        if beg >> s == end >> s {
            return t + (beg >> s) as i32;
        };
        l -= 1;
        s += 3;
        t -= 1 << (l * 3);
    }
    0
}
// /* calculate the list of bins that may overlap with region [beg,end) (zero-based) */
// int reg2bins(int64_t beg, int64_t end, int min_shift, int depth, int *bins)
// {
// int l, t, n, s = min_shift + depth*3;
// for (--end, l = n = t = 0; l <= depth; s -= 3, t += 1<<l*3, ++l) {
// int b = t + (beg>>s), e = t + (end>>s), i;
// for (i = b; i <= e; ++i) bins[n++] = i;
// }
// return n;
//}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use std::str;

    #[test]
    fn test_tabix_read() -> Result<()> {
        let mut reader = File::open("testfiles/common_all_20180418_half.vcf.gz.tbi")?;
        let tabix = Tabix::from_reader(&mut reader)?;
        //println!("{:?}", tabix);

        let mut chunks_writer = csv::Writer::from_path("target/sequence.csv")?;
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
}
