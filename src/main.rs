//! `workbench`: sets up a workspace of projects and records what it contains.

use std::{env, process::ExitCode};

use clap::Parser;
use console::Style;

use crate::{
    cli::{
        Cli, Commands,
        commands::{index, init, project},
        log::Logger,
        workspace::Workspace,
    },
    errors::{WorkbenchErrors, WorkbenchResult},
};

mod cli;
mod errors;
mod graph;

/// Exit codes: 0 on success, 1 when the command failed. Usage errors exit with 2 (clap).
fn main() -> ExitCode {
    let cli = Cli::parse();
    let command = command_name(&cli.command);
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(command, &error);
            ExitCode::FAILURE
        }
    }
}

fn run(command: Commands) -> WorkbenchResult<()> {
    match command {
        Commands::Init(args) => init::run(args)?,
        Commands::Project(command) => project::run(command)?,
        Commands::Index(args) => index::run(&args)?,
    }
    Ok(())
}

/// The name a log entry is filed under.
fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Init(_) => "init",
        Commands::Project(_) => "project",
        Commands::Index(_) => "index",
    }
}

/// Prints the friendly message to stderr, and writes the details to the workspace log.
///
/// Outside a workspace there is no log, so only the message is shown.
fn report(command: &'static str, error: &WorkbenchErrors) {
    let colors = console::colors_enabled_stderr();
    let paint = |style: Style, text: &str| style.force_styling(colors).apply_to(text).to_string();
    eprintln!(
        "{} {}",
        paint(Style::new().red().bold(), "error:"),
        error.render(colors)
    );

    let Some(workspace) = env::current_dir()
        .ok()
        .and_then(|directory| Workspace::find(&directory).ok())
    else {
        return;
    };
    let logger = Logger::new(workspace.root(), command);
    if logger.error(&format!("{error}\ndetails: {error:?}")) {
        eprintln!(
            "{} {}",
            paint(Style::new().dim(), "Details are in"),
            workspace.relative_path(logger.path())
        );
    }
}
