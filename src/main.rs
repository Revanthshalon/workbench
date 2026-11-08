#![allow(missing_docs)]

use std::process::ExitCode;

use clap::Parser;

use crate::{
    cli::{
        Cli, Commands,
        commands::{init, project},
    },
    errors::WorkbenchResult,
};

mod cli;
mod errors;
mod mcp;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> WorkbenchResult<()> {
    match Cli::parse().command {
        Commands::Init(args) => init::run(args)?,
        Commands::Project(command) => project::run(command)?,
    }
    Ok(())
}
