use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use clap::Parser;

use super::{AddArgs, add_project};
use crate::cli::{
    Cli, Commands,
    commands::project::{
        ProjectCommands,
        kinds::{
            DependencyManager, Language, PythonDependencyManager, TypescriptDependencyManager,
            TypescriptFramework,
        },
    },
    errors::{CliErrors, CliResult},
};

const EMPTY_CONFIG: &str = "# .workbench/config.yml\nversion: 1\nprojects: []\n";

/// A scratch workspace that is removed when dropped.
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str, config: &str) -> CliResult<Self> {
        let workspace =
            Self(env::temp_dir().join(format!("workbench-add-{name}-{}", process::id())));
        workspace.write(".workbench/config.yml", config)?;
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

    fn add(&self, args: &AddArgs) -> CliResult<()> {
        add_project(args, &self.0).map(|_| ())
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args(path: &str) -> AddArgs {
    AddArgs {
        path: PathBuf::from(path),
        id: None,
        language: None,
        framework: None,
        dependency_manager: None,
        venv: None,
        activate: None,
        compose: None,
        index: false,
    }
}

#[test]
fn parses_every_option() -> Result<(), clap::Error> {
    let cli = Cli::try_parse_from([
        "workbench",
        "project",
        "add",
        "services/web",
        "--id",
        "web",
        "--language",
        "typescript",
        "--framework",
        "angular",
        "--dependency-manager",
        "pnpm",
        "--activate",
        "pnpm exec",
        "--compose",
        "compose.yml",
        "--index",
    ])?;
    let Commands::Project(ProjectCommands::Add(parsed)) = cli.command else {
        return Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand));
    };
    assert_eq!(parsed.path, Path::new("services/web"));
    assert_eq!(parsed.id.as_deref(), Some("web"));
    assert_eq!(parsed.language, Some(Language::Typescript));
    assert_eq!(parsed.framework, Some(TypescriptFramework::Angular));
    assert_eq!(
        parsed.dependency_manager,
        Some(DependencyManager::Typescript(
            TypescriptDependencyManager::Pnpm
        ))
    );
    assert_eq!(parsed.activate.as_deref(), Some("pnpm exec"));
    assert_eq!(parsed.compose.as_deref(), Some(Path::new("compose.yml")));
    assert!(parsed.index);
    Ok(())
}

#[test]
fn language_is_inferred_from_explicit_dependency_manager_or_framework() -> CliResult<()> {
    let workspace = Workspace::new("inferred", EMPTY_CONFIG)?;
    workspace.write("api/README.md", "")?;
    workspace.write("web/README.md", "")?;
    workspace.write("web/yarn.lock", "")?;

    workspace.add(&AddArgs {
        dependency_manager: Some(DependencyManager::Python(PythonDependencyManager::Poetry)),
        ..args("api")
    })?;
    workspace.add(&AddArgs {
        framework: Some(TypescriptFramework::Angular),
        ..args("web")
    })?;

    let config = workspace.config()?;
    assert!(config.contains("    language: python\n    dependency_manager: poetry\n"));
    assert!(config.contains(
        "    language: typescript\n    framework: angular\n    dependency_manager: yarn\n"
    ));
    Ok(())
}

#[test]
fn rejects_unknown_language() {
    assert!(Cli::try_parse_from(["workbench", "project", "add", ".", "--language", "go"]).is_err());
}

#[test]
fn adds_detected_python_project() -> CliResult<()> {
    let workspace = Workspace::new("python", EMPTY_CONFIG)?;
    workspace.write("services/Billing_API/pyproject.toml", "")?;
    workspace.write("services/Billing_API/uv.lock", "")?;
    workspace.write("services/Billing_API/docker-compose.yml", "")?;
    workspace.write("services/Billing_API/.venv/pyvenv.cfg", "")?;

    workspace.add(&args("services/Billing_API"))?;

    assert_eq!(
        workspace.config()?,
        "# .workbench/config.yml
version: 1
projects:
  - id: billing-api
    path: services/Billing_API
    language: python
    dependency_manager: uv
    compose: docker-compose.yml
    environment:
      venv: .venv
      activate: uv run
"
    );
    Ok(())
}

#[test]
fn adds_detected_angular_project_after_existing_one() -> CliResult<()> {
    let config = "version: 1
projects:
  - id: billing-api   # the backend
    path: services/billing

# Compose files that span projects
compose: []
";
    let workspace = Workspace::new("angular", config)?;
    workspace.write("web/package.json", "{}")?;
    workspace.write("web/angular.json", "{}")?;
    workspace.write("web/pnpm-lock.yaml", "")?;

    workspace.add(&args("web"))?;

    assert_eq!(
        workspace.config()?,
        "version: 1
projects:
  - id: billing-api   # the backend
    path: services/billing
  - id: web
    path: web
    language: typescript
    framework: angular
    dependency_manager: pnpm

# Compose files that span projects
compose: []
"
    );
    Ok(())
}

#[test]
fn explicit_options_override_detection() -> CliResult<()> {
    let workspace = Workspace::new("explicit", EMPTY_CONFIG)?;
    workspace.write("api/pyproject.toml", "[tool.poetry]\nname = \"api\"\n")?;

    workspace.add(&AddArgs {
        id: Some("backend".to_owned()),
        venv: Some(PathBuf::from("env")),
        activate: Some("poetry run --quiet".to_owned()),
        compose: Some(PathBuf::from("deploy/compose.yml")),
        ..args("api")
    })?;

    let config = workspace.config()?;
    assert!(config.contains("  - id: backend\n    path: api\n"));
    assert!(config.contains("    dependency_manager: poetry\n    compose: deploy/compose.yml\n"));
    assert!(config.contains("      venv: env\n      activate: poetry run --quiet\n"));
    Ok(())
}

#[test]
fn stores_path_outside_workspace_relative_to_root() -> CliResult<()> {
    let workspace = Workspace::new("outside", EMPTY_CONFIG)?;
    workspace.write("root/.workbench/config.yml", EMPTY_CONFIG)?;
    workspace.write("shared/web/package.json", "{}")?;
    workspace.write("shared/web/yarn.lock", "")?;

    add_project(&args("../shared/web"), &workspace.0.join("root"))?;

    let config = fs::read_to_string(workspace.0.join("root/.workbench/config.yml"))?;
    assert!(config.contains("  - id: web\n    path: ../shared/web\n"));
    Ok(())
}

#[test]
fn derived_id_that_is_invalid_names_the_id_flag() -> CliResult<()> {
    let workspace = Workspace::new("invalid-id", EMPTY_CONFIG)?;
    workspace.write("3d-viewer/package.json", "{}")?;
    workspace.write("3d-viewer/package-lock.json", "{}")?;

    let error = workspace.add(&args("3d-viewer"));

    assert!(matches!(
        &error,
        Err(CliErrors::InvalidProjectId { id, derived: true }) if id == "3d-viewer"
    ));
    assert!(error.is_err_and(|error| error.to_string().contains("--id")));
    assert_eq!(workspace.config()?, EMPTY_CONFIG);

    workspace.add(&AddArgs {
        id: Some("viewer-3d".to_owned()),
        ..args("3d-viewer")
    })?;
    assert!(workspace.config()?.contains("  - id: viewer-3d\n"));
    Ok(())
}

#[test]
fn rejects_duplicate_id_and_path() -> CliResult<()> {
    let workspace = Workspace::new("duplicate", EMPTY_CONFIG)?;
    for project in ["api", "other"] {
        workspace.write(&format!("{project}/pyproject.toml"), "")?;
        workspace.write(&format!("{project}/uv.lock"), "")?;
    }
    workspace.add(&args("api"))?;

    assert!(matches!(
        workspace.add(&AddArgs {
            id: Some("api".to_owned()),
            ..args("other")
        }),
        Err(CliErrors::DuplicateProjectId(_))
    ));
    assert!(matches!(
        workspace.add(&AddArgs {
            id: Some("api-again".to_owned()),
            ..args("api")
        }),
        Err(CliErrors::DuplicateProjectPath { .. })
    ));
    Ok(())
}

#[test]
fn rejects_options_that_do_not_fit_the_language() -> CliResult<()> {
    let workspace = Workspace::new("combination", EMPTY_CONFIG)?;
    workspace.write("api/pyproject.toml", "")?;
    workspace.write("api/uv.lock", "")?;
    workspace.write("web/package.json", "{}")?;
    workspace.write("web/package-lock.json", "{}")?;

    let rejected = [
        AddArgs {
            framework: Some(TypescriptFramework::Angular),
            language: Some(Language::Python),
            ..args("api")
        },
        AddArgs {
            framework: Some(TypescriptFramework::Angular),
            ..args("api")
        },
        AddArgs {
            dependency_manager: Some(DependencyManager::Typescript(
                TypescriptDependencyManager::Npm,
            )),
            language: Some(Language::Python),
            ..args("api")
        },
        AddArgs {
            venv: Some(PathBuf::from(".venv")),
            ..args("web")
        },
        AddArgs {
            dependency_manager: Some(DependencyManager::Python(PythonDependencyManager::Uv)),
            language: Some(Language::Typescript),
            ..args("web")
        },
    ];
    for arguments in &rejected {
        assert!(matches!(
            workspace.add(arguments),
            Err(CliErrors::InvalidOptionCombination(_))
        ));
    }
    assert_eq!(workspace.config()?, EMPTY_CONFIG);
    Ok(())
}

#[test]
fn fails_when_nothing_can_be_detected() -> CliResult<()> {
    let workspace = Workspace::new("undetected", EMPTY_CONFIG)?;
    workspace.write("empty/README.md", "")?;
    workspace.write("no-lock/package.json", "{}")?;
    workspace.write("mixed/package.json", "{}")?;
    workspace.write("mixed/pyproject.toml", "")?;

    assert!(matches!(
        workspace.add(&args("empty")),
        Err(CliErrors::UndetectedOption {
            flag: "--language",
            ..
        })
    ));
    assert!(matches!(
        workspace.add(&args("no-lock")),
        Err(CliErrors::UndetectedOption {
            flag: "--dependency-manager",
            ..
        })
    ));
    assert!(matches!(
        workspace.add(&args("mixed")),
        Err(CliErrors::AmbiguousLanguage(_))
    ));
    assert!(matches!(
        workspace.add(&args("missing")),
        Err(CliErrors::ProjectPathNotFound(_))
    ));
    Ok(())
}

#[test]
fn fails_outside_a_workspace() -> CliResult<()> {
    let directory = env::temp_dir().join(format!("workbench-add-none-{}", process::id()));
    fs::create_dir_all(directory.join("api"))?;
    let result = add_project(&args("api"), &directory);
    fs::remove_dir_all(&directory)?;

    assert!(matches!(result, Err(CliErrors::WorkspaceNotFound(_))));
    Ok(())
}
