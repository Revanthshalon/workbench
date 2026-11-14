use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use clap::Parser;

use super::{RemoveArgs, execute};
use crate::cli::{
    Cli, Commands,
    commands::project::ProjectCommands,
    errors::{CliErrors, CliResult},
    prompt::testing::Recorder,
};

const CONFIG: &str = "version: 1
projects:
  - id: api   # the backend
    path: services/api
    language: python
  - id: web
    path: web
    language: typescript

# Compose files that span projects
compose: []
";

/// A scratch workspace that is removed when dropped.
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str, config: &str) -> CliResult<Self> {
        let workspace =
            Self(env::temp_dir().join(format!("workbench-remove-{name}-{}", process::id())));
        workspace.write(".workbench/config.yml", config)?;
        workspace.write(".workbench/memory/web/INIT-MEMORY.md", "web notes")?;
        workspace.write(".workbench/memory/web/annotations.yml", "")?;
        Ok(workspace)
    }

    fn write(&self, relative: &str, contents: &str) -> CliResult<()> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(fs::write(path, contents)?)
    }

    fn config(&self) -> CliResult<String> {
        Ok(fs::read_to_string(self.0.join(".workbench/config.yml"))?)
    }

    fn exists(&self, relative: &str) -> bool {
        self.0.join(relative).exists()
    }

    fn remove(&self, args: &RemoveArgs, prompt: &mut Recorder) -> CliResult<()> {
        execute(args, &self.0, false, prompt)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args(id: &str) -> RemoveArgs {
    RemoveArgs {
        id: id.to_owned(),
        purge_memory: false,
        yes: false,
    }
}

fn purge(id: &str) -> RemoveArgs {
    RemoveArgs {
        purge_memory: true,
        ..args(id)
    }
}

#[test]
fn parses_remove_with_flags() {
    let cli = Cli::try_parse_from([
        "workbench",
        "project",
        "remove",
        "web",
        "--purge-memory",
        "-y",
    ]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Project(ProjectCommands::Remove(RemoveArgs { id, purge_memory: true, yes: true })))
            if id == "web"
    ));
}

#[test]
fn removes_project_and_keeps_its_memory() -> CliResult<()> {
    let workspace = Workspace::new("keep", CONFIG)?;
    let mut prompt = Recorder::answering(None);

    workspace.remove(&args("web"), &mut prompt)?;

    assert_eq!(
        workspace.config()?,
        "version: 1
projects:
  - id: api   # the backend
    path: services/api
    language: python

# Compose files that span projects
compose: []
"
    );
    assert!(workspace.exists(".workbench/memory/web/INIT-MEMORY.md"));
    assert!(workspace.exists(".workbench/memory/web/annotations.yml"));
    assert!(prompt.questions.is_empty());
    assert_eq!(
        prompt.lines,
        [
            "Removed project web from .workbench/config.yml",
            "",
            "  - Kept .workbench/memory/web/ (adding `web` again finds its notes)",
            "  - Indexed data is removed by the next `workbench index`",
        ]
    );
    Ok(())
}

#[test]
fn summary_colours_each_part_of_a_bullet() -> CliResult<()> {
    let workspace = Workspace::new("colours", CONFIG)?;
    let found = crate::cli::workspace::Workspace::find(&workspace.0)?;

    let lines = super::summary(
        "web",
        &found,
        ".workbench/memory/web/",
        super::MemoryState::Kept,
        true,
    );

    assert!(lines[2].starts_with("  \u{1b}[36m\u{1b}[1m-\u{1b}[0m \u{1b}[32mKept\u{1b}[0m"));
    assert!(lines[2].contains(" .workbench/memory/web/ "));
    assert!(lines[3].contains("\u{1b}[33mIndexed data\u{1b}[0m"));
    Ok(())
}

#[test]
fn removing_the_last_project_leaves_an_empty_list() -> CliResult<()> {
    let workspace = Workspace::new(
        "last",
        "version: 1\nprojects:\n  - id: web\n    path: web\n",
    )?;

    workspace.remove(&args("web"), &mut Recorder::answering(None))?;

    assert_eq!(workspace.config()?, "version: 1\nprojects: []\n");
    Ok(())
}

#[test]
fn unknown_id_fails_and_lists_known_ids() -> CliResult<()> {
    let workspace = Workspace::new("unknown", CONFIG)?;

    let error = workspace.remove(&args("billing"), &mut Recorder::answering(None));

    assert!(error.is_err_and(|error| {
        error.to_string() == "No project with the id `billing`. \nKnown ids:\n  - api\n  - web"
    }));
    assert_eq!(workspace.config()?, CONFIG);
    Ok(())
}

#[test]
fn unknown_id_error_is_coloured_for_the_terminal() {
    let error = CliErrors::UnknownProjectId {
        id: "billing".to_owned(),
        known: vec!["api".to_owned(), "web".to_owned()],
    };

    let lines: Vec<String> = error.render(true).lines().map(str::to_owned).collect();

    assert_eq!(
        lines[0],
        "No project with the id \u{1b}[36m`billing`\u{1b}[0m. "
    );
    assert_eq!(lines[1], "Known ids:");
    assert_eq!(
        lines[2],
        "  \u{1b}[36m\u{1b}[1m-\u{1b}[0m \u{1b}[36m\u{1b}[1mapi\u{1b}[0m"
    );
    assert_eq!(error.render(false), error.to_string());
}

#[test]
fn unknown_id_in_an_empty_workspace_says_so() -> CliResult<()> {
    let workspace = Workspace::new("unknown-empty", "version: 1\nprojects: []\n")?;

    let error = workspace.remove(&args("web"), &mut Recorder::answering(None));

    assert!(error.is_err_and(|error| {
        error
            .to_string()
            .ends_with("The workspace has no projects.")
    }));
    Ok(())
}

#[test]
fn purge_lists_files_then_deletes_after_confirmation() -> CliResult<()> {
    let workspace = Workspace::new("purge", CONFIG)?;
    let mut prompt = Recorder::answering(Some(true));

    workspace.remove(&purge("web"), &mut prompt)?;

    assert_eq!(prompt.questions, ["Delete the memory of `web`?"]);
    assert_eq!(
        prompt.lines[..7],
        [
            "--purge-memory will delete .workbench/memory/web/",
            "+---------------------------------------+------+",
            "| FILE                                  | SIZE |",
            "+---------------------------------------+------+",
            "| .workbench/memory/web/INIT-MEMORY.md  | 9 B  |",
            "| .workbench/memory/web/annotations.yml | 0 B  |",
            "+---------------------------------------+------+",
        ]
    );
    assert_eq!(prompt.lines[7], "");
    assert_eq!(prompt.lines[10], "  - Deleted .workbench/memory/web/");
    assert!(!workspace.exists(".workbench/memory/web"));
    assert!(!workspace.config()?.contains("id: web"));
    Ok(())
}

#[test]
fn declining_the_purge_changes_nothing() -> CliResult<()> {
    let workspace = Workspace::new("decline", CONFIG)?;
    let mut prompt = Recorder::answering(Some(false));

    workspace.remove(&purge("web"), &mut prompt)?;

    assert_eq!(
        prompt.lines.last().map(String::as_str),
        Some("Aborted. Nothing was changed.")
    );
    assert_eq!(workspace.config()?, CONFIG);
    assert!(workspace.exists(".workbench/memory/web/INIT-MEMORY.md"));
    Ok(())
}

#[test]
fn failed_confirmation_changes_nothing() -> CliResult<()> {
    let workspace = Workspace::new("no-terminal", CONFIG)?;

    let result = workspace.remove(&purge("web"), &mut Recorder::answering(None));

    assert!(matches!(result, Err(CliErrors::ConfirmationFailed(_))));
    assert_eq!(workspace.config()?, CONFIG);
    assert!(workspace.exists(".workbench/memory/web"));
    Ok(())
}

#[test]
fn yes_skips_the_confirmation() -> CliResult<()> {
    let workspace = Workspace::new("yes", CONFIG)?;
    let mut prompt = Recorder::answering(None);

    workspace.remove(
        &RemoveArgs {
            yes: true,
            ..purge("web")
        },
        &mut prompt,
    )?;

    assert!(prompt.questions.is_empty());
    assert!(!workspace.exists(".workbench/memory/web"));
    Ok(())
}

#[test]
fn purge_without_a_memory_folder_does_not_ask() -> CliResult<()> {
    let workspace = Workspace::new("purge-missing", CONFIG)?;
    let mut prompt = Recorder::answering(None);

    workspace.remove(&purge("api"), &mut prompt)?;

    assert!(prompt.questions.is_empty());
    assert_eq!(
        prompt.lines[2],
        "  - No memory folder at .workbench/memory/api/"
    );
    assert!(workspace.exists(".workbench/memory/web"));
    Ok(())
}

#[test]
fn purge_refuses_an_id_that_is_a_path() -> CliResult<()> {
    let config = "version: 1\nprojects:\n  - id: ../web\n    path: web\n";
    let workspace = Workspace::new("unsafe", config)?;
    workspace.write(".workbench/memory/../web/x", "")?;

    let result = workspace.remove(
        &RemoveArgs {
            yes: true,
            ..purge("../web")
        },
        &mut Recorder::answering(None),
    );

    assert!(matches!(result, Err(CliErrors::UnsafeMemoryPath(_))));
    assert_eq!(workspace.config()?, config);
    assert!(Path::new(&workspace.0.join(".workbench/web/x")).exists());
    Ok(())
}
