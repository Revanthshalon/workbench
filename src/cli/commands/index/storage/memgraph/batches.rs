//! Code graphs → rows for `UNWIND` write queries.
//!
//! Cypher cannot take a label or a relationship type as a parameter, so nodes are
//! grouped by label and references by relationship type: the store writes one query
//! per group, with the label in the query text.

use std::collections::HashMap;

use rsmgclient::QueryParam;

use crate::graph::{FileGraph, NodeKind, ReferenceKind};

/// Node labels in the order their groups are written.
const NODE_LABELS: [(NodeKind, &str); 5] = [
    (NodeKind::Module, "Module"),
    (NodeKind::Class, "Class"),
    (NodeKind::Function, "Function"),
    (NodeKind::Method, "Method"),
    (NodeKind::Variable, "Variable"),
];
/// Relationship types from a code node to its `Reference` nodes, in write order.
const REFERENCE_TYPES: [(ReferenceKind, &str); 2] = [
    (ReferenceKind::Imports, "IMPORTS"),
    (ReferenceKind::Inherits, "INHERITS"),
];

/// The rows that write a set of files' graphs. (`QueryParam` has no `Debug`.)
#[derive(Default)]
pub(super) struct Batches {
    /// `{path, hash, module}` per file.
    pub(super) files: Vec<QueryParam>,
    /// Per label, `{id, kind, name, path, start_line, end_line}` per node.
    pub(super) nodes: Vec<(&'static str, Vec<QueryParam>)>,
    /// `{from, to}` per `Contains` edge.
    pub(super) contains: Vec<QueryParam>,
    /// Per relationship type, `{from, target, line, path}` per reference.
    pub(super) references: Vec<(&'static str, Vec<QueryParam>)>,
}

/// A hash as 16 lowercase hex digits; Memgraph integers are signed, ours are not.
pub(super) fn hex(hash: u64) -> String {
    format!("{hash:016x}")
}

/// The rows that write `graphs`.
pub(super) fn batches(graphs: &[FileGraph]) -> Batches {
    let mut batches = Batches::default();
    let mut nodes: HashMap<&str, Vec<QueryParam>> = HashMap::new();
    let mut references: HashMap<&str, Vec<QueryParam>> = HashMap::new();

    for graph in graphs {
        batches.files.push(row([
            ("path", text(&graph.path)),
            ("hash", text(&hex(graph.hash))),
            ("module", text(&graph.module)),
        ]));
        for node in &graph.nodes {
            let label = node_label(node.kind);
            nodes.entry(label).or_default().push(row([
                ("id", text(&node.id)),
                ("kind", text(label)),
                ("name", text(&node.name)),
                ("path", text(&graph.path)),
                ("start_line", int(node.span.start_line)),
                ("end_line", int(node.span.end_line)),
            ]));
        }
        batches.contains.extend(
            graph
                .edges
                .iter()
                .map(|edge| row([("from", text(&edge.from)), ("to", text(&edge.to))])),
        );
        for reference in &graph.references {
            references
                .entry(reference_type(reference.kind))
                .or_default()
                .push(row([
                    ("from", text(&reference.from)),
                    ("target", text(&reference.target)),
                    ("line", int(reference.line)),
                    ("path", text(&graph.path)),
                ]));
        }
    }

    batches.nodes = NODE_LABELS
        .iter()
        .filter_map(|(_, label)| Some((*label, nodes.remove(label)?)))
        .collect();
    batches.references = REFERENCE_TYPES
        .iter()
        .filter_map(|(_, kind)| Some((*kind, references.remove(kind)?)))
        .collect();
    batches
}

fn node_label(kind: NodeKind) -> &'static str {
    NODE_LABELS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or("Code", |(_, label)| label)
}

fn reference_type(kind: ReferenceKind) -> &'static str {
    REFERENCE_TYPES
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or("REFERENCES", |(_, name)| name)
}

fn row<const N: usize>(fields: [(&str, QueryParam); N]) -> QueryParam {
    QueryParam::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn text(value: &str) -> QueryParam {
    QueryParam::String(value.to_owned())
}

/// Lines fit easily in Memgraph's signed integers.
fn int(value: usize) -> QueryParam {
    QueryParam::Int(i64::try_from(value).unwrap_or(i64::MAX))
}
