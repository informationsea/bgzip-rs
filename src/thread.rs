use flate2::{Compress, Compression};
use std::collections::HashMap;
use std::io::{Error, ErrorKind, Result, Write};
use std::sync::mpsc::{channel, Receiver, Sender};

struct WriteBlock {
    index: u64,
    compress: Compress,
    compressed_buffer: Vec<u8>,
    temporary_buffer: Vec<u8>,
    raw_buffer: Vec<u8>,
}

impl WriteBlock {
    fn new(level: Compression, block_size: usize) -> Self {
        WriteBlock {
            index: 0,
            compress: Compress::new(level, false),
            compressed_buffer: Vec::with_capacity(
                block_size + crate::write::EXTRA_COMPRESS_BUFFER_SIZE,
            ),
            temporary_buffer: Vec::with_capacity(
                block_size + crate::write::EXTRA_COMPRESS_BUFFER_SIZE,
            ),
            raw_buffer: Vec::with_capacity(block_size),
        }
    }

    fn reset(&mut self) {
        self.index = 0;
        self.compress.reset();
        self.compressed_buffer.clear();
        self.temporary_buffer.clear();
        self.raw_buffer.clear();
    }
}

/// A Multi-thread BGZF writer
pub struct BGZFMultiThreadWriter<W: Write> {
    writer: W,
    compress_unit_size: usize,
    block_list: Vec<WriteBlock>,
    write_waiting_blocks: HashMap<u64, WriteBlock>,
    writer_receiver: Receiver<WriteBlock>,
    writer_sender: Sender<WriteBlock>,
    next_write_index: u64,
    next_compress_index: u64,
}

impl<W: Write> BGZFMultiThreadWriter<W> {
    pub fn new(writer: W, level: Compression) -> Result<Self> {
        Self::with_compress_unit_size(writer, crate::write::DEFAULT_COMPRESS_UNIT_SIZE, level)
    }

    /// Create new
    pub fn with_compress_unit_size(
        writer: W,
        compress_unit_size: usize,
        level: Compression,
    ) -> Result<Self> {
        if compress_unit_size >= crate::write::MAXIMUM_COMPRESS_UNIT_SIZE {
            return Err(Error::new(
                ErrorKind::Other,
                "Too large compress block size",
            ));
        }

        let (tx, rx) = channel();

        Ok(BGZFMultiThreadWriter {
            writer,
            compress_unit_size,
            block_list: (0..(rayon::current_num_threads()))
                .map(|_| WriteBlock::new(level, compress_unit_size))
                .collect(),
            write_waiting_blocks: HashMap::new(),
            writer_receiver: rx,
            writer_sender: tx,
            next_write_index: 0,
            next_compress_index: 0,
        })
    }

    fn process_buffer(&mut self, block: bool, block_all: bool) -> Result<()> {
        let mut current_block = block;
        while self.next_compress_index != self.next_write_index {
            let mut next_data = if current_block {
                self.writer_receiver
                    .recv()
                    .map_err(|_| Error::new(ErrorKind::Other, "Closed channel"))?
            } else {
                match self.writer_receiver.try_recv() {
                    Ok(d) => d,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err(Error::new(ErrorKind::Other, "Closed channel"))
                    }
                }
            };
            // eprintln!(
            //     "fetch thread data: {} / {} / {}",
            //     next_data.index, self.next_write_index, self.next_compress_index
            // );
            if next_data.index == self.next_write_index {
                self.writer.write_all(&next_data.compressed_buffer)?;
                //eprintln!("write block 1: {}", next_data.index);
                self.next_write_index += 1;
                next_data.reset();
                self.block_list.push(next_data);

                while self
                    .write_waiting_blocks
                    .contains_key(&self.next_write_index)
                {
                    let mut next_data = self
                        .write_waiting_blocks
                        .remove(&self.next_write_index)
                        .unwrap();
                    //eprintln!("write block 2: {}", next_data.index);
                    self.writer.write_all(&next_data.compressed_buffer)?;
                    self.next_write_index += 1;
                    next_data.reset();
                    self.block_list.push(next_data);
                }
                current_block = block_all;
            } else {
                //eprintln!("Insert into waiting blocks: {}", next_data.index);
                self.write_waiting_blocks.insert(next_data.index, next_data);
            }
        }

        Ok(())
    }

    fn write_current_block(&mut self) {
        let mut block = self.block_list.remove(0);
        block.index = self.next_compress_index;
        self.next_compress_index += 1;
        let sender = self.writer_sender.clone();
        //eprintln!("spawn thread: {}", block.index);
        rayon::spawn(move || {
            //eprintln!("started thread: {}", block.index);
            crate::write::write_block(
                &mut block.compressed_buffer,
                &block.raw_buffer,
                &mut block.temporary_buffer,
                &mut block.compress,
            )
            .expect("Failed to write block");
            //eprintln!("finished thread: {}", block.index);
            sender.send(block).expect("failed to send write result");
        });
    }

    pub fn close(mut self) -> Result<()> {
        self.flush()?;
        self.writer.write_all(crate::write::FOOTER_BYTES)?;
        Ok(())
    }
}

impl<W: Write> Write for BGZFMultiThreadWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut wrote_bytes = 0;
        while wrote_bytes < buf.len() {
            self.process_buffer(self.block_list.is_empty(), false)?;
            let current_buffer = self.block_list.get_mut(0).unwrap();
            let remain_buffer = self.compress_unit_size - current_buffer.raw_buffer.len();
            let bytes_to_write = remain_buffer.min(buf.len() - wrote_bytes);
            current_buffer
                .raw_buffer
                .extend_from_slice(&buf[wrote_bytes..(wrote_bytes + bytes_to_write)]);
            if bytes_to_write == remain_buffer {
                self.write_current_block();
            }
            wrote_bytes += bytes_to_write;
        }

        Ok(wrote_bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.process_buffer(self.block_list.is_empty(), false)?;
        if self.block_list[0].raw_buffer.len() > 0 {
            self.write_current_block();
        }
        self.process_buffer(true, true)?;
        // eprintln!(
        //     "flush: {}/{}/{}/{}",
        //     self.next_compress_index,
        //     self.next_write_index,
        //     self.block_list.len(),
        //     rayon::current_num_threads()
        // );
        Ok(())
    }
}

impl<W: Write> Drop for BGZFMultiThreadWriter<W> {
    fn drop(&mut self) {
        self.flush().expect("BGZF: Flash Error");
        self.writer
            .write_all(crate::write::FOOTER_BYTES)
            .expect("BGZF: Cannot write EOF marker");
    }
}

#[cfg(test)]
mod test {
    use std::io::Read;

    use super::*;
    use rand::prelude::*;

    const WRITE_UNIT: usize = 2000;
    const BUF_SIZE: usize = 1000 * 1000 * 10;

    #[test]
    fn test_thread_writer() -> anyhow::Result<()> {
        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(0x9387402456157523);
        let path = "./target/test_thread_writer.data.gz";
        let write_file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut writer =
            BGZFMultiThreadWriter::with_compress_unit_size(write_file, 1024, Compression::best())?;

        let mut data = vec![0; BUF_SIZE];

        rand.fill_bytes(&mut data);

        let mut wrote_bytes = 0;
        loop {
            let to_write_bytes = WRITE_UNIT.min(data.len() - wrote_bytes);
            if to_write_bytes == 0 {
                break;
            }
            wrote_bytes += writer.write(&mut data[wrote_bytes..(wrote_bytes + to_write_bytes)])?;
        }
        //eprintln!("wrote_bytes: {}/{}", i, wrote_bytes);

        std::mem::drop(writer);

        let mut rand = rand_pcg::Pcg64Mcg::seed_from_u64(0x9387402456157523);
        let mut reader = crate::BGZFReader::new(std::fs::File::open(path)?);
        let mut read_data = vec![];

        rand.fill_bytes(&mut data);
        reader.read_to_end(&mut read_data)?;
        assert_eq!(read_data.len(), data.len());
        assert!(read_data == data, "unmatched");

        //writer.flush()?;

        Ok(())
    }
}
