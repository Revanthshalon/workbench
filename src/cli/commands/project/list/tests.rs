use clap::Parser;

use super::render;
use crate::cli::{Cli, Commands, commands::project::ProjectCommands, workspace::ExistingProject};

fn project(
    id: &str,
    path: &str,
    language: Option<&str>,
    framework: Option<&str>,
    dependency_manager: Option<&str>,
) -> ExistingProject {
    ExistingProject {
        id: id.to_owned(),
        path: path.to_owned(),
        language: language.map(str::to_owned),
        framework: framework.map(str::to_owned),
        dependency_manager: dependency_manager.map(str::to_owned),
        venv: None,
    }
}

#[test]
fn parses_project_list() {
    let cli = Cli::try_parse_from(["workbench", "project", "list"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Project(ProjectCommands::List))
    ));
}

fn sample() -> [ExistingProject; 2] {
    [
        project(
            "billing-api",
            "services/billing",
            Some("python"),
            None,
            Some("uv"),
        ),
        project(
            "web",
            "../shared/web",
            Some("typescript"),
            Some("angular"),
            Some("pnpm"),
        ),
    ]
}

#[test]
fn prints_hint_when_there_are_no_projects() {
    assert_eq!(
        render(&[], false),
        ["No projects yet. Add one with `workbench project add <PATH>`."]
    );
}

#[test]
fn prints_projects_as_an_ascii_table() {
    assert_eq!(
        render(&sample(), false),
        [
            "+-------------+--------------------+--------------------+------------------+",
            "| ID          | LANGUAGE           | DEPENDENCY MANAGER | PATH             |",
            "+-------------+--------------------+--------------------+------------------+",
            "| billing-api | python             | uv                 | services/billing |",
            "| web         | typescript/angular | pnpm               | ../shared/web    |",
            "+-------------+--------------------+--------------------+------------------+",
            "2 projects",
        ]
    );
}

#[test]
fn shows_a_dash_for_fields_the_config_does_not_set() {
    let lines = render(&[project("legacy", "old", None, None, None)], false);
    assert_eq!(
        lines[3],
        "| legacy | -        | -                  | old  |"
    );
    assert_eq!(lines[5], "1 project");
}

#[test]
fn colours_cells_without_breaking_alignment() {
    let lines = render(&sample(), true);
    assert!(lines[3].contains("\u{1b}[33m python             \u{1b}[0m"));
    assert!(lines[4].contains("\u{1b}[34m typescript/angular \u{1b}[0m"));
    assert!(lines[1].contains("\u{1b}[1m ID          \u{1b}[0m"));

    let plain = render(&sample(), false);
    let strip = |line: &String| console::strip_ansi_codes(line).into_owned();
    assert_eq!(lines.iter().map(strip).collect::<Vec<_>>(), plain);
}
