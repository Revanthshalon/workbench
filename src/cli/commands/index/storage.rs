//! Where the index is written: Memgraph, configured in the workspace `storage` block.
//!
//! Memgraph is not set up yet, so [`MemgraphPlaceholder`] stores nothing. Every run
//! therefore treats all files as new. The real store implements the same trait.

use std::collections::HashMap;

use crate::cli::workspace::StorageSettings;

/// A file as written to the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFile {
    /// Relative to the project, with `/` separators.
    pub path: String,
    pub hash: u64,
}

/// What changed in one project since the last run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProjectChanges {
    pub upserted: Vec<IndexedFile>,
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

pub trait Storage {
    /// The content hash of every file stored for `project` by the last run.
    fn stored_hashes(&self, project: &str) -> Result<HashMap<String, u64>, StorageError>;
    fn write_project(
        &self,
        project: &str,
        changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError>;
    /// One line for the summary saying where the index went.
    fn describe(&self) -> String;
}

/// Stands in for Memgraph until it is set up: remembers nothing, writes nothing.
#[derive(Debug)]
pub struct MemgraphPlaceholder {
    settings: StorageSettings,
}

impl MemgraphPlaceholder {
    pub fn new(settings: StorageSettings) -> Self {
        Self { settings }
    }
}

impl Storage for MemgraphPlaceholder {
    fn stored_hashes(&self, _project: &str) -> Result<HashMap<String, u64>, StorageError> {
        Ok(HashMap::new())
    }

    fn write_project(
        &self,
        _project: &str,
        _changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError> {
        Ok(WriteResult::default())
    }

    fn describe(&self) -> String {
        let namespace = self
            .settings
            .namespace
            .as_deref()
            .map(|namespace| format!(", namespace {namespace}"))
            .unwrap_or_default();
        format!(
            "Storage: Memgraph at {}:{}{namespace} (placeholder; nothing was written)",
            self.settings.host, self.settings.port
        )
    }
}
