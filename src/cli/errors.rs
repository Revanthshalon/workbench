use std::error::Error;

use console::Style;

use crate::cli::table;
use std::fmt::Display;
use std::path::PathBuf;

pub type CliResult<T> = Result<T, CliErrors>;

#[derive(Debug)]
pub enum CliErrors {
    DirectoryError(std::io::Error),
    WorkspaceNotFound(PathBuf),
    InvalidWorkspaceConfig {
        path: PathBuf,
        reason: String,
    },
    ProjectPathNotFound(PathBuf),
    InvalidProjectId {
        id: String,
        derived: bool,
    },
    DuplicateProjectId(String),
    DuplicateProjectPath {
        path: String,
        id: String,
    },
    UndetectedOption {
        what: &'static str,
        flag: &'static str,
        path: PathBuf,
    },
    AmbiguousLanguage(PathBuf),
    InvalidOptionCombination(String),
    UnknownProjectId {
        id: String,
        known: Vec<String>,
    },
    UnsafeMemoryPath(String),
    ConfirmationFailed(String),
}

impl Display for CliErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliErrors::DirectoryError(error) => {
                write!(f, "Unable to create directory. {error}")
            }
            CliErrors::WorkspaceNotFound(start) => write!(
                f,
                "No workspace found in {} or its parent directories. Run `workbench init` first.",
                start.display()
            ),
            CliErrors::InvalidWorkspaceConfig { path, reason } => {
                write!(f, "Invalid workspace config {}: {reason}.", path.display())
            }
            CliErrors::ProjectPathNotFound(path) => {
                write!(f, "Project path {} is not a directory.", path.display())
            }
            CliErrors::InvalidProjectId { id, derived: true } => write!(
                f,
                "The id `{id}` derived from the directory name is not a valid project id. \
                 Pass one with --id (kebab-case, starting with a letter)."
            ),
            CliErrors::InvalidProjectId { id, derived: false } => write!(
                f,
                "`{id}` is not a valid project id. \
                 Pass a kebab-case --id that starts with a letter."
            ),
            CliErrors::DuplicateProjectId(id) => write!(
                f,
                "A project with the id `{id}` already exists. Pass a different one with --id."
            ),
            CliErrors::DuplicateProjectPath { path, id } => {
                write!(f, "The path {path} is already added as project `{id}`.")
            }
            CliErrors::UndetectedOption { what, flag, path } => write!(
                f,
                "Could not detect the {what} of {}. Pass it with {flag}.",
                path.display()
            ),
            CliErrors::AmbiguousLanguage(path) => write!(
                f,
                "{} looks like both a python and a typescript project. Pass --language.",
                path.display()
            ),
            CliErrors::InvalidOptionCombination(reason) => write!(f, "{reason}."),
            CliErrors::UnknownProjectId { id, known } if known.is_empty() => write!(
                f,
                "No project with the id `{id}`. The workspace has no projects."
            ),
            CliErrors::UnknownProjectId { .. } => write!(f, "{}", self.render(false)),
            CliErrors::UnsafeMemoryPath(id) => write!(
                f,
                "Refusing to delete memory for the id `{id}`: it is not a plain folder name."
            ),
            CliErrors::ConfirmationFailed(reason) => write!(
                f,
                "Could not ask for confirmation ({reason}). Pass --yes to delete without asking."
            ),
        }
    }
}

impl Error for CliErrors {}

impl CliErrors {
    /// The message for the terminal: text in backticks is highlighted, and lists are bulleted.
    pub fn render(&self, colors: bool) -> String {
        match self {
            CliErrors::UnknownProjectId { id, known } if !known.is_empty() => {
                let mut message = highlight_code(
                    &format!("No project with the id `{id}`. \nKnown ids:"),
                    colors,
                );
                for known_id in known {
                    message.push('\n');
                    message.push_str(&table::bullet(
                        &table::paint(&Style::new().cyan().bold(), known_id, colors),
                        colors,
                    ));
                }
                message
            }
            _ => highlight_code(&self.to_string(), colors),
        }
    }
}

/// Colours every `quoted` part of `text`, such as ids and commands, keeping the backticks.
fn highlight_code(text: &str, colors: bool) -> String {
    text.split('`')
        .enumerate()
        .map(|(index, part)| {
            if index % 2 == 1 {
                table::paint(&Style::new().cyan(), &format!("`{part}`"), colors)
            } else {
                part.to_owned()
            }
        })
        .collect()
}

impl From<std::io::Error> for CliErrors {
    fn from(value: std::io::Error) -> Self {
        Self::DirectoryError(value)
    }
}
