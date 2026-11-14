//! `workbench`: sets up a workspace of projects and records what it contains.

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

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let colors = console::colors_enabled_stderr();
            let prefix = console::Style::new()
                .red()
                .bold()
                .force_styling(colors)
                .apply_to("error:");
            eprintln!("{prefix} {}", error.render(colors));
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
