use clap::{Parser, Subcommand};

use crate::cli::{
    commands::{init::InitArgs, project::ProjectCommands},
    constants::BANNER,
};

pub mod commands;
pub mod constants;
pub mod errors;
pub mod prompt;
pub mod table;
pub mod workspace;

#[derive(Debug, Parser)]
#[command(version, about, before_help = BANNER, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(InitArgs),
    #[command(subcommand)]
    Project(ProjectCommands),
}
