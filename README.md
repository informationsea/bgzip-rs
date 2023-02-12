bgzip-rs
========

[![Build](https://github.com/informationsea/bgzip-rs/actions/workflows/build.yml/badge.svg)](https://github.com/informationsea/bgzip-rs/actions/workflows/build.yml)
[![Crates.io](https://img.shields.io/crates/v/bgzip)](https://crates.io/crates/bgzip)
[![Crates.io](https://img.shields.io/crates/d/bgzip)](https://crates.io/crates/bgzip)
[![Crates.io](https://img.shields.io/crates/l/bgzip)](https://crates.io/crates/bgzip)
[![doc-rs](https://docs.rs/bgzip/badge.svg)](https://docs.rs/bgzip)

Rust implementation of BGZF

Feature flags
-------------

* `rayon`: Enable [rayon](https://github.com/rayon-rs/rayon) based multi-threaded reader/writer. This is default feature.
* `log`: Enable [log](https://github.com/rust-lang/log) crate to log warnings. This is default feature.
* `rust_backend`: use [miniz_oxide](https://crates.io/crates/miniz_oxide) crate for [flate2](https://github.com/rust-lang/flate2-rs) backend. This is default feature.
* `zlib`: use `zlib` for flate2 backend. Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
* `zlib-ng`: use `zlib-ng` for flate2 backend. Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
* `zlib-ng-compat`: Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
* `cloudflare_zlib`: Please read [flate2](https://github.com/rust-lang/flate2-rs) description for the detail.
* `libdeflater`: use [libdeflater](https://github.com/adamkewley/libdeflater) instead of [flate2](https://github.com/rust-lang/flate2-rs) crate.

Write Examples
--------
```rust
use bgzip::{BGZFWriter, BGZFError, Compression};
use std::io::{self, Write};
fn main() -> Result<(), BGZFError> {
    let mut write_buffer = Vec::new();
    let mut writer = BGZFWriter::new(&mut write_buffer, Compression::default());
    writer.write_all(b"##fileformat=VCFv4.2\n")?;
    writer.write_all(b"#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")?;
    writer.close()?;
    Ok(())
}
```

Read Examples
--------
```rust
use bgzip::{BGZFReader, BGZFError};
use std::io::{self, BufRead};
use std::fs;
fn main() -> Result<(), BGZFError> {
    let mut reader =
        BGZFReader::new(fs::File::open("testfiles/common_all_20180418_half.vcf.gz")?)?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    assert_eq!("##fileformat=VCFv4.0\n", line);
    reader.bgzf_seek(4210818610)?;
    line.clear();
    reader.read_line(&mut line)?;
    assert_eq!("1\t72700625\trs12116859\tT\tA,C\t.\t.\tRS=12116859;RSPOS=72700625;dbSNPBuildID=120;SSR=0;SAO=0;VP=0x05010008000517053e000100;GENEINFO=LOC105378798:105378798;WGT=1;VC=SNV;SLO;INT;ASP;VLD;G5A;G5;HD;GNO;KGPhase1;KGPhase3;CAF=0.508,.,0.492;COMMON=1;TOPMED=0.37743692660550458,0.00608435270132517,0.61647872069317023\n", line);

    Ok(())
}
```

Author
------

Yasunobu OKAMURA

License
-------

MIT

