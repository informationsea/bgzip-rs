use bgzip::{BGZFError, BGZFWriter};
use clap::{App, Arg};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Copy)]
enum Mode {
    Compress,
    Decompress,
}

fn main() -> Result<(), BGZFError> {
    let matches = App::new("bgzip")
        .version("0.1.0")
        .author("Okamura, Yasunobu")
        .about("Rust implementation of bgzip")
        .arg(
            Arg::with_name("decompress")
                .short("d")
                .long("decompress")
                .conflicts_with("compress")
                .help("Force decompress"),
        )
        .arg(
            Arg::with_name("compress")
                .short("z")
                .long("compress")
                .conflicts_with("decompress")
                .help("Force compress"),
        )
        .arg(
            Arg::with_name("keep")
                .short("k")
                .long("keep")
                .help("keep input files"),
        )
        .arg(
            Arg::with_name("stdout")
                .short("c")
                .long("stdout")
                .help("write on standard output"),
        )
        .arg(
            Arg::with_name("fast")
                .short("1")
                .long("fast")
                .conflicts_with("better")
                .help("compress faster"),
        )
        .arg(
            Arg::with_name("better")
                .short("9")
                .long("best")
                .conflicts_with("fast")
                .help("compress better"),
        )
        .arg(
            Arg::with_name("force")
                .short("f")
                .long("force")
                .help("overwrite existing file"),
        )
        .arg(
            Arg::with_name("files")
                .index(1)
                .takes_value(true)
                .multiple(true),
        )
        .get_matches();

    let level = if matches.is_present("best") {
        flate2::Compression::best()
    } else if matches.is_present("fast") {
        flate2::Compression::fast()
    } else {
        flate2::Compression::default()
    };

    let stdin = io::stdin();
    let stdout = io::stdout();

    let files: Vec<Option<String>> = if let Some(files) = matches.values_of("files") {
        files
            .map(|x| {
                if x == "-" || matches.is_present("stdout") {
                    None
                } else {
                    Some(x.to_string())
                }
            })
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
            if matches.is_present("decompress")
                || (x.ends_with(".gz") && !matches.is_present("compress"))
            {
                let output_filename = if x.ends_with(".gz") {
                    x[..x.len() - 3].to_string()
                } else {
                    format!("{}.decompressed", x)
                };

                if Path::new(&output_filename).exists() && !matches.is_present("force") {
                    return Err(BGZFError::Other {
                        message: "already exist",
                    });
                }
                (
                    Mode::Decompress,
                    Box::new(fs::File::create(output_filename)?),
                )
            } else {
                let output_filename = format!("{}.gz", x);
                if Path::new(&output_filename).exists() && !matches.is_present("force") {
                    return Err(BGZFError::Other {
                        message: "already exist",
                    });
                }

                (Mode::Compress, Box::new(fs::File::create(output_filename)?))
            }
        } else if matches.is_present("decompress") {
            (Mode::Decompress, Box::new(stdout.lock()))
        } else {
            (Mode::Compress, Box::new(stdout.lock()))
        };

        match mode {
            Mode::Decompress => {
                //println!("decompress");
                let mut reader = flate2::bufread::MultiGzDecoder::new(input);
                io::copy(&mut reader, &mut output)?;
            }
            Mode::Compress => {
                let mut writer = BGZFWriter::new(output, level);
                io::copy(&mut input, &mut writer)?;
            }
        }
    }

    Ok(())
}
