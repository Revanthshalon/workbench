//! Storing the code graph in Memgraph.
//!
//! Every node carries the workspace's `namespace`, and every query filters on it, so
//! workspaces sharing one Memgraph never see each other. Below the workspace, nodes
//! also carry their `project`. A project is written in one transaction: the data of
//! its changed and removed files is deleted, then the new graphs are created in
//! batches. Any error rolls the whole project back.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    thread,
    time::Duration,
};

use rsmgclient::{ConnectParams, Connection, MgError, QueryParam, Record, Value};

use super::{ProjectChanges, Storage, StorageError, WriteResult};
use crate::cli::workspace::{ExistingProject, StorageSettings};

mod batches;
#[cfg(test)]
mod tests;

use batches::{Batches, batches};

/// An index or constraint the store relies on.
struct SchemaItem {
    /// As `SHOW INDEX INFO` / `SHOW CONSTRAINT INFO` describe it: `Code(id)`.
    key: &'static str,
    unique: bool,
    statement: &'static str,
}

/// The indexes and the uniqueness constraint, created on connect when missing.
const SCHEMA: [SchemaItem; 7] = [
    SchemaItem {
        key: "Workspace(namespace)",
        unique: false,
        statement: "CREATE INDEX ON :Workspace(namespace)",
    },
    SchemaItem {
        key: "Project(id)",
        unique: false,
        statement: "CREATE INDEX ON :Project(id)",
    },
    SchemaItem {
        key: "File(path)",
        unique: false,
        statement: "CREATE INDEX ON :File(path)",
    },
    SchemaItem {
        key: "Code(id)",
        unique: false,
        statement: "CREATE INDEX ON :Code(id)",
    },
    SchemaItem {
        key: "Code(path)",
        unique: false,
        statement: "CREATE INDEX ON :Code(path)",
    },
    SchemaItem {
        key: "Reference(path)",
        unique: false,
        statement: "CREATE INDEX ON :Reference(path)",
    },
    SchemaItem {
        key: "Code(namespace,id)",
        unique: true,
        statement: "CREATE CONSTRAINT ON (c:Code) ASSERT c.namespace, c.id IS UNIQUE",
    },
];
/// Creating schema needs exclusive access, which waits for other transactions; a
/// busy server is retried this many times, this long apart.
const SCHEMA_ATTEMPTS: u32 = 10;
const SCHEMA_RETRY_DELAY: Duration = Duration::from_millis(300);

/// What it takes to open a connection, kept so a broken one can be replaced.
struct Credentials {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

/// The code graph in a Memgraph database, scoped to one workspace's namespace.
pub struct MemgraphStore {
    connection: RefCell<Connection>,
    credentials: Credentials,
    namespace: String,
    address: String,
}

impl std::fmt::Debug for MemgraphStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemgraphStore")
            .field("namespace", &self.namespace)
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl MemgraphStore {
    /// Connects, and makes sure the indexes and the uniqueness constraint exist.
    pub fn connect(
        settings: &StorageSettings,
        namespace: &str,
        password: Option<String>,
    ) -> Result<Self, StorageError> {
        let credentials = Credentials {
            host: settings.host.clone(),
            port: settings.port,
            username: settings.username.clone(),
            password,
        };
        // Schema statements cannot run inside a transaction, so this one autocommits.
        let mut connection = open(&credentials, true)?;
        ensure_schema(&mut connection)?;
        connection.set_autocommit(false);
        Ok(Self {
            connection: RefCell::new(connection),
            credentials,
            namespace: namespace.to_owned(),
            address: format!("{}:{}", settings.host, settings.port),
        })
    }

    /// Runs `text` with `params` plus `$ns`, and returns its records.
    fn query(
        &self,
        text: &str,
        mut params: HashMap<String, QueryParam>,
    ) -> Result<Vec<Record>, StorageError> {
        params.insert("ns".to_owned(), QueryParam::String(self.namespace.clone()));
        let mut connection = self.connection.borrow_mut();
        connection
            .execute(text, Some(&params))
            .map_err(storage_error)?;
        connection.fetchall().map_err(storage_error)
    }

    /// Runs `work` in one transaction: commits on success, rolls back on error.
    fn transaction<T>(
        &self,
        work: impl FnOnce(&Self) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        // Memgraph reports some errors, such as a broken constraint, only at commit.
        let result = work(self).and_then(|value| {
            self.connection
                .borrow_mut()
                .commit()
                .map_err(storage_error)
                .map(|()| value)
        });
        if result.is_err() {
            // The original error matters more than a failed rollback.
            let _rolled_back = self.connection.borrow_mut().rollback();
            // After a failed query the client refuses further queries on this
            // connection, so it is replaced; the server drops the old session's
            // transaction with it. If reconnecting fails, the next query reports it.
            if let Ok(fresh) = open(&self.credentials, false) {
                *self.connection.borrow_mut() = fresh;
            }
        }
        result
    }

    /// Deletes the stored data of `paths` in `project`: references, code nodes, files.
    fn delete_files(&self, project: &str, paths: &[String]) -> Result<(), StorageError> {
        if paths.is_empty() {
            return Ok(());
        }
        // `QueryParam` is not `Clone`, so the list is built once per query.
        let list = || QueryParam::List(paths.iter().cloned().map(QueryParam::String).collect());
        for label in ["Reference", "Code", "File"] {
            self.query(
                &format!(
                    "UNWIND $paths AS path \
                     MATCH (n:{label} {{namespace: $ns, project: $project, path: path}}) \
                     DETACH DELETE n"
                ),
                params([("project", text(project)), ("paths", list())]),
            )?;
        }
        Ok(())
    }

    /// Creates the nodes and relationships of `batches` in `project`.
    fn create(&self, project: &str, batches: Batches) -> Result<(), StorageError> {
        let run = |query: String, rows: Vec<QueryParam>| -> Result<(), StorageError> {
            if rows.is_empty() {
                return Ok(());
            }
            self.query(
                &query,
                params([("project", text(project)), ("rows", QueryParam::List(rows))]),
            )
            .map(drop)
        };
        for (label, rows) in batches.nodes {
            run(
                format!(
                    "UNWIND $rows AS row \
                     CREATE (:Code:{label} {{namespace: $ns, project: $project, id: row.id, \
                     kind: row.kind, name: row.name, path: row.path, \
                     start_line: row.start_line, end_line: row.end_line}})"
                ),
                rows,
            )?;
        }
        run(
            "UNWIND $rows AS row \
             MATCH (p:Project {namespace: $ns, id: $project}) \
             MATCH (m:Code {namespace: $ns, id: row.module}) \
             CREATE (f:File {namespace: $ns, project: $project, path: row.path, hash: row.hash}) \
             CREATE (p)-[:CONTAINS]->(f) \
             CREATE (f)-[:CONTAINS]->(m)"
                .to_owned(),
            batches.files,
        )?;
        run(
            "UNWIND $rows AS row \
             MATCH (a:Code {namespace: $ns, id: row.from}) \
             MATCH (b:Code {namespace: $ns, id: row.to}) \
             CREATE (a)-[:CONTAINS]->(b)"
                .to_owned(),
            batches.contains,
        )?;
        for (kind, rows) in batches.references {
            run(
                format!(
                    "UNWIND $rows AS row \
                     MATCH (a:Code {{namespace: $ns, id: row.from}}) \
                     CREATE (a)-[:{kind} {{line: row.line}}]->(:Reference {{namespace: $ns, \
                     project: $project, path: row.path, target: row.target, line: row.line}})"
                ),
                rows,
            )?;
        }
        Ok(())
    }

    /// Deletes everything stored under this namespace. For tests.
    #[cfg(test)]
    fn delete_namespace(&self) -> Result<(), StorageError> {
        self.transaction(|store| {
            store
                .query("MATCH (n {namespace: $ns}) DETACH DELETE n", HashMap::new())
                .map(drop)
        })
    }

    /// The ids of a project's code nodes, sorted. For tests.
    #[cfg(test)]
    fn ids(&self, project: &str) -> Result<Vec<String>, StorageError> {
        let records = self.transaction(|store| {
            store.query(
                "MATCH (c:Code {namespace: $ns, project: $project}) RETURN c.id ORDER BY c.id",
                params([("project", text(project))]),
            )
        })?;
        Ok(records
            .iter()
            .filter_map(|record| string(record.values.first()))
            .collect())
    }
}

impl Storage for MemgraphStore {
    fn stored_hashes(&self, project: &str) -> Result<HashMap<String, u64>, StorageError> {
        let records = self.transaction(|store| {
            store.query(
                "MATCH (f:File {namespace: $ns, project: $project}) RETURN f.path, f.hash",
                params([("project", text(project))]),
            )
        })?;
        // A hash that does not parse is skipped, so that file is simply parsed again.
        Ok(records
            .iter()
            .filter_map(|record| {
                let path = string(record.values.first())?;
                let hash = u64::from_str_radix(&string(record.values.get(1))?, 16).ok()?;
                Some((path, hash))
            })
            .collect())
    }

    fn write_project(
        &self,
        project: &ExistingProject,
        changes: &ProjectChanges,
    ) -> Result<WriteResult, StorageError> {
        let replaced: Vec<String> = changes
            .removed
            .iter()
            .cloned()
            .chain(changes.upserted.iter().map(|graph| graph.path.clone()))
            .collect();
        self.transaction(|store| {
            store.query(
                "MERGE (w:Workspace {namespace: $ns}) \
                 MERGE (p:Project {namespace: $ns, id: $project}) \
                 SET p.path = $path, p.language = $language, p.framework = $framework \
                 MERGE (w)-[:CONTAINS]->(p)",
                params([
                    ("project", text(&project.id)),
                    ("path", text(&project.path)),
                    ("language", optional(project.language.as_deref())),
                    ("framework", optional(project.framework.as_deref())),
                ]),
            )?;
            store.delete_files(&project.id, &replaced)?;
            store.create(&project.id, batches(&changes.upserted))
        })?;
        Ok(WriteResult::default())
    }

    fn remove_projects_except(&self, keep: &[&str]) -> Result<Vec<String>, StorageError> {
        let keep = QueryParam::List(keep.iter().map(|id| text(id)).collect());
        self.transaction(|store| {
            let records = store.query(
                "MATCH (p:Project {namespace: $ns}) WHERE NOT p.id IN $keep \
                 RETURN p.id ORDER BY p.id",
                params([("keep", keep)]),
            )?;
            let removed: Vec<String> = records
                .iter()
                .filter_map(|record| string(record.values.first()))
                .collect();
            let ids = || QueryParam::List(removed.iter().map(|id| text(id)).collect());
            store.query(
                "UNWIND $ids AS id MATCH (n {namespace: $ns, project: id}) DETACH DELETE n",
                params([("ids", ids())]),
            )?;
            store.query(
                "MATCH (p:Project {namespace: $ns}) WHERE p.id IN $ids DETACH DELETE p",
                params([("ids", ids())]),
            )?;
            Ok(removed)
        })
    }

    fn describe(&self) -> String {
        format!(
            "Storage: Memgraph at {}, namespace {}",
            self.address, self.namespace
        )
    }
}

/// Opens a connection with `autocommit` set as given.
fn open(credentials: &Credentials, autocommit: bool) -> Result<Connection, StorageError> {
    let params = ConnectParams {
        host: Some(credentials.host.clone()),
        port: credentials.port,
        username: credentials.username.clone(),
        password: credentials.password.clone(),
        lazy: false,
        autocommit,
        ..ConnectParams::default()
    };
    Connection::connect(&params).map_err(storage_error)
}

/// Creates the indexes and the constraint that do not exist yet. Existing ones are
/// left alone, since creating schema needs exclusive access to the database.
fn ensure_schema(connection: &mut Connection) -> Result<(), StorageError> {
    let indexes = existing_schema(connection, "SHOW INDEX INFO")?;
    let constraints = existing_schema(connection, "SHOW CONSTRAINT INFO")?;
    let missing = SCHEMA.iter().filter(|item| {
        let existing = if item.unique { &constraints } else { &indexes };
        !existing.contains(item.key)
    });
    for item in missing {
        create_schema_item(connection, item.statement)?;
    }
    Ok(())
}

/// Runs a schema statement, retrying while the database is busy with other
/// transactions.
fn create_schema_item(connection: &mut Connection, statement: &str) -> Result<(), StorageError> {
    let mut attempt = 1;
    loop {
        match connection.execute_without_results(statement) {
            Ok(()) => return Ok(()),
            Err(error) if error.to_string().contains("already exists") => return Ok(()),
            Err(error) if attempt < SCHEMA_ATTEMPTS && is_busy(&error) => {
                attempt += 1;
                thread::sleep(SCHEMA_RETRY_DELAY);
            }
            Err(error) => return Err(storage_error(error)),
        }
    }
}

fn is_busy(error: &MgError) -> bool {
    error.to_string().contains("Cannot get")
}

/// The `Label(property,...)` keys a `SHOW ... INFO` query lists.
fn existing_schema(
    connection: &mut Connection,
    query: &str,
) -> Result<HashSet<String>, StorageError> {
    connection.execute(query, None).map_err(storage_error)?;
    let records = connection.fetchall().map_err(storage_error)?;
    Ok(records
        .iter()
        .filter_map(|record| {
            let label = string(record.values.get(1))?;
            let Some(Value::List(properties)) = record.values.get(2) else {
                return None;
            };
            let properties: Vec<String> = properties
                .iter()
                .filter_map(|property| string(Some(property)))
                .collect();
            Some(format!("{label}({})", properties.join(",")))
        })
        .collect())
}

fn storage_error(error: MgError) -> StorageError {
    StorageError(error.to_string())
}

fn params<const N: usize>(entries: [(&str, QueryParam); N]) -> HashMap<String, QueryParam> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn text(value: &str) -> QueryParam {
    QueryParam::String(value.to_owned())
}

fn optional(value: Option<&str>) -> QueryParam {
    value.map_or(QueryParam::Null, text)
}

fn string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
}
