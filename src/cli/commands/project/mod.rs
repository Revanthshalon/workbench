use clap::Subcommand;

use crate::cli::errors::CliResult;

pub mod add;
pub mod kinds;

#[derive(Debug, Subcommand)]
pub enum ProjectCommands {
    /// Add a project to the workspace.
    Add(add::AddArgs),
}

pub fn run(command: ProjectCommands) -> CliResult<()> {
    match command {
        ProjectCommands::Add(args) => add::run(args),
    }
}
