use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use clap::Parser;

use super::{InitArgs, run};
use crate::cli::{Cli, Commands, errors::CliResult};

/// A scratch workspace that is removed when dropped.
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        Self(env::temp_dir().join(format!("Workbench Test_{name}-{}", process::id())))
    }

    fn init(&self) -> CliResult<()> {
        self.init_with(false)
    }

    fn init_with(&self, force: bool) -> CliResult<()> {
        run(InitArgs {
            out: self.0.clone(),
            force,
        })
    }

    fn write(&self, relative: &str, contents: &str) -> CliResult<()> {
        Ok(fs::write(self.0.join(relative), contents)?)
    }

    fn read(&self, relative: &str) -> CliResult<String> {
        Ok(fs::read_to_string(self.0.join(relative))?)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parses_init_with_out() {
    let cli = Cli::try_parse_from(["workbench", "init", "--out", "some/dir"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Init(InitArgs { out, force: false })) if out == Path::new("some/dir")
    ));
}

#[test]
fn parses_init_with_default_out() {
    let cli = Cli::try_parse_from(["workbench", "init"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Init(InitArgs { out, force: false })) if out == Path::new(".")
    ));
}

#[test]
fn creates_workbench_layout_in_fresh_directory() -> CliResult<()> {
    let workspace = Workspace::new("fresh");
    workspace.init()?;

    let base_path = workspace.0.join(".workbench");
    assert!(base_path.join("skills").is_dir());
    assert!(base_path.join("memory/INIT-MEMORY.md").is_file());
    assert!(base_path.join("mcp/config.yml").is_file());
    for context in ["production", "staging", "testing"] {
        let contents = workspace.read(&format!(".workbench/mcp/contexts/{context}.yml"))?;
        assert!(contents.contains(context));
    }
    assert_eq!(workspace.read(".gitignore")?, ".workbench/\n");
    Ok(())
}

#[test]
fn writes_config_with_unique_namespace() -> CliResult<()> {
    let workspace = Workspace::new("config");
    workspace.init()?;

    let config = workspace.read(".workbench/config.yml")?;
    assert!(config.contains("version: 1\n"));
    assert!(config.contains("  host: localhost\n  port: 7687\n"));
    assert!(config.ends_with("projects: []\n"));

    let namespace = config
        .lines()
        .find_map(|line| line.trim().strip_prefix("namespace: "))
        .unwrap_or_default();
    let (prefix, suffix) = namespace.rsplit_once('-').unwrap_or_default();
    assert!(prefix.starts_with("workbench-test-config-"));
    assert_eq!(suffix.len(), 4);
    assert!(
        suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    Ok(())
}

#[test]
fn appends_to_existing_gitignore_once() -> CliResult<()> {
    let workspace = Workspace::new("gitignore");
    fs::create_dir_all(&workspace.0)?;
    fs::write(workspace.0.join(".gitignore"), "target")?;
    workspace.init()?;
    assert_eq!(workspace.read(".gitignore")?, "target\n.workbench/\n");

    workspace.init()?;
    assert_eq!(workspace.read(".gitignore")?, "target\n.workbench/\n");
    Ok(())
}

#[test]
fn parses_init_with_force() {
    let cli = Cli::try_parse_from(["workbench", "init", "--force"]);
    assert!(matches!(
        cli.map(|cli| cli.command),
        Ok(Commands::Init(InitArgs { force: true, .. }))
    ));
}

#[test]
fn rerun_keeps_existing_files_and_restores_missing_ones() -> CliResult<()> {
    let workspace = Workspace::new("rerun");
    workspace.init()?;
    workspace.write(".workbench/config.yml", "version: 1\nprojects: [edited]\n")?;
    workspace.write(".workbench/memory/INIT-MEMORY.md", "my notes")?;
    fs::remove_file(workspace.0.join(".workbench/mcp/contexts/staging.yml"))?;
    fs::remove_dir(workspace.0.join(".workbench/skills"))?;

    workspace.init()?;

    assert_eq!(
        workspace.read(".workbench/config.yml")?,
        "version: 1\nprojects: [edited]\n"
    );
    assert_eq!(
        workspace.read(".workbench/memory/INIT-MEMORY.md")?,
        "my notes"
    );
    assert!(
        workspace
            .0
            .join(".workbench/mcp/contexts/staging.yml")
            .is_file()
    );
    assert!(workspace.0.join(".workbench/skills").is_dir());
    Ok(())
}

#[test]
fn force_rewrites_config_files_to_defaults_keeping_namespace() -> CliResult<()> {
    let workspace = Workspace::new("force");
    workspace.init()?;
    let defaults = workspace.read(".workbench/config.yml")?;
    workspace.write(
        ".workbench/config.yml",
        &defaults
            .replace("port: 7687", "port: 9999")
            .replace("projects: []", "projects: [edited]"),
    )?;
    workspace.write(".workbench/memory/INIT-MEMORY.md", "my notes")?;
    let mcp_defaults = workspace.read(".workbench/mcp/config.yml")?;
    let staging_defaults = workspace.read(".workbench/mcp/contexts/staging.yml")?;
    workspace.write(".workbench/mcp/config.yml", "edited")?;
    workspace.write(".workbench/mcp/contexts/staging.yml", "edited")?;

    workspace.init_with(true)?;

    assert_eq!(workspace.read(".workbench/config.yml")?, defaults);
    assert_eq!(workspace.read(".workbench/mcp/config.yml")?, mcp_defaults);
    assert_eq!(
        workspace.read(".workbench/mcp/contexts/staging.yml")?,
        staging_defaults
    );
    assert_eq!(
        workspace.read(".workbench/memory/INIT-MEMORY.md")?,
        "my notes"
    );
    Ok(())
}

#[test]
fn force_generates_namespace_when_existing_config_has_none() -> CliResult<()> {
    let workspace = Workspace::new("force-broken");
    workspace.init()?;
    workspace.write(".workbench/config.yml", "garbage")?;

    workspace.init_with(true)?;

    let config = workspace.read(".workbench/config.yml")?;
    assert!(config.contains("  namespace: workbench-test-force-broken-"));
    assert!(config.ends_with("projects: []\n"));
    Ok(())
}
