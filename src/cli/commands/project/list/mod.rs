//! `workbench project list`: the projects in the workspace, as a table.

use std::env;

use console::{Style, colors_enabled};

use crate::cli::{
    errors::CliResult,
    table::{self, Kind, MISSING},
    workspace::{ExistingProject, Workspace},
};

#[cfg(test)]
mod tests;

const HEADERS: [&str; 4] = ["ID", "LANGUAGE", "DEPENDENCY MANAGER", "PATH"];
const KINDS: [Kind; 4] = [
    Kind::Id,
    Kind::Language,
    Kind::DependencyManager,
    Kind::Plain,
];
const NO_PROJECTS_HINT: &str = "No projects yet. Add one with `workbench project add <PATH>`.";

pub fn run() -> CliResult<()> {
    let workspace = Workspace::find(&env::current_dir()?)?;
    for line in render(&workspace.projects()?, colors_enabled()) {
        println!("{line}");
    }
    Ok(())
}

/// The output lines: a table with one row per project, then a count.
fn render(projects: &[ExistingProject], colors: bool) -> Vec<String> {
    let dim = Style::new().dim();
    if projects.is_empty() {
        return vec![table::paint(&dim, NO_PROJECTS_HINT, colors)];
    }

    let rows: Vec<[String; 4]> = projects.iter().map(cells).collect();
    let mut lines = table::render(HEADERS, &rows, |column, _| KINDS[column], colors);
    let noun = if projects.len() == 1 {
        "project"
    } else {
        "projects"
    };
    lines.push(table::paint(
        &dim,
        &format!("{} {noun}", projects.len()),
        colors,
    ));
    lines
}

/// The plain text of each cell, in column order.
fn cells(project: &ExistingProject) -> [String; 4] {
    [
        project.id.clone(),
        language(project),
        project
            .dependency_manager
            .clone()
            .unwrap_or_else(|| MISSING.to_owned()),
        project.path.clone(),
    ]
}

/// The language, written as `typescript/angular` when a framework is set.
fn language(project: &ExistingProject) -> String {
    match (&project.language, &project.framework) {
        (Some(language), Some(framework)) => format!("{language}/{framework}"),
        (Some(language), None) => language.clone(),
        (None, Some(framework)) => format!("{MISSING}/{framework}"),
        (None, None) => MISSING.to_owned(),
    }
}
