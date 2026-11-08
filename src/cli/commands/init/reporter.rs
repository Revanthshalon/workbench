use std::path::Path;

use console::{StyledObject, style};
use indicatif::ProgressBar;

/// What `init` did with one file.
#[derive(Debug, Clone, Copy)]
pub(super) enum Status {
    Created,
    Exists,
    Rewrote,
    Updated,
}

impl Status {
    /// The status word, padded to a common width and coloured when stdout is a terminal.
    fn label(self) -> StyledObject<&'static str> {
        match self {
            Self::Created => style("created").green(),
            Self::Exists => style("exists ").dim(),
            Self::Rewrote => style("rewrote").yellow(),
            Self::Updated => style("updated").green(),
        }
    }
}

/// Prints one status line per file, with paths shown relative to the workspace root.
#[derive(Debug)]
pub(super) struct Reporter<'a> {
    pub(super) spinner: &'a ProgressBar,
    pub(super) workspace_root: &'a Path,
}

impl Reporter<'_> {
    pub(super) fn report(&self, status: Status, path: &Path) {
        let path = path.strip_prefix(self.workspace_root).unwrap_or(path);
        // The spinner is hidden while the line is written, so the two never tear.
        self.spinner
            .suspend(|| println!("  {}  {}", status.label(), path.display()));
    }
}
