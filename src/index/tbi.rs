use super::{Index, IndexedFile};
use flate2::read::MultiGzDecoder;
use read::BGzReader;
use std::cmp::max;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::str;
use *;

#[derive(Debug)]
pub struct TabixEntry {
    pub data: Vec<u8>,
    pub begin: u64,
    pub end: u64,
}

#[derive(Debug)]
pub struct TabixFile<R: io::Read + io::Seek> {
    pub reader: BGzReader<R>,
    pub tabix: TabixIndex,

    max_column_pos: usize,
    target_rid: u32,
    target_begin: u64,
    target_end: u64,
    chunks: Vec<(u64, u64)>,
    current_chunk: usize,
    first_scan: bool,
}

impl<R: io::Read + io::Seek> IndexedFile for TabixFile<R> {
    // 0-based half-close, half-open
    fn fetch0(&mut self, rid: u32, begin: u64, end: u64) -> io::Result<()> {
        self.target_rid = rid;
        self.target_begin = begin;
        self.target_end = end;

        self.chunks = self.tabix.region_chunks(rid, begin, end);
        self.current_chunk = 0;
        self.first_scan = true;
        Ok(())
    }

    fn read(&mut self, mut data: &mut Vec<u8>) -> io::Result<Option<(u64, u64)>> {
        //println!("one read");

        while self.current_chunk < self.chunks.len() {
            let chunk = &self.chunks[self.current_chunk];
            //println!(
            //    "current_chunk:{}  {}-{}",
            //    self.current_chunk,
            //    chunk.0,
            //    chunk.1
            //);
            if self.first_scan {
                self.reader.seek_virtual_file_offset(chunk.0)?;
                self.first_scan = false;
            //println!("first scan");
            } else {
                //println!("Continue scan {}", self.reader.tell_virtual_file_offset());
            }

            loop {
                let current_virtual_offset = self.reader.tell_virtual_file_offset();
                if current_virtual_offset >= chunk.1 {
                    //println!("end of chunk {}", current_virtual_offset);
                    break;
                }

                data.clear();
                self.reader.read_until(b'\n', &mut data)?;
                if data[0] == self.tabix.meta as u8 {
                    // skip meta line
                    continue;
                }

                let elements: Vec<Vec<u8>> = data
                    .split(|x| *x == b'\t')
                    .take(self.max_column_pos + 1)
                    .map(|x| x.into_iter().map(|y| *y).collect())
                    .collect();
                // let seq_text = &elements[self.tabix.col_seq as usize - 1]; // do not check seq id
                let start_text = &elements[self.tabix.col_beg as usize - 1];
                let start_pos =
                    convert_data_to_u64(start_text)? - if self.tabix.zero_based { 1 } else { 0 };

                let end_text = &elements[self.tabix.col_end as usize - 1];
                let end_pos = if self.tabix.vcf_mode {
                    start_pos + end_text.len() as u64
                } else if self.tabix.sam_mode {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "SAM mode is not implemented yet",
                    ));
                } else {
                    convert_data_to_u64(end_text)?
                };

                /*
                        println!(
                            "scanning {} {} {}",
                            current_virtual_offset, start_pos, end_pos
                        );
                        */
                /*
                let this_bin = super::reg2bin(
                    start_pos,
                    end_pos,
                    super::DEFAULT_MIN_SHIFT,
                    super::DEFAULT_DEPTH,
                );
                */

                if start_pos <= self.target_end && self.target_begin <= end_pos {
                    //println!("[{}] data {}", this_bin, str::from_utf8(data).unwrap());
                    return Ok(Some((start_pos, end_pos)));
                }
            }

            self.first_scan = true;
            self.current_chunk += 1;
            if self.chunks.len() <= self.current_chunk {
                //println!("no more chunk");
                break;
            }
        }

        Ok(None)
    }
}

impl<R: io::Read + io::Seek> TabixFile<R> {
    pub fn new<U: io::Read>(reader: R, index_reader: U) -> io::Result<TabixFile<R>> {
        let mut bgz_reader = BGzReader::new(reader)?;
        let index = TabixIndex::new(index_reader)?;

        bgz_reader.seek_virtual_file_offset(index.seq_index[0].interval[0])?;

        Ok(TabixFile {
            reader: bgz_reader,
            max_column_pos: max(index.col_beg, max(index.col_end, index.col_seq)) as usize,
            tabix: index,
            target_rid: 0,
            target_begin: 0,
            target_end: 0,
            chunks: Vec::new(),
            current_chunk: 0,
            first_scan: true,
        })
    }
}

impl TabixFile<io::BufReader<fs::File>> {
    pub fn with_filename(filename: &str) -> io::Result<TabixFile<io::BufReader<fs::File>>> {
        let tabix_name = format!("{}.tbi", filename);
        let reader = io::BufReader::new(fs::File::open(filename)?);
        let index_reader = io::BufReader::new(fs::File::open(tabix_name)?);
        TabixFile::new(reader, index_reader)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TabixIndex {
    pub n_ref: u32,
    pub format: u32,
    pub col_seq: u32,
    pub col_beg: u32,
    pub col_end: u32,
    pub meta: u32,
    pub skip: u32,
    pub l_nm: u32,
    pub names: Vec<Vec<u8>>,
    pub name_to_index: BTreeMap<Vec<u8>, u32>,
    pub seq_index: Vec<SequenceIndex>,

    zero_based: bool,
    sam_mode: bool,
    vcf_mode: bool,
}

impl super::Index for TabixIndex {
    fn region_chunks(&self, rid: u32, begin: u64, end: u64) -> Vec<(u64, u64)> {
        let mut bins = Vec::new();
        super::reg2bins(
            begin,
            end,
            super::DEFAULT_MIN_SHIFT,
            super::DEFAULT_DEPTH,
            &mut bins,
        );

        let mut simplfy = super::RegionSimplify::new();
        for one_bin in bins {
            if let Some(bin_chunks) = self.seq_index[rid as usize].bins.get(&one_bin.into()) {
                for one_chunk in &bin_chunks.chunks {
                    simplfy.insert(one_chunk.chunk_beg, one_chunk.chunk_end);
                }
            }
        }
        simplfy.regions()
    }

    fn rid2name(&self, rid: u32) -> &[u8] {
        &self.names[rid as usize]
    }

    fn name2rid(&self, name: &[u8]) -> u32 {
        self.name_to_index[name]
    }

    fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SequenceIndex {
    pub n_bin: u32,
    pub bins: BTreeMap<u32, BinIndex>,
    pub n_intv: u32,
    pub interval: Vec<u64>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BinIndex {
    pub bin: u32,
    pub n_chunk: u32,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Chunk {
    pub chunk_beg: u64,
    pub chunk_end: u64,
}

impl TabixIndex {
    pub fn new<R: io::Read>(reader: R) -> io::Result<TabixIndex> {
        let mut reader = MultiGzDecoder::new(reader);
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"TBI\x01" {
            return Err(io::Error::new(io::ErrorKind::Other, "not tabix file"));
        }
        let mut name_to_index = BTreeMap::new();

        let n_ref = read_le_u32(&mut reader)?;
        let format = read_le_u32(&mut reader)?;
        let col_seq = read_le_u32(&mut reader)?;
        let col_beg = read_le_u32(&mut reader)?;
        let mut col_end = read_le_u32(&mut reader)?;
        let meta = read_le_u32(&mut reader)?;
        let skip = read_le_u32(&mut reader)?;
        let l_nm = read_le_u32(&mut reader)?;

        // load names
        let mut name_data = sized_vec(0u8, l_nm as usize);
        reader.read_exact(&mut name_data)?;
        let mut names = Vec::new();
        {
            let mut temp = Vec::new();
            for i in 0..l_nm {
                if name_data[i as usize] == 0 {
                    names.push(temp.clone());
                    //println!("chr {}", str::from_utf8(&temp[..]).unwrap());
                    temp.clear();
                } else {
                    temp.push(name_data[i as usize]);
                }
            }
            //println!("remain {:?}", temp);
        }

        let mut seq_index = Vec::new();
        for i in 0..n_ref {
            name_to_index.insert(names[i as usize].clone(), i as u32);
            let n_bin = read_le_u32(&mut reader)?;

            let mut bin_index = BTreeMap::new();
            for _ in 0..n_bin {
                let bin = read_le_u32(&mut reader)?;
                let n_chunk = read_le_u32(&mut reader)?;
                let mut chunks = Vec::new();

                for _ in 0..n_chunk {
                    let cnk_beg = read_le_u64(&mut reader)?;
                    let cnk_end = read_le_u64(&mut reader)?;
                    chunks.push(Chunk {
                        chunk_beg: cnk_beg,
                        chunk_end: cnk_end,
                    });
                }
                bin_index.insert(
                    bin,
                    BinIndex {
                        bin,
                        n_chunk,
                        chunks,
                    },
                );
            }

            let n_intv = read_le_u32(&mut reader)?;
            let mut interval = Vec::new();
            for _ in 0..n_intv {
                let ioff = read_le_u64(&mut reader)?;
                interval.push(ioff);
            }
            seq_index.push(SequenceIndex {
                n_bin,
                bins: bin_index,
                n_intv,
                interval,
            });
        }

        let zero_based = format & 0x10000 > 0;
        let vcf_mode = format == 2;
        let sam_mode = format == 1;

        if vcf_mode {
            col_end = 5;
        }

        Ok(TabixIndex {
            n_ref,
            format,
            col_seq,
            col_beg,
            col_end,
            meta,
            skip,
            l_nm,
            names,
            seq_index,
            name_to_index,
            zero_based,
            vcf_mode,
            sam_mode,
        })
    }
}

fn convert_data_to_u64(data: &[u8]) -> io::Result<u64> {
    str::from_utf8(data)
        .map_err(|x| io::Error::new(io::ErrorKind::Other, x))?
        .parse::<u64>()
        .map_err(|x| io::Error::new(io::ErrorKind::Other, x))
}

#[cfg(test)]
mod test {
    use index::IndexedFile;
    use std::fs;
    use std::io;
    use std::str;

    #[test]
    fn test_load() -> io::Result<()> {
        let mut reader = fs::File::open("./testfiles/common_all_20180418_half.vcf.gz.tbi")?;
        let _tabix = super::TabixIndex::new(&mut reader)?;
        //println!("{:?}", _tabix);

        Ok(())
    }

    #[test]
    fn test_fetch() {
        let mut reader = super::TabixFile::with_filename(
            "./testfiles/gencode.v28.annotation.sorted.subset.gff3.gz",
        ).unwrap();
        reader.fetch0(0, 42990000, 42990600).unwrap();
        let actual_data = reader.read_all().unwrap();

        let expected_data: Vec<Vec<u8>> = include_bytes!(
            "../../testfiles/gencode.v28.annotation.sorted.subset.chr17-42990000-42990600.gff3"
        ).split(|x| *x == b'\n')
        .map(|x| x.to_vec())
        .filter(|x| x.len() > 0)
        .map(|mut x| {
            x.push(b'\n');
            x
        }).collect();

        for x in &actual_data {
            println!("data: {}", str::from_utf8(&x.2).unwrap());
        }

        assert_eq!(actual_data.len(), expected_data.len());

        let mut i = 0;
        for (x, y) in actual_data.into_iter().zip(expected_data) {
            assert_eq!(
                x.2,
                y,
                "{} : {} / {}",
                i,
                str::from_utf8(&x.2).unwrap(),
                str::from_utf8(&y).unwrap()
            );
            i += 1;
        }
    }
}
