//! The lock that stops two `workbench index` runs in one workspace at the same time.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process,
};

use crate::cli::{
    constants::INDEX_LOCK_FILE,
    errors::{CliErrors, CliResult},
};

/// Holds `.workbench/index.lock` while it lives, and deletes it when dropped.
#[derive(Debug)]
pub struct IndexLock {
    path: PathBuf,
}

impl IndexLock {
    /// Takes the lock, or fails when another run holds it.
    pub fn acquire(workbench_path: &Path) -> CliResult<Self> {
        let path = workbench_path.join(INDEX_LOCK_FILE);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                return Err(CliErrors::IndexLocked(path));
            }
            Err(error) => return Err(error.into()),
        };
        // The process id helps someone decide whether a leftover lock is stale.
        writeln!(file, "{}", process::id())?;
        Ok(Self { path })
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        // Nothing useful can be done if this fails; the next run reports the lock.
        let _removed = fs::remove_file(&self.path);
    }
}
