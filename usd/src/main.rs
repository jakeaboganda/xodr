//! Export an OpenDRIVE map as an OpenUSD stage.
//!
//! ```sh
//! cargo run -p xodr-usd -- tests/data/town07.xodr
//! cargo run -p xodr-usd -- tests/data/town07.xodr /tmp/town07.usda
//! ```
//!
//! With no output path, the stage goes next to the map as `<map>.usda`. Each
//! warning the load raises, but those for parts of the file the crate only
//! ignores, goes to stderr. The stage is written whole or not at all.

use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
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
    for warning in provenance.warnings.iter().filter(|w| !w.is_ignored()) {
        eprintln!("{}: warning: {warning}", input.display());
    }
    if fs::canonicalize(&output).is_ok_and(|o| fs::canonicalize(input).is_ok_and(|i| i == o)) {
        eprintln!("{}: the stage would overwrite the map", output.display());
        return ExitCode::from(2);
    }
    let written = write_whole(&output, |out| write_stage(&net, &provenance, out));
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

/// Write `path` with `write`, through a file beside it that replaces it only
/// once whole, so a failed write leaves no part of a stage.
fn write_whole(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let result = (|| {
        let mut out = BufWriter::new(File::create(&partial)?);
        write(&mut out)?;
        out.flush()?;
        fs::rename(&partial, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}
