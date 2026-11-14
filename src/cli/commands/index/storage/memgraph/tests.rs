use rsmgclient::QueryParam;

use std::{collections::HashMap, env, process};

use super::{
    MemgraphStore,
    batches::{batches, hex},
};
use crate::{
    cli::{
        commands::index::{
            parser::{ParseOutcome, Parser, python::PythonParser},
            storage::{ProjectChanges, Storage, StorageError},
        },
        workspace::{ExistingProject, StorageSettings},
    },
    graph::{FileGraph, SourceFile},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn plain<'a>(project: &'a str, path: &'a str) -> SourceFile<'a> {
    SourceFile {
        project,
        path,
        src_is_package: false,
        disambiguator: None,
        modules: None,
    }
}

fn parse(file: &SourceFile<'_>, source: &str) -> Result<FileGraph, String> {
    match PythonParser.parse(file, source.as_bytes()) {
        ParseOutcome::Parsed(graph) => Ok(graph),
        other => Err(format!("expected a clean parse, got {other:?}")),
    }
}

/// The text value of `key` in a map row.
fn text<'a>(row: &'a QueryParam, key: &str) -> Option<&'a str> {
    match row {
        QueryParam::Map(map) => match map.get(key) {
            Some(QueryParam::String(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn hashes_are_sixteen_hex_digits() {
    assert_eq!(hex(0), "0000000000000000");
    assert_eq!(hex(u64::MAX), "ffffffffffffffff");
}

#[test]
fn batches_group_nodes_by_label_and_references_by_type() -> TestResult {
    let graph = parse(
        &plain("api", "billing/models.py"),
        "from .base import Model\nclass Invoice(Model):\n    def total(self): ...\nrate = 1\n",
    )?;
    let batches = batches(&[graph]);
    assert_eq!(batches.files.len(), 1);
    assert_eq!(text(&batches.files[0], "path"), Some("billing/models.py"));
    assert_eq!(
        text(&batches.files[0], "module"),
        Some("api:billing.models")
    );
    assert_eq!(text(&batches.files[0], "hash").map(str::len), Some(16));
    let labels: Vec<(&str, usize)> = batches
        .nodes
        .iter()
        .map(|(label, rows)| (*label, rows.len()))
        .collect();
    assert_eq!(
        labels,
        [("Module", 1), ("Class", 1), ("Method", 1), ("Variable", 1)]
    );
    assert_eq!(
        text(&batches.nodes[2].1[0], "id"),
        Some("api:billing.models.Invoice.total")
    );
    assert_eq!(
        text(&batches.nodes[2].1[0], "path"),
        Some("billing/models.py")
    );
    assert_eq!(text(&batches.nodes[2].1[0], "kind"), Some("Method"));
    assert_eq!(batches.contains.len(), 3);
    let types: Vec<&str> = batches.references.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(types, ["IMPORTS", "INHERITS"]);
    assert_eq!(
        text(&batches.references[1].1[0], "target"),
        Some("billing.base.Model")
    );
    Ok(())
}

/// The Memgraph integration tests run only with `WORKBENCH_TEST_MEMGRAPH=host:port`.
const TEST_SERVER_VARIABLE: &str = "WORKBENCH_TEST_MEMGRAPH";

/// A store on its own namespace, deleted when dropped.
struct Store(MemgraphStore);

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.0.delete_namespace();
    }
}

/// A store for one test, or `None` when no test server is configured.
fn store(name: &str) -> Result<Option<Store>, StorageError> {
    let Ok(address) = env::var(TEST_SERVER_VARIABLE) else {
        return Ok(None);
    };
    let (host, port) = address.rsplit_once(':').unwrap_or((&address, "7687"));
    let settings = StorageSettings {
        host: host.to_owned(),
        port: port.parse().unwrap_or(7687),
        namespace: None,
        username: None,
        password_env: None,
    };
    let namespace = format!("wb-test-{name}-{}", process::id());
    let store = Store(MemgraphStore::connect(&settings, &namespace, None)?);
    store.0.delete_namespace()?;
    Ok(Some(store))
}

fn project(id: &str) -> ExistingProject {
    ExistingProject {
        id: id.to_owned(),
        path: id.to_owned(),
        language: Some("python".to_owned()),
        ..ExistingProject::default()
    }
}

fn graph(project: &str, path: &str, source: &str, hash: u64) -> Result<FileGraph, String> {
    let mut graph = parse(&plain(project, path), source)?;
    graph.hash = hash;
    Ok(graph)
}

fn upsert(graphs: Vec<FileGraph>) -> ProjectChanges {
    ProjectChanges {
        upserted: graphs,
        removed: Vec::new(),
    }
}

#[test]
fn written_files_read_back_as_hashes() -> TestResult {
    let Some(store) = store("hashes")? else {
        return Ok(());
    };
    store.0.write_project(
        &project("api"),
        &upsert(vec![
            graph("api", "a.py", "class A: ...\n", 1)?,
            graph("api", "b.py", "import a\n", u64::MAX)?,
        ]),
    )?;
    assert_eq!(
        store.0.stored_hashes("api")?,
        HashMap::from([("a.py".to_owned(), 1), ("b.py".to_owned(), u64::MAX)])
    );
    assert_eq!(store.0.ids("api")?, ["api:a", "api:a.A", "api:b"]);
    Ok(())
}

#[test]
fn changed_and_removed_files_replace_their_nodes() -> TestResult {
    let Some(store) = store("replace")? else {
        return Ok(());
    };
    store.0.write_project(
        &project("api"),
        &upsert(vec![
            graph("api", "a.py", "class Old: ...\n", 1)?,
            graph("api", "b.py", "x = 1\n", 2)?,
        ]),
    )?;
    store.0.write_project(
        &project("api"),
        &ProjectChanges {
            upserted: vec![graph("api", "a.py", "class New: ...\n", 3)?],
            removed: vec!["b.py".to_owned()],
        },
    )?;
    assert_eq!(store.0.ids("api")?, ["api:a", "api:a.New"]);
    assert_eq!(
        store.0.stored_hashes("api")?,
        HashMap::from([("a.py".to_owned(), 3)])
    );
    Ok(())
}

#[test]
fn a_failed_write_rolls_back() -> TestResult {
    let Some(store) = store("rollback")? else {
        return Ok(());
    };
    store.0.write_project(
        &project("api"),
        &upsert(vec![graph("api", "a.py", "class A: ...\n", 1)?]),
    )?;
    // Two files claiming the same node id break the uniqueness constraint.
    let failed = store.0.write_project(
        &project("api"),
        &upsert(vec![
            graph("api", "b.py", "class B: ...\n", 2)?,
            graph("api", "b.py", "class B: ...\n", 2)?,
        ]),
    );
    assert!(failed.is_err());
    assert_eq!(
        store.0.stored_hashes("api")?,
        HashMap::from([("a.py".to_owned(), 1)])
    );
    assert_eq!(store.0.ids("api")?, ["api:a", "api:a.A"]);
    Ok(())
}

#[test]
fn projects_with_the_same_file_path_stay_apart() -> TestResult {
    let Some(store) = store("same-path")? else {
        return Ok(());
    };
    store.0.write_project(
        &project("api"),
        &upsert(vec![graph("api", "main.py", "", 1)?]),
    )?;
    store.0.write_project(
        &project("web"),
        &upsert(vec![graph("web", "main.py", "", 2)?]),
    )?;
    store.0.write_project(
        &project("api"),
        &ProjectChanges {
            upserted: Vec::new(),
            removed: vec!["main.py".to_owned()],
        },
    )?;
    assert!(store.0.stored_hashes("api")?.is_empty());
    assert_eq!(
        store.0.stored_hashes("web")?,
        HashMap::from([("main.py".to_owned(), 2)])
    );
    Ok(())
}

#[test]
fn namespaces_do_not_see_each_other() -> TestResult {
    let (Some(first), Some(second)) = (store("namespace-one")?, store("namespace-two")?) else {
        return Ok(());
    };
    first
        .0
        .write_project(&project("api"), &upsert(vec![graph("api", "a.py", "", 1)?]))?;
    second
        .0
        .write_project(&project("api"), &upsert(vec![graph("api", "a.py", "", 2)?]))?;
    assert_eq!(
        first.0.stored_hashes("api")?,
        HashMap::from([("a.py".to_owned(), 1)])
    );
    assert_eq!(
        second.0.stored_hashes("api")?,
        HashMap::from([("a.py".to_owned(), 2)])
    );
    Ok(())
}

#[test]
fn removed_projects_are_deleted() -> TestResult {
    let Some(store) = store("cleanup")? else {
        return Ok(());
    };
    store.0.write_project(
        &project("api"),
        &upsert(vec![graph("api", "a.py", "class A: ...\n", 1)?]),
    )?;
    store.0.write_project(
        &project("old"),
        &upsert(vec![graph("old", "o.py", "class O: ...\n", 2)?]),
    )?;
    assert_eq!(store.0.remove_projects_except(&["api"])?, ["old"]);
    assert!(store.0.stored_hashes("old")?.is_empty());
    assert!(store.0.ids("old")?.is_empty());
    assert_eq!(store.0.ids("api")?, ["api:a", "api:a.A"]);
    Ok(())
}
