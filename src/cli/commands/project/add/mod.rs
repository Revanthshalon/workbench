use std::{
    env, fs,
    path::{Path, PathBuf},
};

use clap::Args;
use console::style;
use yaml_serde::{Mapping, Value};

use crate::cli::{
    commands::project::kinds::{DependencyManager, Language, Stack, TypescriptFramework},
    errors::{CliErrors, CliResult},
    workspace::Workspace,
};

mod detect;
mod id;
#[cfg(test)]
mod tests;

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Project directory, relative to the current directory or absolute.
    pub path: PathBuf,
    /// Project id. Default: derived from the directory name.
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,
    /// Default: detected.
    #[arg(long, value_enum)]
    pub language: Option<Language>,
    /// Only with typescript. Default: detected (angular.json).
    #[arg(long, value_enum)]
    pub framework: Option<TypescriptFramework>,
    /// Default: detected.
    #[arg(long, value_enum)]
    pub dependency_manager: Option<DependencyManager>,
    /// Python only. Default: .venv if it exists in the project.
    #[arg(long, value_name = "PATH")]
    pub venv: Option<PathBuf>,
    /// Command prefix to launch tools in the project. Default: derived from the dependency manager.
    #[arg(long, value_name = "COMMAND")]
    pub activate: Option<String>,
    /// The project's Docker Compose file, relative to the project. Default: detected.
    #[arg(long, value_name = "PATH")]
    pub compose: Option<PathBuf>,
    /// Index the new project right after adding it.
    #[arg(long)]
    pub index: bool,
}

/// A project as it is written to the workspace config.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Project {
    id: String,
    path: String,
    stack: Stack,
    compose: Option<String>,
    activate: Option<String>,
}

pub fn run(args: AddArgs) -> CliResult<()> {
    let (workspace, project) = add_project(&args, &env::current_dir()?)?;

    println!(
        "Added project {} to {}\n",
        style(&project.id).green(),
        workspace.config_path().display()
    );
    for (label, value) in project.fields().into_iter().skip(1) {
        println!("  {}  {value}", style(format!("{label:<18}")).dim());
    }
    if args.index {
        eprintln!(
            "\nwarning: --index was ignored because `workbench index` is not available yet; \
             the project was added but not indexed."
        );
    }
    Ok(())
}

/// Adds the project to the workspace that contains `current_directory`.
fn add_project(args: &AddArgs, current_directory: &Path) -> CliResult<(Workspace, Project)> {
    let mut workspace = Workspace::find(current_directory)?;
    let requested_path = current_directory.join(&args.path);
    let project_path = fs::canonicalize(&requested_path)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or(CliErrors::ProjectPathNotFound(requested_path))?;

    let project = resolve(args, &workspace, &project_path)?;
    for existing in workspace.projects()? {
        if existing.id == project.id {
            return Err(CliErrors::DuplicateProjectId(project.id));
        }
        if existing.path == project.path {
            return Err(CliErrors::DuplicateProjectPath {
                path: project.path,
                id: existing.id,
            });
        }
    }

    workspace.append_project(&project.id, project.to_yaml())?;
    Ok((workspace, project))
}

/// Fills in every option the user left out.
fn resolve(args: &AddArgs, workspace: &Workspace, project_path: &Path) -> CliResult<Project> {
    let id = id::resolve(args.id.as_deref(), project_path)?;
    let stack = resolve_stack(args, project_path)?;
    let activate = args
        .activate
        .clone()
        .or_else(|| stack.activate_command().map(str::to_owned));
    let compose = path_option(args.compose.as_deref()).or_else(|| detect::compose(project_path));

    Ok(Project {
        id,
        path: workspace.relative_path(project_path),
        stack,
        compose,
        activate,
    })
}

/// Turns the flat command-line flags into a [`Stack`].
///
/// This is the only place where flags of the wrong language can meet, so it is the
/// only place that has to reject them: a `Stack` cannot hold such a combination.
fn resolve_stack(args: &AddArgs, project_path: &Path) -> CliResult<Stack> {
    let language = match (args.language, args.dependency_manager) {
        (Some(language), _) => language,
        (None, Some(manager)) => manager.language(),
        (None, None) => match (detect::language(project_path), args.framework) {
            // Nothing on disk says otherwise, and every framework is a typescript one.
            (Err(CliErrors::UndetectedOption { .. }), Some(_)) => Language::Typescript,
            (detected, _) => detected?,
        },
    };

    match language {
        Language::Python => {
            if args.framework.is_some() {
                return Err(wrong_language("--framework", Language::Typescript));
            }
            let dependency_manager = match args.dependency_manager {
                Some(DependencyManager::Python(manager)) => manager,
                Some(manager @ DependencyManager::Typescript(_)) => {
                    return Err(wrong_dependency_manager(manager, language));
                }
                None => detect::python_dependency_manager(project_path)?,
            };
            Ok(Stack::Python {
                dependency_manager,
                venv: path_option(args.venv.as_deref()).or_else(|| detect::venv(project_path)),
            })
        }
        Language::Typescript => {
            if args.venv.is_some() {
                return Err(wrong_language("--venv", Language::Python));
            }
            let dependency_manager = match args.dependency_manager {
                Some(DependencyManager::Typescript(manager)) => manager,
                Some(manager @ DependencyManager::Python(_)) => {
                    return Err(wrong_dependency_manager(manager, language));
                }
                None => detect::typescript_dependency_manager(project_path)?,
            };
            Ok(Stack::Typescript {
                dependency_manager,
                framework: args.framework.or_else(|| detect::framework(project_path)),
            })
        }
    }
}

fn wrong_language(flag: &str, required: Language) -> CliErrors {
    CliErrors::InvalidOptionCombination(format!(
        "{flag} can only be used with {}",
        required.as_str()
    ))
}

fn wrong_dependency_manager(manager: DependencyManager, language: Language) -> CliErrors {
    CliErrors::InvalidOptionCombination(format!(
        "--dependency-manager {} cannot be used with {}",
        manager.as_str(),
        language.as_str()
    ))
}

fn path_option(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.to_string_lossy().into_owned())
}

impl Project {
    /// The fields that are set, in the order they are written and shown.
    fn fields(&self) -> Vec<(&'static str, &str)> {
        [
            ("id", Some(self.id.as_str())),
            ("path", Some(self.path.as_str())),
            ("language", Some(self.stack.language().as_str())),
            (
                "framework",
                self.stack.framework().map(TypescriptFramework::as_str),
            ),
            (
                "dependency_manager",
                Some(self.stack.dependency_manager().as_str()),
            ),
            ("compose", self.compose.as_deref()),
            ("venv", self.stack.venv()),
            ("activate", self.activate.as_deref()),
        ]
        .into_iter()
        .filter_map(|(label, value)| Some((label, value?)))
        .collect()
    }

    /// The project's entry in the `projects` list of the workspace config.
    fn to_yaml(&self) -> Mapping {
        let mut entry = Mapping::new();
        let mut environment = Mapping::new();
        for (key, value) in self.fields() {
            let target = match key {
                "venv" | "activate" => &mut environment,
                _ => &mut entry,
            };
            target.insert(Value::from(key), Value::from(value));
        }
        if !environment.is_empty() {
            entry.insert(Value::from("environment"), Value::Mapping(environment));
        }
        entry
    }
}
