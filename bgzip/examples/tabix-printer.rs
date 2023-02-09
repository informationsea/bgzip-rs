use bgzip::tabix::Tabix;
use clap::Parser;
use std::fs::File;
use std::io::{stdout, Write};

#[derive(Debug, Parser)]
struct Args {
    #[command()]
    file: String,
    #[arg(short, long)]
    output: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let parser = Args::parse();

    let file = Tabix::from_reader(File::open(&parser.file)?)?;
    let out: Box<dyn Write> = if let Some(out) = parser.output {
        Box::new(File::create(out)?)
    } else {
        Box::new(stdout().lock())
    };
    let mut csv_out = csv::WriterBuilder::new().flexible(true).from_writer(out);

    csv_out.write_record(&[
        "# of sequences",
        "format",
        "coordinate rule",
        "column for the sequence name",
        "column for the start of a region",
        "column for the end fo a region",
        "meta",
        "skip",
        "Length of concatenated sequence names",
    ])?;

    csv_out.write_record(&[
        format!("{}", file.number_of_references),
        match file.format & 0xffff {
            0 => "Generic".to_string(),
            1 => "SAM".to_string(),
            2 => "VCF".to_string(),
            _ => format!("Unknown: {}", file.format),
        },
        match file.format & 0x10000 {
            0 => "GFF Rule".to_string(),
            _ => "BED Rule".to_string(),
        },
        format!("{}", file.column_for_sequence),
        format!("{}", file.column_for_begin),
        format!("{}", file.column_for_end),
        format!("{}", String::from_utf8_lossy(&file.meta)),
        format!("{}", file.skip),
        format!("{}", file.length_of_concatenated_sequence_names),
    ])?;

    csv_out.write_record(&[""])?;
    csv_out.write_record(&[
        "sequence index",
        "sequence name",
        "bin index",
        "bin",
        "chunk index",
        "begin",
        "end",
    ])?;
    for (i, (ref_name, sequence)) in file.names.iter().zip(file.sequences.iter()).enumerate() {
        let mut bins: Vec<_> = sequence.bins.values().collect();
        bins.sort_by_key(|x| x.bin);
        for (j, bin) in bins.iter().enumerate() {
            for (k, x) in bin.chunks.iter().enumerate() {
                csv_out.write_record(&[
                    format!("{}", i),
                    String::from_utf8_lossy(ref_name).to_string(),
                    format!("{}", j),
                    format!("0x{:x}", bin.bin),
                    format!("{}", k),
                    format!("0x{:x}", x.begin),
                    format!("0x{:x}", x.end),
                ])?;
            }
        }
    }

    Ok(())
}
