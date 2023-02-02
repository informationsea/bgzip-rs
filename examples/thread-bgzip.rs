use bgzip::BGZFError;
use clap::Parser;
use flate2::Compression;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Copy)]
enum Mode {
    Compress,
    Decompress,
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
struct Args {
    #[arg(short, long, conflicts_with = "compress")]
    decompress: bool,
    #[arg(long, conflicts_with = "decompress")]
    compress: bool,
    #[arg(short, long)]
    keep: bool,
    #[arg(short, long)]
    force: bool,
    #[arg(short = 'c', long)]
    stdout: bool,
    #[arg(short = 'l', long = "compress-level")]
    compress_level: Option<u32>,
    #[cfg(feature = "rayon")]
    #[arg(short = 't', long = "thread", default_value = "1")]
    thread: usize,
    #[command()]
    files: Option<Vec<String>>,
}

fn main() -> anyhow::Result<()> {
    let matches = Args::parse();
    let level = if let Some(compression_level) = matches.compress_level {
        if compression_level > 9 {
            panic!("Invalid compression level: {}", compression_level);
        }
        Compression::new(compression_level)
    } else {
        Compression::default()
    };

    #[cfg(feature = "rayon")]
    rayon::ThreadPoolBuilder::new()
        .num_threads(matches.thread)
        .build_global()?;

    let stdin = io::stdin();
    let stdout = io::stdout();

    let files: Vec<Option<String>> = if let Some(files) = matches.files {
        files
            .iter()
            .map(|x| if x == "-" { None } else { Some(x.to_string()) })
            .collect()
    } else {
        vec![None]
    };

    for one in &files {
        let mut input: Box<dyn io::BufRead> = if let Some(x) = one {
            Box::new(io::BufReader::new(fs::File::open(x)?))
        } else {
            Box::new(stdin.lock())
        };
        let (mode, mut output): (Mode, Box<dyn io::Write>) = if let Some(x) = one {
            if matches.decompress || (x.ends_with(".gz") && !matches.compress) {
                let output_filename = if x.ends_with(".gz") {
                    x[..x.len() - 3].to_string()
                } else {
                    format!("{}.decompressed", x)
                };

                if Path::new(&output_filename).exists() && !matches.force {
                    return Err(BGZFError::Other {
                        message: "already exist",
                    }
                    .into());
                }
                (
                    Mode::Decompress,
                    Box::new(fs::File::create(output_filename)?),
                )
            } else {
                let output_filename = format!("{}.gz", x);
                if Path::new(&output_filename).exists() && !matches.force {
                    return Err(BGZFError::Other {
                        message: "already exist",
                    }
                    .into());
                }

                (Mode::Compress, Box::new(fs::File::create(output_filename)?))
            }
        } else if matches.decompress {
            (Mode::Decompress, Box::new(stdout.lock()))
        } else {
            (Mode::Compress, Box::new(stdout.lock()))
        };

        #[cfg(feature = "rayon")]
        match mode {
            Mode::Decompress => {
                //println!("decompress");
                let mut reader = bgzip::read::BGZFMultiThreadReader::with_buf_reader(input);
                io::copy(&mut reader, &mut output)?;
            }
            Mode::Compress => {
                let mut writer = bgzip::write::BGZFMultiThreadWriter::new(output, level)?;
                io::copy(&mut input, &mut writer)?;
            }
        }
    }

    Ok(())
}
