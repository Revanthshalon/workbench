//! The workspace a command runs in: its root directory and its `.workbench/config.yml`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use yaml_serde::{Mapping, Value};

use crate::cli::{
    constants::{
        DEFAULT_SEQUENCE_INDENT, DEFAULT_STORAGE_HOST, DEFAULT_STORAGE_PORT, PROJECTS_KEY,
        STORAGE_KEY, WORKBENCH_CONFIG_FILE, WORKBENCH_DIRECTORY, WORKBENCH_MEMORY_DIRECTORY,
    },
    errors::{CliErrors, CliResult},
};

#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    config_path: PathBuf,
    config: String,
}

/// A project already listed in the workspace config.
///
/// Fields are read as written, since the file may have been edited by hand.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExistingProject {
    pub id: String,
    pub path: String,
    pub language: Option<String>,
    pub framework: Option<String>,
    pub dependency_manager: Option<String>,
    /// `environment.venv`, relative to the project.
    pub venv: Option<String>,
}

/// The `storage` block of the workspace config: where Memgraph is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSettings {
    pub host: String,
    pub port: u16,
    pub namespace: Option<String>,
}

impl Workspace {
    /// Finds the workspace in `start` or the nearest parent directory that has one.
    pub fn find(start: &Path) -> CliResult<Self> {
        let start = fs::canonicalize(start)?;
        for root in start.ancestors() {
            let config_path = root.join(WORKBENCH_DIRECTORY).join(WORKBENCH_CONFIG_FILE);
            if config_path.is_file() {
                return Ok(Self {
                    root: root.to_path_buf(),
                    config: fs::read_to_string(&config_path)?,
                    config_path,
                });
            }
        }
        Err(CliErrors::WorkspaceNotFound(start))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `.workbench/` directory.
    pub fn workbench_path(&self) -> PathBuf {
        self.root.join(WORKBENCH_DIRECTORY)
    }

    /// The `storage` block, with defaults for the fields it leaves out.
    pub fn storage(&self) -> CliResult<StorageSettings> {
        let document: Value =
            yaml_serde::from_str(&self.config).map_err(|error| self.invalid(&error.to_string()))?;
        let storage = document.get(STORAGE_KEY);
        let field = |key: &str| storage.and_then(|storage| storage.get(key));
        let port = match field("port") {
            None | Some(Value::Null) => DEFAULT_STORAGE_PORT,
            Some(value) => value
                .as_u64()
                .and_then(|port| u16::try_from(port).ok())
                .ok_or_else(|| self.invalid("`storage.port` must be a port number"))?,
        };
        Ok(StorageSettings {
            host: field("host")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_STORAGE_HOST)
                .to_owned(),
            port,
            namespace: field("namespace")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// The directory that holds the per-project memory folders.
    pub fn memory_path(&self) -> PathBuf {
        self.root
            .join(WORKBENCH_DIRECTORY)
            .join(WORKBENCH_MEMORY_DIRECTORY)
    }

    pub fn projects(&self) -> CliResult<Vec<ExistingProject>> {
        Ok(self
            .parse(&self.config)?
            .iter()
            .map(|project| {
                let field = |key: &str| project.get(key).and_then(Value::as_str).map(str::to_owned);
                ExistingProject {
                    id: field("id").unwrap_or_default(),
                    path: field("path").unwrap_or_default(),
                    language: field("language"),
                    framework: field("framework"),
                    dependency_manager: field("dependency_manager"),
                    venv: project
                        .get("environment")
                        .and_then(|environment| environment.get("venv"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }
            })
            .collect())
    }

    /// Appends a project to the `projects` list and saves the config.
    ///
    /// The entry is inserted as text so the comments and layout of the file survive.
    pub fn append_project(&mut self, id: &str, entry: Mapping) -> CliResult<()> {
        let entry = yaml_serde::to_string(&Value::Sequence(vec![Value::Mapping(entry)]))
            .map_err(|error| self.invalid(&error.to_string()))?;
        let updated = insert_project_entry(&self.config, &entry).ok_or_else(|| {
            self.invalid(
                "`projects` must be written as a block list (one `- id:` entry per project)",
            )
        })?;

        // Guards the text edit: the result must parse and end with the new project.
        let projects = self.parse(&updated)?;
        let is_appended = projects.len() == self.parse(&self.config)?.len() + 1
            && projects
                .last()
                .and_then(|project| project.get("id"))
                .and_then(Value::as_str)
                == Some(id);
        if !is_appended {
            return Err(self.invalid("could not add the project to the `projects` list"));
        }

        fs::write(&self.config_path, &updated)?;
        self.config = updated;
        Ok(())
    }

    /// Removes the project with `id` from the `projects` list and saves the config.
    ///
    /// Like [`Self::append_project`], it edits the text so comments and layout survive.
    pub fn remove_project(&mut self, id: &str) -> CliResult<()> {
        let projects = self.parse(&self.config)?;
        let index = projects
            .iter()
            .position(|project| project.get("id").and_then(Value::as_str) == Some(id))
            .ok_or_else(|| CliErrors::UnknownProjectId {
                id: id.to_owned(),
                known: Vec::new(),
            })?;
        let updated =
            remove_project_entry(&self.config, index, projects.len()).ok_or_else(|| {
                self.invalid(
                    "`projects` must be written as a block list (one `- id:` entry per project)",
                )
            })?;

        // Guards the text edit: exactly that one project is gone, the rest are untouched.
        let mut expected = projects;
        expected.remove(index);
        if self.parse(&updated)? != expected {
            return Err(self.invalid("could not remove the project from the `projects` list"));
        }

        fs::write(&self.config_path, &updated)?;
        self.config = updated;
        Ok(())
    }

    /// `target` relative to the workspace root, with `/` separators.
    pub fn relative_path(&self, target: &Path) -> String {
        let root: Vec<_> = self.root.components().collect();
        let target_components: Vec<_> = target.components().collect();
        let common = root
            .iter()
            .zip(&target_components)
            .take_while(|(left, right)| left == right)
            .count();
        if common == 0 {
            return target.to_string_lossy().into_owned();
        }
        let parts: Vec<_> = root[common..]
            .iter()
            .map(|_| "..".to_owned())
            .chain(
                target_components[common..]
                    .iter()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned()),
            )
            .collect();
        if parts.is_empty() {
            ".".to_owned()
        } else {
            parts.join("/")
        }
    }

    fn parse(&self, config: &str) -> CliResult<Vec<Value>> {
        let document: Value =
            yaml_serde::from_str(config).map_err(|error| self.invalid(&error.to_string()))?;
        match document.get(PROJECTS_KEY) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Sequence(projects)) => Ok(projects.clone()),
            Some(_) => Err(self.invalid("`projects` must be a list")),
        }
    }

    fn invalid(&self, reason: &str) -> CliErrors {
        CliErrors::InvalidWorkspaceConfig {
            path: self.config_path.clone(),
            reason: reason.to_owned(),
        }
    }
}

/// Inserts a serialised `- id: ...` entry at the end of the top-level `projects` list.
///
/// Returns `None` when the list is written inline (`projects: [{...}]`).
fn insert_project_entry(config: &str, entry: &str) -> Option<String> {
    let mut lines: Vec<String> = config.lines().map(str::to_owned).collect();
    let key = format!("{PROJECTS_KEY}:");

    let (insert_at, indent) = match lines.iter().position(|line| line.starts_with(&key)) {
        None => {
            lines.push(key);
            (lines.len(), DEFAULT_SEQUENCE_INDENT.to_owned())
        }
        Some(key_index) => {
            let inline = lines[key_index][key.len()..]
                .split('#')
                .next()
                .unwrap_or_default()
                .trim();
            match inline {
                "[]" => {
                    lines[key_index] = key;
                    (key_index + 1, DEFAULT_SEQUENCE_INDENT.to_owned())
                }
                "" => block_end(&lines, key_index),
                _ => return None,
            }
        }
    };

    let entry_lines = entry.lines().map(|line| format!("{indent}{line}"));
    lines.splice(insert_at..insert_at, entry_lines);
    Some(lines.join("\n") + "\n")
}

/// Removes the `index`-th of `count` entries from the top-level `projects` block list.
///
/// Returns `None` when the list is not a block list with exactly `count` entries.
fn remove_project_entry(config: &str, index: usize, count: usize) -> Option<String> {
    let mut lines: Vec<String> = config.lines().map(str::to_owned).collect();
    let key = format!("{PROJECTS_KEY}:");
    let key_index = lines.iter().position(|line| line.starts_with(&key))?;
    let inline = lines[key_index][key.len()..].split('#').next()?.trim();
    if !inline.is_empty() {
        return None;
    }

    let (end, indent) = block_end(&lines, key_index);
    let starts: Vec<usize> = (key_index + 1..end)
        .filter(|&line| {
            lines[line]
                .strip_prefix(indent.as_str())
                .is_some_and(|rest| rest.starts_with('-'))
        })
        .collect();
    if starts.len() != count {
        return None;
    }

    let entry_end = starts.get(index + 1).copied().unwrap_or(end);
    lines.drain(starts[index]..entry_end);
    if count == 1 {
        lines[key_index] = format!("{key} []");
    }
    Some(lines.join("\n") + "\n")
}

/// Where the `projects` block ends, and the indentation its entries use.
fn block_end(lines: &[String], key_index: usize) -> (usize, String) {
    let is_next_key = |line: &String| !line.is_empty() && !line.starts_with([' ', '\t', '-', '#']);
    let mut end = lines
        .iter()
        .enumerate()
        .skip(key_index + 1)
        .find(|(_, line)| is_next_key(line))
        .map_or(lines.len(), |(index, _)| index);
    // Blank lines and comments just above the next key belong to that key.
    while end > key_index + 1
        && (lines[end - 1].trim().is_empty() || lines[end - 1].starts_with('#'))
    {
        end -= 1;
    }

    let indent = lines[key_index + 1..end]
        .iter()
        .find(|line| line.trim_start().starts_with('-'))
        .map_or(DEFAULT_SEQUENCE_INDENT, |line| {
            &line[..line.len() - line.trim_start().len()]
        });
    (end, indent.to_owned())
}
