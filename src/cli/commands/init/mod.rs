use std::{fs, path::PathBuf, time::Duration};

use clap::Args;
use indicatif::{ProgressBar, ProgressFinish};

use crate::cli::{
    constants::{
        CONTEXT_NAME_PLACEHOLDER, GITIGNORE_FILE, SPINNER_TICK_MILLISECONDS, WORKBENCH_CONFIG_FILE,
        WORKBENCH_CONTEXT_TEMPLATE, WORKBENCH_CONTEXTS_DIRECTORY, WORKBENCH_DEFAULT_CONTEXTS,
        WORKBENCH_DIRECTORY, WORKBENCH_INIT_MEMORY_FILE, WORKBENCH_INIT_MEMORY_TEMPLATE,
        WORKBENCH_LOGS_DIRECTORY, WORKBENCH_MCP_CONFIG_FILE, WORKBENCH_MCP_CONFIG_TEMPLATE,
        WORKBENCH_MCP_DIRECTORY, WORKBENCH_MEMORY_DIRECTORY, WORKBENCH_SKILLS_DIRECTORY,
    },
    errors::CliResult,
};

mod files;
mod reporter;
#[cfg(test)]
mod tests;

use reporter::Reporter;

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Workspace root in which the `.workbench` folder is created.
    #[arg(long, value_name = "DIR", default_value = ".")]
    pub out: PathBuf,
    /// Rewrite existing config files under `.workbench` to their defaults.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: InitArgs) -> CliResult<()> {
    println!("Initialising workspace in {}\n", args.out.display());

    // Cleared on every exit path, so an error never leaves a stale spinner behind.
    let spinner = ProgressBar::new_spinner().with_finish(ProgressFinish::AndClear);
    spinner.enable_steady_tick(Duration::from_millis(SPINNER_TICK_MILLISECONDS));
    let reporter = Reporter {
        spinner: &spinner,
        workspace_root: &args.out,
    };

    initialise(&args, &reporter)?;

    spinner.finish_and_clear();
    Ok(())
}

fn initialise(args: &InitArgs, reporter: &Reporter<'_>) -> CliResult<()> {
    let spinner = reporter.spinner;
    spinner.set_message("Creating directories");
    let base_path = args.out.join(WORKBENCH_DIRECTORY);
    let memory_path = base_path.join(WORKBENCH_MEMORY_DIRECTORY);
    let mcp_path = base_path.join(WORKBENCH_MCP_DIRECTORY);
    let contexts_path = mcp_path.join(WORKBENCH_CONTEXTS_DIRECTORY);
    let skills_path = base_path.join(WORKBENCH_SKILLS_DIRECTORY);
    let logs_path = base_path.join(WORKBENCH_LOGS_DIRECTORY);
    for directory in [&memory_path, &contexts_path, &skills_path, &logs_path] {
        fs::create_dir_all(directory)?;
    }

    spinner.set_message("Writing files");
    files::write_config(reporter, &base_path.join(WORKBENCH_CONFIG_FILE), args.force)?;
    // The memory file holds hand-written notes, so `--force` never rewrites it.
    files::write_template(
        reporter,
        &memory_path.join(WORKBENCH_INIT_MEMORY_FILE),
        WORKBENCH_INIT_MEMORY_TEMPLATE,
        false,
    )?;
    files::write_template(
        reporter,
        &mcp_path.join(WORKBENCH_MCP_CONFIG_FILE),
        WORKBENCH_MCP_CONFIG_TEMPLATE,
        args.force,
    )?;
    for context in WORKBENCH_DEFAULT_CONTEXTS {
        files::write_template(
            reporter,
            &contexts_path.join(format!("{context}.yml")),
            &WORKBENCH_CONTEXT_TEMPLATE.replace(CONTEXT_NAME_PLACEHOLDER, context),
            args.force,
        )?;
    }

    spinner.set_message("Updating .gitignore");
    files::ignore_workbench_directory(reporter, &args.out.join(GITIGNORE_FILE))
}
