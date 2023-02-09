use clap::Parser;
use std::fs::File;
use std::io::{stdout, BufReader, Read, Seek, SeekFrom, Write};

#[derive(Debug, Parser)]
struct Args {
    #[command()]
    file: String,
    #[arg(short, long)]
    output: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let parser = Args::parse();

    let mut file = BufReader::new(File::open(&parser.file)?);
    let out: Box<dyn Write> = if let Some(out) = parser.output {
        Box::new(File::create(out)?)
    } else {
        Box::new(stdout().lock())
    };
    let mut csv_out = csv::WriterBuilder::new().from_writer(out);

    csv_out.write_record(&[
        "offset",
        "header-size",
        "compressed-size",
        "decompressed-size",
    ])?;

    loop {
        let offset = file.seek(SeekFrom::Current(0))?;
        let header = bgzip::header::BGZFHeader::from_reader(&mut file)?;
        let compressed_size = header.block_size()?;
        file.seek(SeekFrom::Current(compressed_size as i64 - 20 - 6 + 4))?;

        let mut size_buf: [u8; 4] = [0, 0, 0, 0];
        file.read_exact(&mut size_buf)?;
        let uncompressed_size = u32::from_le_bytes(size_buf);
        csv_out.write_record(&[
            format!("{}", offset),
            format!("{}", header.header_size()),
            format!("{}", compressed_size),
            format!("{}", uncompressed_size),
        ])?;

        if uncompressed_size == 0 {
            break;
        }
    }

    Ok(())
}
