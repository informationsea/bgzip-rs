extern crate bgzip;
extern crate clap;

use clap::{App, AppSettings, Arg, ArgMatches};
use std::boxed::Box;
use std::cmp::min;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::str;

fn main() {
    let app = App::new("tbiview")
        .about("Tabix index viewer")
        .author("Yasunobu OKAMURA")
        .version("0.1")
        .setting(AppSettings::ColorAuto)
        .setting(AppSettings::ColoredHelp)
        .arg(
            Arg::with_name("file")
                .index(1)
                .takes_value(true)
                .required(true)
                .help("Indexed file"),
        );

    let matches = app.get_matches();

    let mut file =
        bgzip::index::tbi::TabixFile::with_filename(matches.value_of("file").unwrap()).unwrap();
    let index = file.tabix.clone();

    println!("n_ref = {}", index.n_ref);
    println!("format = {}", index.format);
    println!("col_seq = {}", index.col_seq);
    println!("col_beg = {}", index.col_beg);
    println!("col_end = {}", index.col_end);
    println!(
        "meta = {}  -  {}",
        index.meta,
        str::from_utf8(&[index.meta as u8][..]).unwrap()
    );
    println!("skip = {}", index.skip);
    println!("l_nm = {}", index.l_nm);
    print!("names: ");
    for (i, one) in (&index.names).into_iter().enumerate() {
        if i != 0 {
            print!(", ");
        }
        print!("{}", str::from_utf8(&one).unwrap());
    }
    println!("");

    for (i, seq_index) in (&index.seq_index).into_iter().enumerate() {
        let seq_name = str::from_utf8(&index.names[i]).unwrap();
        println!("sequence {}", seq_name);
        println!("  n_bin = {}", seq_index.n_bin);
        for (j, bin_entry) in (&seq_index.bins).into_iter().enumerate() {
            let bin = bin_entry.1;
            println!("{}[{}]", seq_name, j);
            println!("    bin = {:10}  {:32b}", bin.bin, bin.bin);

            println!("    n_chunk = {}", bin.n_chunk);
            for (k, chunk) in (&bin.chunks).into_iter().enumerate() {
                print!(
                    "    [{}] chunk_beg = {}({}/{})  chunk_end = {}({}/{})  : ",
                    k,
                    chunk.chunk_beg,
                    chunk.chunk_beg >> 16,
                    chunk.chunk_beg & 0xffff,
                    chunk.chunk_end,
                    chunk.chunk_end >> 16,
                    chunk.chunk_end & 0xffff,
                );
                file.reader
                    .seek_virtual_file_offset(chunk.chunk_beg)
                    .unwrap();

                let mut data = String::new();
                file.reader.read_line(&mut data).unwrap();
                print!("{}", data);
            }
        }
        println!("  n_intv = {}", seq_index.n_intv);

        for (j, interval) in (&seq_index.interval).into_iter().enumerate() {
            println!("  [{}] {}", j, interval);
        }
    }
}
