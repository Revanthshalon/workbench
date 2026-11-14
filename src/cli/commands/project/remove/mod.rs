//! `workbench project remove <ID>`: takes a project out of the workspace config.
//!
//! The project's memory folder is kept, so adding it again under the same id finds
//! its notes. `--purge-memory` deletes the folder too, after showing what it holds
//! and asking for confirmation. `workbench project cleanup` deletes kept folders later.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use clap::Args;
use console::{Style, colors_enabled};

use super::memory::{files_in, is_plain_folder_name};
use crate::cli::{
    errors::{CliErrors, CliResult},
    prompt::{Prompt, Terminal, confirmed},
    table::{self, Kind},
    workspace::Workspace,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Id of the project to remove.
    pub id: String,
    /// Also delete the project's memory folder, `.workbench/memory/<ID>/`.
    #[arg(long)]
    pub purge_memory: bool,
    /// Delete without asking for confirmation.
    #[arg(long, short)]
    pub yes: bool,
}

pub fn run(args: &RemoveArgs) -> CliResult<()> {
    execute(args, &env::current_dir()?, colors_enabled(), &mut Terminal)
}

/// Removes the project; with `--purge-memory`, lists and deletes its memory folder.
///
/// Nothing is changed until every check has passed and any confirmation is given.
pub(super) fn execute(
    args: &RemoveArgs,
    current_directory: &Path,
    colors: bool,
    prompt: &mut impl Prompt,
) -> CliResult<()> {
    let workspace = Workspace::find(current_directory)?;
    remove_project(&args.id, args, workspace, colors, prompt)
}

/// Removes one project; with `--purge-memory`, lists and deletes its memory folder.
fn remove_project(
    id: &str,
    args: &RemoveArgs,
    mut workspace: Workspace,
    colors: bool,
    prompt: &mut impl Prompt,
) -> CliResult<()> {
    let known: Vec<String> = workspace
        .projects()?
        .into_iter()
        .map(|project| project.id)
        .collect();
    if !known.iter().any(|known_id| known_id == id) {
        return Err(CliErrors::UnknownProjectId {
            id: id.to_owned(),
            known,
        });
    }

    let memory_path = workspace.memory_path().join(id);
    let purge = if args.purge_memory && memory_path.exists() {
        if !is_plain_folder_name(id) {
            return Err(CliErrors::UnsafeMemoryPath(id.to_owned()));
        }
        let files = files_in(&memory_path)?;
        for line in purge_listing(&workspace, &memory_path, &files, colors) {
            prompt.print(&line);
        }
        if !confirmed(
            args.yes,
            &format!("Delete the memory of `{id}`?"),
            colors,
            prompt,
        )? {
            return Ok(());
        }
        true
    } else {
        false
    };

    workspace.remove_project(id)?;
    if purge {
        fs::remove_dir_all(&memory_path)?;
    }

    let memory = format!("{}/", workspace.relative_path(&memory_path));
    for line in summary(
        id,
        &workspace,
        &memory,
        memory_state(purge, args, &memory_path),
        colors,
    ) {
        prompt.print(&line);
    }
    Ok(())
}

/// What happened to the project's memory folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryState {
    Kept,
    Deleted,
    Missing,
}

fn memory_state(purged: bool, args: &RemoveArgs, memory_path: &Path) -> MemoryState {
    if purged {
        MemoryState::Deleted
    } else if !args.purge_memory && memory_path.exists() {
        MemoryState::Kept
    } else {
        MemoryState::Missing
    }
}

/// The output after removal: a heading, then one bullet per consequence.
fn summary(
    id: &str,
    workspace: &Workspace,
    memory: &str,
    state: MemoryState,
    colors: bool,
) -> Vec<String> {
    let dim = Style::new().dim();
    let paint = |style: &Style, text: &str| table::paint(style, text, colors);

    let heading = format!(
        "{} {} {} {}",
        paint(&Style::new().red().bold(), "Removed project"),
        paint(&Style::new().cyan().bold(), id),
        paint(&dim, "from"),
        workspace.relative_path(workspace.config_path()),
    );
    let memory_bullet = match state {
        MemoryState::Kept => format!(
            "{} {memory} {}",
            paint(&Style::new().green(), "Kept"),
            paint(&dim, &format!("(adding `{id}` again finds its notes)")),
        ),
        MemoryState::Deleted => format!("{} {memory}", paint(&Style::new().red(), "Deleted")),
        MemoryState::Missing => format!("{} {memory}", paint(&dim, "No memory folder at")),
    };
    let index_bullet = format!(
        "{} {}",
        paint(&Style::new().yellow(), "Indexed data"),
        paint(&dim, "is removed by the next `workbench index`"),
    );

    vec![
        heading,
        String::new(),
        table::bullet(&memory_bullet, colors),
        table::bullet(&index_bullet, colors),
    ]
}

/// What `--purge-memory` is about to delete, as a table of files and their sizes.
fn purge_listing(
    workspace: &Workspace,
    memory_path: &Path,
    files: &[(PathBuf, u64)],
    colors: bool,
) -> Vec<String> {
    let heading = format!(
        "{} {}/",
        table::paint(
            &Style::new().red().bold(),
            "--purge-memory will delete",
            colors
        ),
        workspace.relative_path(memory_path),
    );
    if files.is_empty() {
        return vec![heading];
    }
    let rows: Vec<[String; 2]> = files
        .iter()
        .map(|(path, size)| [workspace.relative_path(path), format!("{size} B")])
        .collect();
    let mut lines = vec![heading];
    lines.extend(table::render(
        ["FILE", "SIZE"],
        &rows,
        |column, _| {
            if column == 0 {
                Kind::Danger
            } else {
                Kind::Plain
            }
        },
        colors,
    ));
    lines
}
