//! Finding the files of a project that the index covers.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
};

use crate::cli::constants::{PYTHON_EXTENSIONS, SKIPPED_DIRECTORIES, TYPESCRIPT_EXTENSIONS};

/// The file extensions indexed for a language, or `None` for one Workbench does not know.
pub fn extensions(language: &str) -> Option<&'static [&'static str]> {
    match language {
        "python" => Some(&PYTHON_EXTENSIONS),
        "typescript" => Some(&TYPESCRIPT_EXTENSIONS),
        _ => None,
    }
}

/// Decides which paths inside a project are indexed.
#[derive(Debug)]
pub struct Filter<'a> {
    pub extensions: &'a [&'a str],
    /// The project's virtual environment, relative to the project.
    pub venv: Option<&'a Path>,
}

impl Filter<'_> {
    /// Whether a path relative to the project is a source file outside skipped directories.
    pub fn includes(&self, relative: &Path) -> bool {
        let has_extension = relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| self.extensions.contains(&extension));
        let parent = relative.parent().unwrap_or(Path::new(""));
        has_extension
            && !parent.components().any(|component| match component {
                Component::Normal(name) => is_skipped_directory(&name.to_string_lossy()),
                _ => true,
            })
            && !self.venv.is_some_and(|venv| relative.starts_with(venv))
    }
}

fn is_skipped_directory(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_DIRECTORIES.contains(&name)
}

/// Every indexed file in the project, relative to it and sorted.
///
/// Symbolic links are listed but not followed; the caller skips them.
pub fn source_files(project_root: &Path, filter: &Filter<'_>) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_directory) = pending.pop() {
        for entry in fs::read_dir(project_root.join(&relative_directory))? {
            let entry = entry?;
            let relative = relative_directory.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_venv = filter.venv.is_some_and(|venv| relative == venv);
                if !is_skipped_directory(&name) && !is_venv {
                    pending.push(relative);
                }
            } else if filter.includes(&relative) {
                files.push(relative);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The files git reports as changed: modified, staged, deleted or untracked.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitChanges {
    /// Changed files that exist, relative to the project.
    pub present: Vec<PathBuf>,
    /// Changed files that were deleted.
    pub deleted: Vec<PathBuf>,
}

/// Asks git which indexed files in the project changed since the last commit.
///
/// Fails with the details for the log when git is missing or the project is not in
/// a repository.
pub fn git_changes(project_root: &Path, filter: &Filter<'_>) -> Result<GitChanges, String> {
    let has_head = git(project_root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
    let tracked = if has_head {
        git(
            project_root,
            &["diff", "--name-only", "-z", "--relative", "HEAD", "--"],
        )?
    } else {
        // A repository without commits: everything it tracks counts as changed.
        git(project_root, &["ls-files", "-z", "--cached"])?
    };
    let untracked = git(
        project_root,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?;

    let mut changes = GitChanges::default();
    let mut paths: Vec<PathBuf> = tracked
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .filter(|path| filter.includes(path))
        .collect();
    paths.sort();
    paths.dedup();
    for path in paths {
        if project_root.join(&path).exists() {
            changes.present.push(path);
        } else {
            changes.deleted.push(path);
        }
    }
    Ok(changes)
}

/// Runs git in `directory` and returns its output, or the details of why it failed.
fn git(directory: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "`git {}` failed ({}): {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// A path as stored in the index: relative, with `/` separators on every platform.
pub fn index_path(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
