use clap::Subcommand;

use crate::cli::errors::CliResult;

pub mod add;
pub mod cleanup;
pub mod kinds;
pub mod list;
mod memory;
pub mod remove;

#[derive(Debug, Subcommand)]
pub enum ProjectCommands {
    /// Add a project to the workspace.
    Add(add::AddArgs),
    /// List the projects in the workspace.
    #[command(visible_alias = "ls")]
    List,
    /// Remove a project from the workspace config.
    #[command(visible_alias = "rm")]
    Remove(remove::RemoveArgs),
    /// Delete memory folders that no project in the config uses.
    Cleanup(cleanup::CleanupArgs),
}

pub fn run(command: ProjectCommands) -> CliResult<()> {
    match command {
        ProjectCommands::Add(args) => add::run(args),
        ProjectCommands::List => list::run(),
        ProjectCommands::Remove(args) => remove::run(&args),
        ProjectCommands::Cleanup(args) => cleanup::run(&args),
    }
}
