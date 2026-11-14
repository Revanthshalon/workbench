use super::parse_python;
use crate::graph::{FileGraph, NodeKind, Span};

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
