//! Where the index is written: Memgraph, configured in the workspace `storage` block.
//!
//! The default build stores to Memgraph (`memgraph::MemgraphStore`). A build without
//! the `memgraph` feature has only [`NoStorage`], which stores nothing, so every run
//! treats all files as new.

use std::collections::HashMap;

use crate::{cli::workspace::ExistingProject, graph::FileGraph};

/// What changed in one project since the last run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProjectChanges {
    /// The graphs of files that are new or changed.
    pub upserted: Vec<FileGraph>,
    pub removed: Vec<String>,
}

/// What writing a project's changes did to the rest of the graph.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WriteResult {
    pub relinked: usize,
    pub stale_annotations: usize,
    pub orphaned_annotations: usize,
}

/// A storage error, with the details that go to the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageError(pub String);

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StorageError {}

pub trait Storage {
    /// The content hash of every file stored for `project` by the last run.
    fn stored_hashes(&self, project: &str) -> Result<HashMap<String, u64>, StorageError>;
    /// Writes one project's changed and removed files, all or nothing.
    fn write_project(
        &self,
        project: &ExistingProject,
        changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError>;
    /// Deletes every stored project whose id is not in `keep`, with all its data.
    /// Returns the deleted ids, sorted.
    fn remove_projects_except(&self, keep: &[&str]) -> Result<Vec<String>, StorageError>;
    /// One line for the summary saying where the index went.
    fn describe(&self) -> String;
}

/// Storage for builds without the `memgraph` feature: remembers nothing, writes nothing.
#[cfg(not(feature = "memgraph"))]
#[derive(Debug)]
pub struct NoStorage;

#[cfg(not(feature = "memgraph"))]
impl Storage for NoStorage {
    fn stored_hashes(&self, _project: &str) -> Result<HashMap<String, u64>, StorageError> {
        Ok(HashMap::new())
    }

    fn write_project(
        &self,
        _project: &ExistingProject,
        _changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError> {
        Ok(WriteResult::default())
    }

    fn remove_projects_except(&self, _keep: &[&str]) -> Result<Vec<String>, StorageError> {
        Ok(Vec::new())
    }

    fn describe(&self) -> String {
        "Storage: none (built without Memgraph support; nothing was written)".to_owned()
    }
}

/// Writing to Memgraph, built with the default `memgraph` feature.
#[cfg(feature = "memgraph")]
pub mod memgraph;
