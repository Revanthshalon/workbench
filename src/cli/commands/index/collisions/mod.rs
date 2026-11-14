//! Which files of a project lose a module-id collision.
//!
//! Two files can map to the same module name: `pkg/x.py` and `pkg/x/__init__.py`, or
//! a root script `x.py` and `src/x.py` in a plain src layout. The file Python (or the
//! toolchain) would actually load keeps the plain id; the others are disambiguated
//! with their path.

use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// The stem of a package file: `pkg/x/__init__.py` is the package `pkg.x`.
const PACKAGE_FILE_STEM: &str = "__init__";
/// The folder of the "src layout", dropped from module names unless it is a package.
const SRC_FOLDER: &str = "src/";

/// A source file and the module name it would get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleFile {
    /// Relative to the project, with `/` separators.
    pub path: String,
    pub module: String,
    /// A `.pyi` stub: stubs only collide with stubs.
    pub stub: bool,
}

/// Files that share a module name, and which of them keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    pub module: String,
    pub winner: String,
    /// In precedence order.
    pub losers: Vec<String>,
}

/// The collisions among `files`, each with its winner.
///
/// Within a group sharing a module name (and stub-ness), the first by these keys wins:
/// a package file; then, in a plain src layout, a file under `src/`; then the earlier
/// extension in `extensions`; then the path.
pub fn losers(files: &[ModuleFile], extensions: &[&str], src_is_package: bool) -> Vec<Collision> {
    let mut groups: BTreeMap<(&str, bool), Vec<&ModuleFile>> = BTreeMap::new();
    for file in files {
        groups
            .entry((file.module.as_str(), file.stub))
            .or_default()
            .push(file);
    }
    groups
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .filter_map(|((module, _), mut group)| {
            group.sort_by_key(|file| precedence(&file.path, extensions, src_is_package));
            let (winner, losers) = group.split_first()?;
            Some(Collision {
                module: module.to_owned(),
                winner: winner.path.clone(),
                losers: losers.iter().map(|file| file.path.clone()).collect(),
            })
        })
        .collect()
}

/// The sort key that puts the file to keep a module name first.
fn precedence<'a>(
    path: &'a str,
    extensions: &[&str],
    src_is_package: bool,
) -> (bool, bool, usize, &'a str) {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let (stem, extension) = file_name.rsplit_once('.').unwrap_or((file_name, ""));
    let is_package = stem == PACKAGE_FILE_STEM;
    let in_src_layout = !src_is_package && path.starts_with(SRC_FOLDER);
    let extension_rank = extensions
        .iter()
        .position(|known| *known == extension)
        .unwrap_or(extensions.len());
    (!is_package, !in_src_layout, extension_rank, path)
}
