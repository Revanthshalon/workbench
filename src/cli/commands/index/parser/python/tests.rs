use super::{PythonParse, PythonParser, module_graph, parse_python_checked};
use crate::cli::commands::index::parser::{ParseOutcome, Parser};
use crate::graph::{FileGraph, NodeKind, ReferenceKind, Span};

/// Parses a snippet; tree-sitter never gives up on these, but if it did the graph
/// would hold only the module node.
fn parse_python(project: &str, relative_path: &str, source: &str) -> PythonParse {
    parse_python_checked(project, relative_path, source).unwrap_or_else(|| PythonParse {
        graph: module_graph(project, relative_path, source),
        error_lines: Vec::new(),
    })
}

fn ids(graph: &FileGraph) -> Vec<(&str, NodeKind)> {
    graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.kind))
        .collect()
}

fn contains(graph: &FileGraph) -> Vec<(&str, &str)> {
    graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect()
}

fn refs(graph: &FileGraph) -> Vec<(&str, &str, ReferenceKind)> {
    graph
        .references
        .iter()
        .map(|reference| {
            (
                reference.from.as_str(),
                reference.target.as_str(),
                reference.kind,
            )
        })
        .collect()
}

fn targets(graph: &FileGraph) -> Vec<&str> {
    graph
        .references
        .iter()
        .map(|reference| reference.target.as_str())
        .collect()
}

#[test]
fn classes_methods_and_nested_functions() {
    let source = "class Invoice:\n    def total(self):\n        def round_it(x):\n            return x\n        return 1\n\nasync def fetch():\n    pass\n";
    let graph = parse_python("api", "billing/invoices.py", source).graph;
    assert_eq!(
        ids(&graph),
        [
            ("api:billing.invoices", NodeKind::Module),
            ("api:billing.invoices.Invoice", NodeKind::Class),
            ("api:billing.invoices.Invoice.total", NodeKind::Method),
            (
                "api:billing.invoices.Invoice.total.round_it",
                NodeKind::Function
            ),
            ("api:billing.invoices.fetch", NodeKind::Function),
        ]
    );
    assert_eq!(
        contains(&graph),
        [
            ("api:billing.invoices", "api:billing.invoices.Invoice"),
            (
                "api:billing.invoices.Invoice",
                "api:billing.invoices.Invoice.total"
            ),
            (
                "api:billing.invoices.Invoice.total",
                "api:billing.invoices.Invoice.total.round_it"
            ),
            ("api:billing.invoices", "api:billing.invoices.fetch"),
        ]
    );
}

#[test]
fn decorated_definitions_span_their_decorators() {
    let graph = parse_python("api", "app.py", "@cache\n@log\ndef run():\n    pass\n").graph;
    assert_eq!(graph.nodes[1].id, "api:app.run");
    assert_eq!(
        graph.nodes[1].span,
        Span {
            start_line: 1,
            end_line: 4
        }
    );
}

#[test]
fn definitions_in_blocks_belong_to_the_enclosing_scope() {
    let source = "if TYPE_CHECKING:\n    def f(): ...\nelse:\n    def f(): ...\ntry:\n    class C: ...\nexcept ImportError:\n    pass\n";
    let graph = parse_python("api", "m.py", source).graph;
    assert_eq!(
        ids(&graph).iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        ["api:m", "api:m.f", "api:m.f#2", "api:m.C"]
    );
}

#[test]
fn stub_ids_do_not_collide_with_source_ids() {
    let graph = parse_python("api", "pkg/x.pyi", "class Foo: ...\n").graph;
    assert_eq!(graph.module, "api:pkg.x@pyi");
    assert_eq!(graph.nodes[1].id, "api:pkg.x.Foo@pyi");
}

#[test]
fn empty_and_comment_only_files_have_just_the_module() {
    for source in ["", "# only a comment\n"] {
        let graph = parse_python("api", "empty.py", source).graph;
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].span.start_line, 1);
    }
}

#[test]
fn non_ascii_names_and_crlf_lines() {
    let graph = parse_python("api", "m.py", "x = 1\r\n\r\ndef café():\r\n    pass\r\n").graph;
    assert_eq!(graph.nodes[1].name, "café");
    assert_eq!(
        graph.nodes[1].span,
        Span {
            start_line: 3,
            end_line: 4
        }
    );
}

#[test]
fn every_import_form() {
    let source = "import os.path\nimport numpy as np\nfrom a.b import c, d as e\nfrom m import *\n\ndef f():\n    import json\n";
    let graph = parse_python("api", "pkg/mod.py", source).graph;
    assert_eq!(
        refs(&graph),
        [
            ("api:pkg.mod", "os.path", ReferenceKind::Imports),
            ("api:pkg.mod", "numpy", ReferenceKind::Imports),
            ("api:pkg.mod", "a.b.c", ReferenceKind::Imports),
            ("api:pkg.mod", "a.b.d", ReferenceKind::Imports),
            ("api:pkg.mod", "m", ReferenceKind::Imports),
            ("api:pkg.mod.f", "json", ReferenceKind::Imports),
        ]
    );
    assert_eq!(graph.references[5].line, 7);
}

#[test]
fn relative_imports_resolve_against_the_package() {
    let module = parse_python(
        "api",
        "pkg/sub/mod.py",
        "from .x import y\nfrom .. import z\nfrom . import w\n",
    )
    .graph;
    assert_eq!(targets(&module), ["pkg.sub.x.y", "pkg.z", "pkg.sub.w"]);
    let init = parse_python("api", "pkg/sub/__init__.py", "from .x import y\n").graph;
    assert_eq!(targets(&init), ["pkg.sub.x.y"]);
}

#[test]
fn relative_imports_above_the_top_package_keep_their_dots() {
    let graph = parse_python("api", "a/b.py", "from ... import x\nfrom ...c import d\n").graph;
    assert_eq!(targets(&graph), ["...x", "...c.d"]);
}

#[test]
fn base_classes_resolve_through_imports_and_local_classes() {
    let source = "import abc\nfrom models import Base as B\nfrom typing import Generic\n\nclass Local: ...\nclass C(B, Local, abc.ABC, Exception, Generic[T], metaclass=Meta): ...\n";
    let graph = parse_python("api", "m.py", source).graph;
    let inherits: Vec<_> = refs(&graph)
        .into_iter()
        .filter(|(_, _, kind)| *kind == ReferenceKind::Inherits)
        .collect();
    assert_eq!(
        inherits,
        [
            ("api:m.C", "models.Base", ReferenceKind::Inherits),
            ("api:m.C", "m.Local", ReferenceKind::Inherits),
            ("api:m.C", "abc.ABC", ReferenceKind::Inherits),
            ("api:m.C", "Exception", ReferenceKind::Inherits),
            ("api:m.C", "typing.Generic", ReferenceKind::Inherits),
        ]
    );
}

#[test]
fn decorated_local_classes_resolve_as_bases() {
    let source = "@dataclass\nclass Base: ...\n\nclass C(Base): ...\n";
    let graph = parse_python("api", "m.py", source).graph;
    assert_eq!(targets(&graph), ["m.Base"]);
}

#[test]
fn syntax_errors_give_a_partial_graph_of_the_rest() -> Result<(), Box<dyn std::error::Error>> {
    let source = "def ok():\n    pass\n\ndef broken(:\n    pass\n\nclass Fine: ...\n";
    let ParseOutcome::Partial(graph, details) =
        PythonParser.parse("api", "m.py", source.as_bytes())
    else {
        return Err("expected a partial parse".into());
    };
    assert!(graph.nodes.iter().any(|node| node.id == "api:m.ok"));
    assert!(graph.nodes.iter().any(|node| node.id == "api:m.Fine"));
    assert!(
        details.starts_with("syntax errors at lines 4"),
        "unexpected details: {details}"
    );
    Ok(())
}

#[test]
fn invalid_utf8_fails() {
    assert!(matches!(
        PythonParser.parse("api", "m.py", &[0xff, 0xfe]),
        ParseOutcome::Failed(details) if details.starts_with("not valid UTF-8")
    ));
}

#[test]
fn clean_files_parse_fully() {
    assert!(matches!(
        PythonParser.parse("api", "m.py", b"x = 1\n"),
        ParseOutcome::Parsed(_)
    ));
}
