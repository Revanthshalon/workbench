//! `workbench project cleanup`: deletes orphaned memory folders.
//!
//! A memory folder is orphaned when its name is not the id of any project in the
//! workspace config, for example after `workbench project remove` kept it.

use std::{collections::HashSet, env, fs, path::Path};

use clap::Args;
use console::{Style, colors_enabled};

use super::memory::{files_in, orphaned_folders};
use crate::cli::{
    errors::CliResult,
    prompt::{Prompt, Terminal, confirmed},
    table::{self, Kind},
    workspace::Workspace,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Args)]
pub struct CleanupArgs {
    /// Delete without asking for confirmation.
    #[arg(long, short)]
    pub yes: bool,
}

pub fn run(args: &CleanupArgs) -> CliResult<()> {
    execute(args, &env::current_dir()?, colors_enabled(), &mut Terminal)
}

/// Deletes every memory folder whose name is not the id of a project in the config.
///
/// Files directly in `.workbench/memory/`, such as the workspace `INIT-MEMORY.md`, are
/// never touched, and neither are symbolic links.
pub(super) fn execute(
    args: &CleanupArgs,
    current_directory: &Path,
    colors: bool,
    prompt: &mut impl Prompt,
) -> CliResult<()> {
    let workspace = Workspace::find(current_directory)?;
    let known: HashSet<String> = workspace
        .projects()?
        .into_iter()
        .map(|project| project.id)
        .collect();
    let orphans = orphaned_folders(&workspace.memory_path(), &known)?;
    let memory_root = format!("{}/", workspace.relative_path(&workspace.memory_path()));
    if orphans.is_empty() {
        prompt.print(&table::paint(
            &Style::new().dim(),
            &format!("No orphaned memory folders in {memory_root}"),
            colors,
        ));
        return Ok(());
    }

    let count = folders(orphans.len());
    let mut rows = Vec::new();
    for folder in &orphans {
        let files = files_in(folder)?;
        let size: u64 = files.iter().map(|(_, size)| size).sum();
        rows.push([
            format!("{}/", workspace.relative_path(folder)),
            files.len().to_string(),
            format!("{size} B"),
        ]);
    }
    prompt.print(&format!(
        "{} {} {}",
        table::paint(&Style::new().red().bold(), "Will delete", colors),
        count,
        table::paint(
            &Style::new().dim(),
            "that no project in the config uses",
            colors
        ),
    ));
    for line in table::render(
        ["FOLDER", "FILES", "SIZE"],
        &rows,
        |column, _| {
            if column == 0 {
                Kind::Danger
            } else {
                Kind::Plain
            }
        },
        colors,
    ) {
        prompt.print(&line);
    }
    if !confirmed(args.yes, &format!("Delete {count}?"), colors, prompt)? {
        return Ok(());
    }

    for folder in &orphans {
        fs::remove_dir_all(folder)?;
    }
    prompt.print(&format!(
        "{} {count}",
        table::paint(&Style::new().red().bold(), "Deleted", colors),
    ));
    prompt.print("");
    for [folder, _, _] in &rows {
        prompt.print(&table::bullet(
            &format!(
                "{} {folder}",
                table::paint(&Style::new().red(), "Deleted", colors)
            ),
            colors,
        ));
    }
    Ok(())
}

/// `1 orphaned memory folder`, `2 orphaned memory folders`.
fn folders(count: usize) -> String {
    let noun = if count == 1 { "folder" } else { "folders" };
    format!("{count} orphaned memory {noun}")
}
