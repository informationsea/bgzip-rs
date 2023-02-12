use anyhow::Context;
use bgzip::{read::BGZFMultiThreadReader, write::BGZFMultiThreadWriter, BGZFReader, BGZFWriter};
use clap::Parser;
use is_terminal::IsTerminal;
use std::fs::File;
use std::io::prelude::*;

#[derive(Debug, Parser, PartialEq, Clone)]
#[command(author, version, about)]
struct Cli {
    // #[arg(
    //     short = 'b',
    //     long = "offset",
    //     help = "decompress at virtual file pointer (0-based uncompressed offset)",
    //     requires = "stdout"
    // )]
    // offset: Option<u64>,
    #[arg(
        short = 'c',
        long = "stdout",
        help = "write on standard output, keep original files unchanged"
    )]
    stdout: bool,
    #[arg(short = 'd', long = "decompress", help = "Decompress.")]
    decompress: bool,
    #[arg(short = 'f', long = "force", help = "overwrite files without asking")]
    force: bool,
    #[arg(short = 'i', long = "index", help = "compress and create BGZF index")]
    index: bool,
    #[arg(
        short = 'I',
        long = "index-name",
        help = "name of BGZF index file [file.gz.gzi]"
    )]
    index_name: Option<String>,
    #[arg(
        short = 'k',
        long = "keep",
        help = "don't delete input files during operation"
    )]
    keep: bool,
    #[arg(
        short = 'l',
        long = "compress-level",
        help = "Compression level to use when compressing; 0 to 9, or -1 for default [-1]",
        default_value = "-1"
    )]
    compress_level: i32,
    // #[arg(short = 'r', long = "reindex", help = "(re)index compressed file")]
    // reindex: bool,
    // #[arg(
    //     short = 's',
    //     long = "size",
    //     help = "decompress INT bytes (uncompressed size)",
    //     requires = "offset"
    // )]
    // size: Option<u64>,
    #[arg(short = 't', long = "test", help = "test integrity of compressed file")]
    test: bool,
    #[arg(
        short = '@',
        long = "threads",
        help = "number of compression threads to use [1]"
    )]
    threads: Option<usize>,
    #[arg(index = 1, help = "files to process")]
    files: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    rayon::ThreadPoolBuilder::new()
        .num_threads(cli.threads.unwrap_or(1))
        .build_global()
        .context("Failed to set number of threads in thread pool")?;

    if cli.files.is_empty() {
        process_file(&cli, None)?;
    } else {
        for one in &cli.files {
            if one == "-" {
                process_file(&cli, None)?;
            } else {
                process_file(&cli, Some(one.as_str()))?;
            }
        }
    }

    Ok(())
}

fn process_file(cli: &Cli, input_path: Option<&str>) -> anyhow::Result<()> {
    let compression = match cli.compress_level {
        -1 => bgzip::Compression::default(),
        i if i >= 0 && i <= 12 => bgzip::Compression::new(
            cli.compress_level
                .try_into()
                .context("Compression level must be -1 to 12")?,
        )?,
        _ => return Err(anyhow::anyhow!("Compression level must be -1 to 12")),
    };

    let mut delete_input = !cli.keep;

    let mut input: Box<dyn Read> = if let Some(path) = input_path {
        if path.ends_with(".gz") && !cli.decompress {
            eprintln!("{} already has .gz suffix -- unchanged", path);
            return Ok(());
        }

        Box::new(File::open(path)?)
    } else {
        delete_input = false;
        Box::new(std::io::stdin().lock())
    };

    let (mut output, index_out): (Box<dyn Write>, Option<File>) = if let Some(path) = input_path
        .map(|x| if cli.stdout { None } else { Some(x) })
        .flatten()
    {
        let new_path = if cli.decompress {
            if path.ends_with(".gz") {
                path[..(path.len() - 3)].to_string()
            } else {
                return Err(anyhow::anyhow!("{}: unknown suffix", path));
            }
        } else {
            format!("{}.gz", path)
        };
        let index_path = if cli.index && !cli.decompress {
            Some(
                cli.index_name
                    .clone()
                    .unwrap_or_else(|| format!("{}.gzi", new_path)),
            )
        } else {
            None
        };

        if std::path::Path::new(new_path.as_str()).exists() && !cli.force {
            return Err(anyhow::anyhow!(
                "{} already exists. Use -f to force overwrite.",
                new_path
            ));
        }
        (
            Box::new(File::create(new_path)?),
            index_path.map(|x| File::create(x)).transpose()?,
        )
    } else {
        if std::io::stdout().is_terminal() && !cli.force && !cli.decompress {
            return Err(anyhow::anyhow!(
                "compressed data not written to a terminal. Use -f to force compression."
            ));
        }
        delete_input = false;
        (Box::new(std::io::stdout().lock()), None)
    };

    if cli.decompress {
        if cli.threads.is_some() {
            let mut reader = BGZFMultiThreadReader::new(&mut input)?;
            std::io::copy(&mut reader, &mut output)?;
        } else {
            let mut reader = BGZFReader::new(&mut input)?;
            std::io::copy(&mut reader, &mut output)?;
        }
    } else {
        if cli.threads.is_some() {
            let mut writer = BGZFMultiThreadWriter::new(&mut output, compression);
            std::io::copy(&mut input, &mut writer)?;
            let index = writer.close()?;
            if let Some(index_out) = index_out {
                index.unwrap().write(std::io::BufWriter::new(index_out))?;
            }
        } else {
            let mut writer = BGZFWriter::new(&mut output, compression);
            std::io::copy(&mut input, &mut writer)?;
            let index = writer.close()?;
            if let Some(index_out) = index_out {
                index.unwrap().write(std::io::BufWriter::new(index_out))?;
            }
        }
    }

    if let Some(path) = input_path {
        if delete_input {
            std::fs::remove_file(path)?;
        }
    }

    Ok(())
}
