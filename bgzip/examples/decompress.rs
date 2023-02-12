use bgzip::read::BGZFReader;
use clap::Parser;
use std::fs::File;
use std::io::prelude::*;

#[cfg(not(feature = "rayon"))]
#[derive(Debug, Parser)]
struct Cli {
    #[command()]
    input_file: String,
    #[arg(short, long)]
    output: String,
}

#[cfg(feature = "rayon")]
#[derive(Debug, Parser)]
struct Cli {
    #[command()]
    input_file: String,
    #[arg(short, long)]
    output: String,
    #[arg(short = '@', long)]
    thread: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let file_reader = File::open(&cli.input_file)?;
    let mut file_writer = File::create(&cli.output)?;

    #[cfg(feature = "rayon")]
    let mut reader: Box<dyn Read> = if let Some(thread) = cli.thread {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread)
            .build_global()?;
        Box::new(bgzip::read::BGZFMultiThreadReader::new(file_reader)?)
    } else {
        Box::new(BGZFReader::new(file_reader)?)
    };

    #[cfg(not(feature = "rayon"))]
    let mut reader = BGZFReader::new(file_reader)?;

    std::io::copy(&mut reader, &mut file_writer)?;

    Ok(())
}
