use rsmgclient::QueryParam;

use super::batches::{batches, hex};
use crate::{
    cli::commands::index::parser::{ParseOutcome, Parser, python::PythonParser},
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
