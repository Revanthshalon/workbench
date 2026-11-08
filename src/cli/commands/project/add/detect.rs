use std::{fs, path::Path};

use crate::cli::{
    commands::project::kinds::{
        Language, PythonDependencyManager, TypescriptDependencyManager, TypescriptFramework,
    },
    constants::{
        ANGULAR_MARKER, COMPOSE_FILES, DEFAULT_VENV, PYPROJECT_FILE, PYTHON_LOCKFILES,
        PYTHON_MARKERS, PYTHON_PYPROJECT_TABLES, TYPESCRIPT_LOCKFILES, TYPESCRIPT_MARKERS,
    },
    errors::{CliErrors, CliResult},
};

fn has_any(project_path: &Path, files: &[&str]) -> bool {
    files.iter().any(|file| project_path.join(file).is_file())
}

pub(super) fn language(project_path: &Path) -> CliResult<Language> {
    let is_python = has_any(project_path, &PYTHON_MARKERS);
    let is_typescript = has_any(project_path, &TYPESCRIPT_MARKERS);
    match (is_python, is_typescript) {
        (true, false) => Ok(Language::Python),
        (false, true) => Ok(Language::Typescript),
        (true, true) => Err(CliErrors::AmbiguousLanguage(project_path.to_path_buf())),
        (false, false) => Err(CliErrors::UndetectedOption {
            what: "language",
            flag: "--language",
            path: project_path.to_path_buf(),
        }),
    }
}

pub(super) fn framework(project_path: &Path) -> Option<TypescriptFramework> {
    project_path
        .join(ANGULAR_MARKER)
        .is_file()
        .then_some(TypescriptFramework::Angular)
}

pub(super) fn python_dependency_manager(project_path: &Path) -> CliResult<PythonDependencyManager> {
    from_lockfile(project_path, &PYTHON_LOCKFILES)
        .or_else(|| from_pyproject(project_path))
        .ok_or_else(|| undetected_dependency_manager(project_path))
}

pub(super) fn typescript_dependency_manager(
    project_path: &Path,
) -> CliResult<TypescriptDependencyManager> {
    from_lockfile(project_path, &TYPESCRIPT_LOCKFILES)
        .ok_or_else(|| undetected_dependency_manager(project_path))
}

fn from_lockfile<Manager: Copy>(
    project_path: &Path,
    lockfiles: &[(&str, Manager)],
) -> Option<Manager> {
    lockfiles
        .iter()
        .find(|(file, _)| project_path.join(file).is_file())
        .map(|(_, manager)| *manager)
}

/// Falls back to the tool tables in `pyproject.toml` when there is no lockfile yet.
fn from_pyproject(project_path: &Path) -> Option<PythonDependencyManager> {
    let pyproject = fs::read_to_string(project_path.join(PYPROJECT_FILE)).ok()?;
    PYTHON_PYPROJECT_TABLES
        .iter()
        .find(|(table, _)| pyproject.lines().any(|line| line.trim().starts_with(table)))
        .map(|(_, manager)| *manager)
}

fn undetected_dependency_manager(project_path: &Path) -> CliErrors {
    CliErrors::UndetectedOption {
        what: "dependency manager",
        flag: "--dependency-manager",
        path: project_path.to_path_buf(),
    }
}

pub(super) fn venv(project_path: &Path) -> Option<String> {
    project_path
        .join(DEFAULT_VENV)
        .is_dir()
        .then(|| DEFAULT_VENV.to_owned())
}

pub(super) fn compose(project_path: &Path) -> Option<String> {
    COMPOSE_FILES
        .iter()
        .find(|file| project_path.join(file).is_file())
        .map(|file| (*file).to_owned())
}
