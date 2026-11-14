use std::{env, fs, path::PathBuf, process};

use clap::Parser;

use super::{CleanupArgs, execute};
use crate::cli::{
    Cli, Commands, commands::project::ProjectCommands, errors::CliResult, prompt::testing::Recorder,
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
            Self(env::temp_dir().join(format!("workbench-cleanup-{name}-{}", process::id())));
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

    fn cleanup(&self, args: &CleanupArgs, prompt: &mut Recorder) -> CliResult<()> {
        execute(args, &self.0, false, prompt)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parses_cleanup() {
    let cli = Cli::try_parse_from(["workbench", "project", "cleanup", "--yes"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Project(ProjectCommands::Cleanup(CleanupArgs {
            yes: true
        })))
    ));
}

#[test]
fn deletes_only_orphaned_folders() -> CliResult<()> {
    let workspace = Workspace::new("orphans", CONFIG)?;
    workspace.write(".workbench/memory/INIT-MEMORY.md", "workspace notes")?;
    workspace.write(".workbench/memory/api/INIT-MEMORY.md", "")?;
    workspace.write(".workbench/memory/old-api/INIT-MEMORY.md", "old notes")?;
    workspace.write(".workbench/memory/old-api/annotations.yml", "")?;
    fs::create_dir_all(workspace.0.join(".workbench/memory/empty"))?;
    let mut prompt = Recorder::answering(Some(true));

    workspace.cleanup(&CleanupArgs { yes: false }, &mut prompt)?;

    assert_eq!(prompt.questions, ["Delete 2 orphaned memory folders?"]);
    assert_eq!(
        prompt.lines[..7],
        [
            "Will delete 2 orphaned memory folders that no project in the config uses",
            "+----------------------------+-------+------+",
            "| FOLDER                     | FILES | SIZE |",
            "+----------------------------+-------+------+",
            "| .workbench/memory/empty/   | 0     | 0 B  |",
            "| .workbench/memory/old-api/ | 2     | 9 B  |",
            "+----------------------------+-------+------+",
        ]
    );
    assert_eq!(
        prompt.lines[8..],
        [
            "Deleted 2 orphaned memory folders",
            "",
            "  - Deleted .workbench/memory/empty/",
            "  - Deleted .workbench/memory/old-api/",
        ]
    );
    assert!(!workspace.exists(".workbench/memory/old-api"));
    assert!(!workspace.exists(".workbench/memory/empty"));
    assert!(workspace.exists(".workbench/memory/api/INIT-MEMORY.md"));
    assert!(workspace.exists(".workbench/memory/web/INIT-MEMORY.md"));
    assert!(workspace.exists(".workbench/memory/INIT-MEMORY.md"));
    assert_eq!(workspace.config()?, CONFIG);
    Ok(())
}

#[test]
fn reports_when_nothing_is_orphaned() -> CliResult<()> {
    let workspace = Workspace::new("no-orphans", CONFIG)?;
    let mut prompt = Recorder::answering(None);

    workspace.cleanup(&CleanupArgs { yes: false }, &mut prompt)?;

    assert!(prompt.questions.is_empty());
    assert_eq!(
        prompt.lines,
        ["No orphaned memory folders in .workbench/memory/"]
    );
    assert!(workspace.exists(".workbench/memory/web"));
    Ok(())
}

#[test]
fn declining_deletes_nothing() -> CliResult<()> {
    let workspace = Workspace::new("orphans-decline", CONFIG)?;
    workspace.write(".workbench/memory/old-api/INIT-MEMORY.md", "")?;
    let mut prompt = Recorder::answering(Some(false));

    workspace.cleanup(&CleanupArgs { yes: false }, &mut prompt)?;

    assert_eq!(
        prompt.lines.last().map(String::as_str),
        Some("Aborted. Nothing was changed.")
    );
    assert!(workspace.exists(".workbench/memory/old-api/INIT-MEMORY.md"));
    Ok(())
}

#[test]
fn yes_skips_the_confirmation() -> CliResult<()> {
    let workspace = Workspace::new("orphans-yes", CONFIG)?;
    workspace.write(".workbench/memory/old-api/INIT-MEMORY.md", "")?;
    let mut prompt = Recorder::answering(None);

    workspace.cleanup(&CleanupArgs { yes: true }, &mut prompt)?;

    assert!(prompt.questions.is_empty());
    assert!(!workspace.exists(".workbench/memory/old-api"));
    Ok(())
}
