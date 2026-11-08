use std::{
    collections::hash_map::RandomState,
    fs::{self, OpenOptions},
    hash::{BuildHasher, Hasher},
    io::Write,
    path::Path,
};

use super::reporter::{Reporter, Status};
use crate::cli::{
    constants::{
        DEFAULT_NAMESPACE_PREFIX, GITIGNORE_ENTRY, NAMESPACE_KEY, NAMESPACE_PLACEHOLDER,
        WORKBENCH_CONFIG_TEMPLATE,
    },
    errors::CliResult,
};

/// Writes the workspace config, leaving an existing one alone unless `force` is set.
///
/// A forced rewrite keeps the existing storage namespace, so the workspace stays
/// attached to the data already stored under it.
pub(super) fn write_config(
    reporter: &Reporter<'_>,
    config_path: &Path,
    force: bool,
) -> CliResult<()> {
    let existing = if config_path.exists() {
        Some(fs::read_to_string(config_path)?)
    } else {
        None
    };
    if existing.is_some() && !force {
        reporter.report(Status::Exists, config_path);
        return Ok(());
    }

    let namespace = existing
        .as_deref()
        .and_then(existing_namespace)
        .map_or_else(
            || workspace_namespace(reporter.workspace_root),
            str::to_owned,
        );
    fs::write(
        config_path,
        WORKBENCH_CONFIG_TEMPLATE.replace(NAMESPACE_PLACEHOLDER, &namespace),
    )?;
    let status = if existing.is_some() {
        Status::Rewrote
    } else {
        Status::Created
    };
    reporter.report(status, config_path);
    Ok(())
}

/// Reads the `namespace` value out of an existing workspace config.
fn existing_namespace(config: &str) -> Option<&str> {
    let value = config
        .lines()
        .find_map(|line| line.trim().strip_prefix(NAMESPACE_KEY))?;
    let value = value.split('#').next()?.trim().trim_matches(['"', '\'']);
    let is_valid = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    is_valid.then_some(value)
}

/// Writes a template file, leaving an existing one alone unless `overwrite` is set.
pub(super) fn write_template(
    reporter: &Reporter<'_>,
    path: &Path,
    contents: &str,
    overwrite: bool,
) -> CliResult<()> {
    let exists = path.exists();
    if exists && !overwrite {
        reporter.report(Status::Exists, path);
        return Ok(());
    }
    fs::write(path, contents)?;
    let status = if exists {
        Status::Rewrote
    } else {
        Status::Created
    };
    reporter.report(status, path);
    Ok(())
}

/// Appends the `.workbench/` entry to the `.gitignore`, creating the file if needed.
pub(super) fn ignore_workbench_directory(
    reporter: &Reporter<'_>,
    gitignore_path: &Path,
) -> CliResult<()> {
    let file_exists = gitignore_path.exists();
    let existing = if file_exists {
        fs::read_to_string(gitignore_path)?
    } else {
        String::new()
    };
    let already_ignored = existing
        .lines()
        .map(|line| line.trim().trim_matches('/'))
        .any(|line| line == GITIGNORE_ENTRY.trim_matches('/'));
    if already_ignored {
        reporter.report(Status::Exists, gitignore_path);
        return Ok(());
    }

    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut gitignore = OpenOptions::new()
        .create(true)
        .append(true)
        .open(gitignore_path)?;
    writeln!(gitignore, "{separator}{GITIGNORE_ENTRY}")?;
    let status = if file_exists {
        Status::Updated
    } else {
        Status::Created
    };
    reporter.report(status, gitignore_path);
    Ok(())
}

/// Builds a namespace such as `billing-3f9a`: the workspace directory name plus a random suffix.
fn workspace_namespace(workspace_root: &Path) -> String {
    let directory_name = fs::canonicalize(workspace_root)
        .ok()
        .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let prefix = directory_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    let prefix = if prefix.is_empty() {
        DEFAULT_NAMESPACE_PREFIX
    } else {
        &prefix
    };
    let suffix = RandomState::new().build_hasher().finish() & 0xffff;
    format!("{prefix}-{suffix:04x}")
}
