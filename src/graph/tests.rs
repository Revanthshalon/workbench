use super::{FileGraph, NodeKind, Span, module_name, node_id};

#[test]
fn module_names_follow_python_packages() {
    assert_eq!(module_name("billing/invoices.py"), "billing.invoices");
    assert_eq!(module_name("billing/__init__.py"), "billing");
    assert_eq!(module_name("src/billing/invoices.py"), "billing.invoices");
    assert_eq!(module_name("app.py"), "app");
    assert_eq!(module_name("pkg/types.pyi"), "pkg.types");
    // Only a leading `src/` is the "src layout".
    assert_eq!(module_name("tools/src/x.py"), "tools.src.x");
}

#[test]
fn root_init_files_are_named_init() {
    assert_eq!(module_name("__init__.py"), "__init__");
    assert_eq!(module_name("src/__init__.py"), "__init__");
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
    let graph = FileGraph::module_only("web", "src/main.ts", 0);
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
