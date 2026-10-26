use std::path::PathBuf;

use crate::Error;

/// Driven port: where the Workbench configuration lives.
pub trait ConfigStore {
    /// Creates an empty configuration, returning where it was written.
    ///
    /// Fails with [`Error::AlreadyExists`] rather than overwriting.
    fn create(&self) -> Result<PathBuf, Error>;
}
