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
        )
        .arg(
            Arg::with_name("seqname")
                .long("seqname")
                .short("s")
                .takes_value(true)
                .required(true)
                .help("chromsome name"),
        )
        .arg(
            Arg::with_name("begin")
                .long("begin")
                .short("b")
                .takes_value(true)
                .required(true)
                .help("begin position"),
        )
        .arg(
            Arg::with_name("end")
                .long("end")
                .short("e")
                .takes_value(true)
                .required(true)
                .help("end position"),
        );

    let matches = app.get_matches();

    let mut file =
        bgzip::index::tbi::TabixFile::with_filename(matches.value_of("file").unwrap()).unwrap();
}
