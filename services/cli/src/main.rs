//! `workbench-cli`: command-line companion to `workbench-mcp`.

use std::process::ExitCode;

use clap::Parser;

use crate::{
    adapters::DirFileStore,
    cli::{Cli, Command},
};

mod adapters;
mod cli;
mod utils;

fn run(cli: Cli) -> Result<(), engine::Error> {
    match cli.command {
        Command::Init { platform, out } => {
            let platforms = platform.into_iter().map(Into::into).collect();
            let (path, platforms) = engine::init(&DirFileStore::new(out), platforms)?;
            let names: Vec<_> = platforms.iter().map(ToString::to_string).collect();
            println!(
                "Initialized Workbench for {} at {}",
                names.join(", "),
                path.display()
            );
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
