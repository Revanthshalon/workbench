use std::path::PathBuf;

/// Errors from engine use cases.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A configuration already exists at the location.
    #[error("{} already exists", .0.display())]
    AlreadyExists(PathBuf),
    /// The storage backend failed.
    #[error("storage error: {0}")]
    Storage(String),
}
