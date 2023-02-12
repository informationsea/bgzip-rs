use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::deflate::*;
use crate::BGZFError;

const EOF_BLOCK: [u8; 10] = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const DEFAULT_PROCESS_BLOCK_NUM: usize = 50;

struct ReadBlock {
    index: u64,
    decompressed_data: Vec<u8>,
    compressed_data: Vec<Vec<u8>>,
    decompress: Decompress,
}

impl ReadBlock {
    pub fn new(process_block_num: usize) -> Self {
        let decompress = Decompress::new();

        ReadBlock {
            index: 0,
            decompressed_data: Vec::with_capacity(crate::write::MAXIMUM_COMPRESS_UNIT_SIZE),
            compressed_data: vec![
                Vec::with_capacity(crate::write::MAXIMUM_COMPRESS_UNIT_SIZE);
                process_block_num
            ],
            decompress,
        }
    }
}

/// A Multi-thread BGZF writer.
///
/// [rayon](https://crates.io/crates/rayon) is used to run decompression in a thread pool.
pub struct BGZFMultiThreadReader<R: Read> {
    reader: R,
    block_list: Vec<ReadBlock>,
    current_read_pos: usize,
    current_read_buffer: Option<ReadBlock>,
    read_waiting_blocks: HashMap<u64, ReadBlock>,
    reader_receiver: Receiver<Result<ReadBlock, BGZFError>>,
    reader_sender: Sender<Result<ReadBlock, BGZFError>>,
    next_read_index: u64,
    next_decompress_index: u64,
    eof_read_index: u64,
}

impl<R: Read> BGZFMultiThreadReader<R> {
    /// Create new [`BGZFMultiThreadReader`] from `reader`
    pub fn new(reader: R) -> Result<Self, BGZFError> {
        Self::with_process_block_num(reader, DEFAULT_PROCESS_BLOCK_NUM)
    }

    /// Create new [`BGZFMultiThreadReader`] from `reader` and `process_block_num`.
    ///
    /// `process_block_num` is the number blocks to dispatch a new thread.
    /// Default value is 50. If you have fast CPU, larger value can be improve efficiency.
    pub fn with_process_block_num(reader: R, process_block_num: usize) -> Result<Self, BGZFError> {
        let (tx, rx) = channel();
        let mut reader = BGZFMultiThreadReader {
            reader,
            block_list: (0..(rayon::current_num_threads() * 2))
                .map(|_| ReadBlock::new(process_block_num))
                .collect(),
            current_read_pos: 0,
            current_read_buffer: None,
            read_waiting_blocks: HashMap::new(),
            reader_receiver: rx,
            reader_sender: tx,
            next_read_index: 0,
            next_decompress_index: 0,
            eof_read_index: u64::MAX,
        };
        reader.dispatch_read_thread()?;

        Ok(reader)
    }

    fn dispatch_read_thread(&mut self) -> Result<(), BGZFError> {
        while !self.block_list.is_empty() && self.next_decompress_index < self.eof_read_index {
            let mut block = self.block_list.pop().unwrap();
            block.index = self.next_decompress_index;
            self.next_decompress_index += 1;

            let mut last_index = 0;

            for i in 0..block.compressed_data.len() {
                //eprintln!("load block {}", i);
                super::load_block(
                    &mut self.reader,
                    &mut block.compressed_data.get_mut(i).unwrap(),
                )
                .map_err(|e| -> std::io::Error {
                    // eprintln!("load block error: {}", e);
                    e.into()
                })?;
                last_index = i;
                if block.compressed_data.get(i).unwrap() == &EOF_BLOCK {
                    //self.block_list.clear();
                    // eprintln!("EOF reach: {}", block.index);
                    self.eof_read_index = self.next_decompress_index;
                    break;
                }
            }

            if last_index != block.compressed_data.len() - 1 {
                block
                    .compressed_data
                    .drain(last_index..block.compressed_data.len());
            }

            let sender = self.reader_sender.clone();
            // eprintln!("spawn: {}", block.index);
            rayon::spawn(move || {
                let _i = block.index;
                block.decompressed_data.clear();
                for one_compress_data in &block.compressed_data {
                    match super::decompress_block(
                        &mut block.decompressed_data,
                        &one_compress_data,
                        &mut block.decompress,
                    ) {
                        Ok(_) => (),
                        Err(e) => {
                            //eprintln!("send Error: {}", e);
                            sender.send(Err(e)).expect("reader send error 2")
                        }
                    }
                }
                sender.send(Ok(block)).expect("reader send error 1");
                // eprintln!("done: {}", i);
            });
        }

        Ok(())
    }
}

impl<R: Read> BufRead for BGZFMultiThreadReader<R> {
    fn consume(&mut self, amt: usize) {
        self.current_read_pos += amt;
    }
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        // eprintln!(
        //     "fill buf start: {} {} {} {}",
        //     self.current_read_pos,
        //     self.next_read_index,
        //     self.current_read_buffer
        //         .as_ref()
        //         .map(|x| x.index)
        //         .unwrap_or(10000000000),
        //     self.eof_read_index
        // );

        //eprintln!("fill buf 1");

        if let Some(b) = self.current_read_buffer.as_ref() {
            if b.decompressed_data.len() <= self.current_read_pos {
                std::mem::drop(b);
                self.block_list
                    .push(self.current_read_buffer.take().unwrap());
            }
        }

        //eprintln!("fill buf 2");

        if self.next_read_index > self.eof_read_index {
            //eprintln!("EOF 0 bytes fill");
            return Ok(&[]);
        }

        //eprintln!("fill buf 3");
        self.dispatch_read_thread()
            .map_err(|e| Into::<std::io::Error>::into(e))?;

        //eprintln!("fill buf 4");

        if self.current_read_buffer.is_none() {
            if self.next_read_index >= self.eof_read_index {
                //eprintln!("EOF 0 bytes fill");
                return Ok(&[]);
            }

            while !self.read_waiting_blocks.contains_key(&self.next_read_index) {
                let block = self
                    .reader_receiver
                    .recv()
                    .expect("reader receive error")
                    .map_err(|e| -> std::io::Error { e.into() })?;
                // eprintln!("fetch: {}", block.index);
                self.read_waiting_blocks.insert(block.index, block);
            }
            self.current_read_buffer = self.read_waiting_blocks.remove(&self.next_read_index);
            // eprintln!("read: {}", self.next_read_index);
            self.current_read_pos = 0;
            self.next_read_index += 1;
        }

        // eprintln!(
        //     "fill buf end {} {}/{}",
        //     self.current_read_buffer.as_ref().unwrap().index,
        //     self.current_read_pos,
        //     self.current_read_buffer
        //         .as_ref()
        //         .unwrap()
        //         .decompressed_data
        //         .len()
        // );
        Ok(&self.current_read_buffer.as_ref().unwrap().decompressed_data[self.current_read_pos..])
    }
}

impl<R: Read> Read for BGZFMultiThreadReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        //eprintln!("read start: {}", buf.len());
        let internal_buf = self.fill_buf()?;
        let bytes_to_copy = buf.len().min(internal_buf.len());
        buf[0..bytes_to_copy].copy_from_slice(&internal_buf[0..bytes_to_copy]);
        self.consume(bytes_to_copy);
        //eprintln!("read end: {}", bytes_to_copy);
        Ok(bytes_to_copy)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_thread_read() -> anyhow::Result<()> {
        let mut expected_reader = flate2::read::MultiGzDecoder::new(std::fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?);
        let mut expected_buf = Vec::new();
        expected_reader.read_to_end(&mut expected_buf)?;

        // normal read
        let mut reader = BGZFMultiThreadReader::new(std::fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?)?;

        let mut read_buf = Vec::new();
        reader.read_to_end(&mut read_buf)?;
        assert_eq!(expected_buf.len(), read_buf.len());
        assert_eq!(expected_buf, read_buf);

        // with single block
        let mut reader = BGZFMultiThreadReader::with_process_block_num(
            std::io::BufReader::new(std::fs::File::open(
                "testfiles/common_all_20180418_half.vcf.gz",
            )?),
            1,
        )?;

        let mut read_buf = Vec::new();
        reader.read_to_end(&mut read_buf)?;
        assert_eq!(expected_buf.len(), read_buf.len());
        assert_eq!(expected_buf, read_buf);

        // read 100 bytes per loop
        let mut reader = BGZFMultiThreadReader::new(std::fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?)?;

        let mut read_buf = Vec::new();
        loop {
            let mut small_buf = [0; 100];
            let read_bytes = reader.read(&mut small_buf)?;
            if read_bytes == 0 {
                break;
            }
            read_buf.extend_from_slice(&small_buf[..read_bytes]);
        }

        assert_eq!(expected_buf.len(), read_buf.len());

        Ok(())
    }
}
