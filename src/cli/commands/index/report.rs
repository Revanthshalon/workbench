//! What a run did, per project, and how it is printed.

use console::Style;

use crate::cli::table::{self, Kind};

/// The per-project counts in the summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub parsed: usize,
    pub unchanged: usize,
    pub removed: usize,
    /// Symbolic links and files over the size limit. Not failures.
    pub skipped: usize,
    /// Parsed in part. Not failures.
    pub partial: usize,
    /// Could not be read or parsed.
    pub failed: usize,
    pub relinked: usize,
    pub stale_annotations: usize,
    pub orphaned_annotations: usize,
}

/// How far indexing a project got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Its files were processed and written. Individual files may still have failed.
    Indexed,
    /// Its files were processed, but writing them to storage failed.
    WriteFailed,
    /// It could not be indexed at all, for example because its directory is missing.
    Failed,
    /// The run was interrupted while indexing it; nothing was written for it.
    Interrupted,
    /// The run was interrupted before reaching it.
    NotReached,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Indexed => "indexed",
            Self::WriteFailed => "write failed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::NotReached => "not reached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReport {
    pub id: String,
    pub counts: Counts,
    pub status: Status,
}

/// The outcome of a whole run.
#[derive(Debug, PartialEq, Eq)]
pub struct IndexRun {
    pub projects: Vec<ProjectReport>,
    pub interrupted: bool,
    /// Projects no longer in the config whose stored data this run deleted.
    pub removed_projects: Vec<String>,
    /// Deleting the data of projects no longer in the config failed.
    pub cleanup_failed: bool,
}

const HEADERS: [&str; 11] = [
    "PROJECT",
    "STATUS",
    "PARSED",
    "UNCHANGED",
    "REMOVED",
    "SKIPPED",
    "PARTIAL",
    "FAILED",
    "RE-LINKED",
    "STALE",
    "ORPHANED",
];
const STATUS_COLUMN: usize = 1;
const FAILED_COLUMN: usize = 7;

impl IndexRun {
    /// The summary table, then a line saying where the index was written.
    pub fn summary(&self, storage: &str, colors: bool) -> Vec<String> {
        let rows: Vec<[String; 11]> = self
            .projects
            .iter()
            .map(|project| {
                let counts = project.counts;
                [
                    project.id.clone(),
                    project.status.label().to_owned(),
                    counts.parsed.to_string(),
                    counts.unchanged.to_string(),
                    counts.removed.to_string(),
                    counts.skipped.to_string(),
                    counts.partial.to_string(),
                    counts.failed.to_string(),
                    counts.relinked.to_string(),
                    counts.stale_annotations.to_string(),
                    counts.orphaned_annotations.to_string(),
                ]
            })
            .collect();
        let mut lines = table::render(
            HEADERS,
            &rows,
            |column, row| match column {
                0 => Kind::Id,
                STATUS_COLUMN if row[STATUS_COLUMN] == Status::Indexed.label() => Kind::Success,
                STATUS_COLUMN => Kind::Danger,
                FAILED_COLUMN if row[FAILED_COLUMN] != "0" => Kind::Danger,
                _ => Kind::Plain,
            },
            colors,
        );
        if !self.removed_projects.is_empty() {
            let count = self.removed_projects.len();
            lines.push(format!(
                "Removed indexed data of {count} {} no longer in the config: {}",
                plural(count, "project", "projects"),
                self.removed_projects.join(", ")
            ));
        }
        lines.push(table::paint(&Style::new().dim(), storage, colors));
        lines
    }

    /// What went wrong, as short phrases for the error message; empty when nothing did.
    ///
    /// Skipped and partial files are not failures.
    pub fn problems(&self) -> Vec<String> {
        let failed_files: usize = self
            .projects
            .iter()
            .map(|project| project.counts.failed)
            .sum();
        let count = |status: Status| {
            self.projects
                .iter()
                .filter(|project| project.status == status)
                .count()
        };
        let mut problems = Vec::new();
        if failed_files > 0 {
            problems.push(format!(
                "{failed_files} {} could not be read or parsed",
                plural(failed_files, "file", "files")
            ));
        }
        let write_failed = count(Status::WriteFailed);
        if write_failed > 0 {
            problems.push(format!(
                "{write_failed} {} could not be written",
                plural(write_failed, "project", "projects")
            ));
        }
        let failed = count(Status::Failed);
        if failed > 0 {
            problems.push(format!(
                "{failed} {} could not be indexed",
                plural(failed, "project", "projects")
            ));
        }
        if self.interrupted {
            problems.push("the run was interrupted".to_owned());
        }
        if self.cleanup_failed {
            problems.push(
                "the data of projects removed from the config could not be deleted".to_owned(),
            );
        }
        problems
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}
