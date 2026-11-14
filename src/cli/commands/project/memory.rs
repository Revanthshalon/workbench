//! The per-project memory folders in `.workbench/memory/`.

use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::cli::errors::CliResult;

/// Every file under `directory` with its size in bytes, sorted by path.
pub(super) fn files_in(directory: &Path) -> CliResult<Vec<(PathBuf, u64)>> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                files.push((entry.path(), metadata.len()));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The folders directly in `memory_path` whose names are not in `known`, sorted.
pub(super) fn orphaned_folders(
    memory_path: &Path,
    known: &HashSet<String>,
) -> CliResult<Vec<PathBuf>> {
    if !memory_path.is_dir() {
        return Ok(Vec::new());
    }
    let mut orphans = Vec::new();
    for entry in fs::read_dir(memory_path)? {
        let entry = entry?;
        // `file_type` does not follow symbolic links, so a link is never treated as a folder.
        let is_folder = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_folder && !known.contains(&name) {
            orphans.push(entry.path());
        }
    }
    orphans.sort();
    Ok(orphans)
}

/// The id must name one folder directly under `.workbench/memory/`, never `..` or a path.
pub(super) fn is_plain_folder_name(id: &str) -> bool {
    let mut components = Path::new(id).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}
