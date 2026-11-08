use std::path::Path;

use crate::cli::errors::{CliErrors, CliResult};

/// Resolves the project id: the explicit one, or one derived from the directory name.
pub(super) fn resolve(explicit: Option<&str>, project_path: &Path) -> CliResult<String> {
    let (id, derived) = match explicit {
        Some(id) => (id.to_owned(), false),
        None => (derive(project_path), true),
    };
    if is_valid(&id) {
        Ok(id)
    } else {
        Err(CliErrors::InvalidProjectId { id, derived })
    }
}

/// Lower-cases the directory name and joins its alphanumeric parts with `-`.
fn derive(project_path: &Path) -> String {
    project_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

/// A valid id is kebab-case and starts with a letter, such as `billing-api`.
fn is_valid(id: &str) -> bool {
    id.starts_with(|character: char| character.is_ascii_lowercase())
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}
