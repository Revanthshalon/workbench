use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::PathBuf,
};

use engine::{ConfigStore, Error};

/// Stores the config as `<dir>/.workbench-cli/config.yml`.
#[derive(Debug)]
pub struct DirFileStore {
    dir: PathBuf,
}

impl DirFileStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl ConfigStore for DirFileStore {
    fn create(&self) -> Result<PathBuf, Error> {
        let dir = self.dir.join(".workbench-cli");
        fs::create_dir_all(&dir).map_err(|e| Error::Storage(e.to_string()))?;
        let path = dir.join("config.yml");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| match e.kind() {
                ErrorKind::AlreadyExists => Error::AlreadyExists(path.clone()),
                _ => Error::Storage(e.to_string()),
            })?;
        Ok(path)
    }
}
