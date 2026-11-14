use super::{FileGraph, NodeKind, SourceFile, Span, module_name, node_id};

fn source<'a>(project: &'a str, path: &'a str) -> SourceFile<'a> {
    SourceFile {
        project,
        path,
        src_is_package: false,
        disambiguator: None,
    }
}

#[test]
fn module_names_follow_python_packages() {
    assert_eq!(
        module_name("billing/invoices.py", false),
        "billing.invoices"
    );
    assert_eq!(module_name("billing/__init__.py", false), "billing");
    assert_eq!(
        module_name("src/billing/invoices.py", false),
        "billing.invoices"
    );
    assert_eq!(module_name("app.py", false), "app");
    assert_eq!(module_name("pkg/types.pyi", false), "pkg.types");
    // Only a leading `src/` is the "src layout".
    assert_eq!(module_name("tools/src/x.py", false), "tools.src.x");
}

#[test]
fn root_init_files_are_named_init() {
    assert_eq!(module_name("__init__.py", false), "__init__");
    assert_eq!(module_name("src/__init__.py", false), "__init__");
}

#[test]
fn a_src_package_is_part_of_module_names() {
    assert_eq!(module_name("src/config.py", true), "src.config");
    assert_eq!(
        module_name("src/models/user_models.py", true),
        "src.models.user_models"
    );
    assert_eq!(module_name("src/__init__.py", true), "src");
    assert_eq!(module_name("tests/conftest.py", true), "tests.conftest");
}

#[test]
fn ids_join_project_and_qualified_name() {
    assert_eq!(
        node_id("billing-api", "billing.invoices.Invoice.total"),
        "billing-api:billing.invoices.Invoice.total"
    );
}

#[test]
fn module_only_graph_has_one_module_node() {
    let graph = FileGraph::module_only(&source("web", "src/main.ts"), 0);
    assert_eq!(graph.module, "web:main");
    assert_eq!(graph.path, "src/main.ts");
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].kind, NodeKind::Module);
    assert_eq!(graph.nodes[0].name, "main");
    assert_eq!(
        graph.nodes[0].span,
        Span {
            start_line: 1,
            end_line: 1
        }
    );
    assert!(graph.edges.is_empty() && graph.references.is_empty());
}

#[test]
fn describe_lists_nodes_edges_and_references() {
    use super::{Edge, EdgeKind, Node, Reference, ReferenceKind};

    let mut graph = FileGraph::module_only(&source("api", "billing/models.py"), 5);
    graph.nodes.push(Node {
        id: "api:billing.models.Invoice".to_owned(),
        kind: NodeKind::Class,
        name: "Invoice".to_owned(),
        span: Span {
            start_line: 3,
            end_line: 5,
        },
    });
    graph.edges.push(Edge {
        from: "api:billing.models".to_owned(),
        to: "api:billing.models.Invoice".to_owned(),
        kind: EdgeKind::Contains,
    });
    graph.references.push(Reference {
        from: "api:billing.models.Invoice".to_owned(),
        target: "billing.base.Model".to_owned(),
        kind: ReferenceKind::Inherits,
        line: 3,
    });

    assert_eq!(
        graph.describe(),
        "nodes:\n\
         \x20 Module   api:billing.models (lines 1-5)\n\
         \x20 Class    api:billing.models.Invoice (lines 3-5)\n\
         edges:\n\
         \x20 Contains api:billing.models -> api:billing.models.Invoice\n\
         references:\n\
         \x20 Inherits api:billing.models.Invoice -> billing.base.Model (line 3)"
    );
}

#[test]
fn describe_says_none_for_empty_sections() {
    let graph = FileGraph::module_only(&source("web", "main.ts"), 1);
    assert_eq!(
        graph.describe(),
        "nodes:\n  Module   web:main (lines 1-1)\nedges: none\nreferences: none"
    );
}

#[test]
fn describe_prints_variables() {
    use super::Node;

    let mut graph = FileGraph::module_only(&source("api", "m.py"), 1);
    graph.nodes.push(Node {
        id: "api:m.x".to_owned(),
        kind: NodeKind::Variable,
        name: "x".to_owned(),
        span: Span {
            start_line: 1,
            end_line: 1,
        },
    });
    assert!(
        graph
            .describe()
            .contains("\n  Variable api:m.x (lines 1-1)\n")
    );
}

#[test]
fn module_only_graphs_carry_the_disambiguator() {
    let file = SourceFile {
        project: "web",
        path: "a.js",
        src_is_package: false,
        disambiguator: Some("a.js"),
    };
    let graph = FileGraph::module_only(&file, 1);
    assert_eq!(graph.module, "web:a@a.js");
    assert_eq!(graph.nodes[0].id, "web:a@a.js");
}
