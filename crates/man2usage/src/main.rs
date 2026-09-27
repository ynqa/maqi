//! Generate Usage specifications from installed or Debian command manuals.
mod emit;
mod input;
mod model;
mod parse;

use anyhow::{Result, ensure};
use clap::{ArgGroup, Parser};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    version,
    about = "Generate Usage KDL from installed or Debian man pages"
)]
#[command(group(ArgGroup::new("source").required(true).args(["bin", "debian_man_url"])))]
struct Cli {
    /// Look up an installed command's man page; fail if it is missing.
    #[arg(long, value_name = "COMMAND")]
    bin: Option<String>,
    /// Root man-page URL on manpages.debian.org; include its subcommand manuals.
    #[arg(long, value_name = "URL")]
    debian_man_url: Option<String>,
    /// Write KDL to this file; omit to write to stdout.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let documents = if let Some(bin) = &cli.bin {
        input::bin::load(bin)?
    } else {
        input::debian::load(cli.debian_man_url.as_deref().unwrap())?
    };
    let manuals = documents
        .iter()
        .map(parse::parse)
        .collect::<Result<Vec<_>>>()?;
    let root = &manuals[0];
    ensure!(
        root.command.len() == 1,
        "specify the root command's man page, not a subcommand"
    );
    let bin = &root.command[0];
    if let Some(requested) = cli.bin {
        ensure!(&requested == bin, "manual describes {bin}, not {requested}");
    }
    let generated = emit::generate(bin, &manuals)?;
    if let Some(output) = cli.output {
        fs::write(output, &generated)?;
    } else {
        io::stdout().lock().write_all(generated.as_bytes())?;
    }
    eprintln!(
        "{} manuals converted; {} bytes",
        manuals.len(),
        generated.len()
    );
    Ok(())
}
