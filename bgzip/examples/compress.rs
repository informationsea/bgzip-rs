use bgzip::write::BGZFWriter;
use clap::Parser;
use std::fs::File;
use std::io::prelude::*;

#[cfg(not(feature = "rayon"))]
#[derive(Debug, Clone, Parser, PartialEq)]
struct Cli {
    #[command()]
    input_file: String,
    #[arg(short, long)]
    output: String,
    #[arg(short, long)]
    compress_level: u32,
}

#[cfg(feature = "rayon")]
#[derive(Debug, Clone, Parser, PartialEq)]
struct Cli {
    #[command()]
    input_file: String,
    #[arg(short, long)]
    output: String,
    #[arg(short, long)]
    compress_level: u32,
    #[arg(short = '@', long)]
    thread: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut file_reader = File::open(&cli.input_file)?;
    let file_writer = File::create(&cli.output)?;

    let level = bgzip::Compression::new(cli.compress_level)?;

    #[cfg(feature = "rayon")]
    let mut writer: Box<dyn Write> = if let Some(thread) = cli.thread {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread)
            .build_global()?;
        Box::new(bgzip::write::BGZFMultiThreadWriter::new(file_writer, level))
    } else {
        Box::new(BGZFWriter::new(file_writer, level))
    };

    #[cfg(not(feature = "rayon"))]
    let mut writer = BGZFWriter::new(file_writer, level)?;

    std::io::copy(&mut file_reader, &mut writer)?;

    Ok(())
}
