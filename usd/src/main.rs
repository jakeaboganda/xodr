//! Export an OpenDRIVE map as an OpenUSD stage.
//!
//! ```sh
//! cargo run -p xodr-usd -- tests/data/town07.xodr
//! cargo run -p xodr-usd -- tests/data/town07.xodr /tmp/town07.usda
//! ```
//!
//! Without an output path the stage goes beside the map, as `<map>.usda`.

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use xodr::load_file_with_provenance;
use xodr_usd::write_stage;

fn main() -> ExitCode {
    let args: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let (input, output) = match args.as_slice() {
        [input] => (input, input.with_extension("usda")),
        [input, output] => (input, output.clone()),
        _ => {
            eprintln!("usage: xodr_usd <map.xodr> [out.usda]");
            return ExitCode::from(2);
        }
    };
    let (net, provenance) = match load_file_with_provenance(input) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("{}: {e}", input.display());
            return ExitCode::FAILURE;
        }
    };
    let written = File::create(&output).and_then(|file| {
        let mut out = BufWriter::new(file);
        write_stage(&net, &provenance, &mut out)?;
        out.flush()
    });
    match written {
        Ok(()) => {
            println!("{}", output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", output.display());
            ExitCode::FAILURE
        }
    }
}
