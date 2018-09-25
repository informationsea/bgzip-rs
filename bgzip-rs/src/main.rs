extern crate bgzip;
extern crate clap;

use clap::{App, AppSettings, Arg, ArgMatches};
use std::boxed::Box;
use std::cmp::min;
use std::fs;
use std::io;
use std::io::prelude::*;

fn main() {
    let app = App::new("bgzip-rs")
        .about("Rust implementation of bgzip")
        .author("Yasunobu OKAMURA")
        .version("0.1")
        .setting(AppSettings::ColorAuto)
        .setting(AppSettings::ColoredHelp)
        .arg(
            Arg::with_name("offset")
                .short("b")
                .long("offset")
                .takes_value(true)
                .help("decompress at virtual file pointer (0-based uncompressed offset)"),
        )
        .arg(
            Arg::with_name("virtual offset")
                .short("v")
                .long("virtual")
                .takes_value(true)
                .help("Virtual offset (see htslib documents)"),
        )
        .arg(
            Arg::with_name("stdout")
                .short("c")
                .long("stdout")
                .help("write on standard output, keep original files unchanged"),
        )
        .arg(
            Arg::with_name("decompress")
                .short("d")
                .long("decompress")
                .help("decompress"),
        )
        .arg(
            Arg::with_name("force")
                .short("f")
                .long("force")
                .help("overwrite files without asking"),
        )
        .arg(
            Arg::with_name("keep")
                .short("k")
                .long("keep")
                .help("keep (don't delete) input files"),
        )
        .arg(
            Arg::with_name("size")
                .short("s")
                .long("size")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("file")
                .index(1)
                .takes_value(true)
                .help("Input file"),
        );
    let matches = app.get_matches();
    //println!("{:?}", matches);

    let result = if matches.is_present("decompress") {
        decompression_mode(matches)
    } else {
        compression_mode(matches)
    };

    match result {
        Ok(_) => (),
        Err(e) => eprintln!("Error: {:?}", e),
    };
}

fn get_output(matches: &ArgMatches) -> io::Result<Box<io::Write>> {
    let output = if matches.is_present("stdout") || !matches.is_present("file") {
        Box::new(io::stdout()) as Box<io::Write>
    } else {
        let original_name = matches.value_of("file").unwrap();
        let new_name = if matches.is_present("decompress") {
            if original_name.ends_with(".gz") {
                original_name[..original_name.len() - 3].to_string()
            } else {
                format!("{}.orig", original_name)
            }
        } else {
            format!("{}.gz", original_name)
        };

        let test_open = fs::File::open(&new_name);
        if !matches.is_present("keep")
            && (test_open.is_ok() || test_open.unwrap_err().kind() != io::ErrorKind::NotFound)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "set -f to overwrite existing file",
            ));
        }

        Box::new(fs::File::create(new_name)?) as Box<io::Write>
    };
    Ok(output)
}

fn decompression_mode(matches: ArgMatches) -> io::Result<()> {
    let mut input = bgzip::read::BGzReader::new(fs::File::open(matches.value_of("file").ok_or(
        io::Error::new(
            io::ErrorKind::Other,
            "input file is required for decompression mode",
        ),
    )?)?)?;

    let mut output = get_output(&matches)?;

    if let Some(voffset) = matches.value_of("virtual offset") {
        input.seek_virtual_file_offset(voffset.parse::<u64>().unwrap())?;
    } else if let Some(offset) = matches.value_of("offset") {
        input.seek(io::SeekFrom::Start(offset.parse::<u64>().unwrap()))?;
    }

    if let Some(bytes) = matches.value_of("size") {
        let mut buf = [0u8; 1024];
        let mut remain: i64 = bytes.parse::<i64>().unwrap();
        while remain > 0 {
            let read_bytes = input.read(&mut buf)?;
            output.write(&buf[..min(remain as usize, buf.len())])?;
            remain -= read_bytes as i64;
        }
    } else {
        io::copy(&mut input, &mut output)?;
    }

    if !matches.is_present("stdout") && !matches.is_present("keep") {
        if let Some(f) = matches.value_of("file") {
            fs::remove_file(f)?;
        }
    }

    Ok(())
}

fn compression_mode(matches: ArgMatches) -> io::Result<()> {
    let mut input = matches.value_of("file").map_or_else(
        || -> Box<io::Read> { Box::new(io::stdin()) },
        |x| -> Box<io::Read> { Box::new(fs::File::open(x).unwrap()) },
    );
    let mut output = bgzip::write::BGzWriter::new(get_output(&matches)?);
    io::copy(&mut input, &mut output)?;

    if !matches.is_present("stdout") && !matches.is_present("keep") {
        if let Some(f) = matches.value_of("file") {
            fs::remove_file(f)?;
        }
    }

    Ok(())
}
