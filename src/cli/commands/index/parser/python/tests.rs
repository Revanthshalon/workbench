use std::collections::HashSet;

use super::{PythonParse, PythonParser, module_graph, parse_python_checked};
use crate::cli::commands::index::parser::{ParseOutcome, Parser};
use crate::graph::{FileGraph, NodeKind, ReferenceKind, SourceFile, Span};

/// Parses a snippet; tree-sitter never gives up on these, but if it did the graph
/// would hold only the module node.
fn parse_file(file: &SourceFile<'_>, source: &str) -> PythonParse {
    parse_python_checked(file, source).unwrap_or_else(|| PythonParse {
        graph: module_graph(file, source),
        error_lines: Vec::new(),
    })
}

/// Parses a snippet in a project whose `src/` is not a package.
fn parse_python(project: &str, path: &str, source: &str) -> PythonParse {
    parse_file(
        &SourceFile {
            project,
            path,
            src_is_package: false,
            disambiguator: None,
            modules: None,
        },
        source,
    )
}

fn plain<'a>(project: &'a str, path: &'a str) -> SourceFile<'a> {
    SourceFile {
        project,
        path,
        src_is_package: false,
        disambiguator: None,
        modules: None,
    }
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

fn inherits_targets(graph: &FileGraph) -> Vec<&str> {
    graph
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::Inherits)
        .map(|reference| reference.target.as_str())
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
        PythonParser.parse(&plain("api", "m.py"), source.as_bytes())
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
        PythonParser.parse(&plain("api", "m.py"), &[0xff, 0xfe]),
        ParseOutcome::Failed(details) if details.starts_with("not valid UTF-8")
    ));
}

#[test]
fn clean_files_parse_fully() {
    assert!(matches!(
        PythonParser.parse(&plain("api", "m.py"), b"x = 1\n"),
        ParseOutcome::Parsed(_)
    ));
}

#[test]
fn very_long_expressions_do_not_overflow_the_stack() {
    let source = format!("x = 1{}\n\ndef after(): ...\n", " + 1".repeat(50_000));
    let graph = parse_python("api", "generated.py", &source).graph;
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.id == "api:generated.after"),
        "the walk stopped before `after`"
    );
}

#[test]
fn relative_imports_to_exactly_above_the_top_keep_their_dots() {
    let graph = parse_python("api", "a/b.py", "from .. import y\nfrom ..c import z\n").graph;
    assert_eq!(targets(&graph), ["..y", "..c.z"]);
}

#[test]
fn relative_imports_in_a_root_init_resolve_from_the_top() {
    for path in ["__init__.py", "src/__init__.py"] {
        let graph = parse_python(
            "api",
            path,
            "from .y import z\nfrom . import x\nfrom .. import w\n",
        )
        .graph;
        assert_eq!(targets(&graph), ["y.z", "x", "..w"], "in {path}");
    }
}

#[test]
fn a_src_package_names_nodes_like_its_imports() {
    let file = SourceFile {
        project: "demo",
        path: "src/models/user_models.py",
        src_is_package: true,
        disambiguator: None,
        modules: None,
    };
    let source = "from src.models.main_model import MainModel\nfrom .main_model import Base\n\nclass User(MainModel): ...\nclass Admin(User): ...\n";
    let graph = parse_file(&file, source).graph;
    assert_eq!(graph.module, "demo:src.models.user_models");
    assert_eq!(graph.nodes[1].id, "demo:src.models.user_models.User");
    assert_eq!(
        targets(&graph),
        [
            "src.models.main_model.MainModel",
            "src.models.main_model.Base",
            "src.models.main_model.MainModel",
            "src.models.user_models.User",
        ]
    );
}

#[test]
fn spaces_inside_dotted_bases_are_ignored() {
    let graph = parse_python(
        "api",
        "m.py",
        "import a.b\nclass C(a . b . X, (lambda: Base)()): ...\n",
    )
    .graph;
    let inherits: Vec<_> = refs(&graph)
        .into_iter()
        .filter(|(_, _, kind)| *kind == ReferenceKind::Inherits)
        .collect();
    assert_eq!(inherits, [("api:m.C", "a.b.X", ReferenceKind::Inherits)]);
}

#[test]
fn bases_use_the_last_binding_in_source_order() {
    let source = "if A:\n    from a import Base\nfrom b import Base\nclass C(Base): ...\nfrom c import Base\n";
    assert_eq!(
        inherits_targets(&parse_python("api", "m.py", source).graph),
        ["b.Base"]
    );
}

#[test]
fn bases_defined_inside_functions_resolve_to_their_scope() {
    let source = "class A: ...\ndef f():\n    class A: ...\n    class B(A): ...\n";
    assert_eq!(
        inherits_targets(&parse_python("api", "m.py", source).graph),
        ["m.f.A"]
    );
}

#[test]
fn bases_name_the_definition_in_effect() {
    let source = "class B: ...\nclass B: ...\nclass C(B): ...\n";
    assert_eq!(
        inherits_targets(&parse_python("api", "m.py", source).graph),
        ["m.B#2"]
    );
}

#[test]
fn class_bodies_do_not_enclose_nested_functions() {
    let source = "class Outer:\n    class Inner: ...\n    def method(self):\n        class Local(Inner): ...\n";
    assert_eq!(
        inherits_targets(&parse_python("api", "m.py", source).graph),
        ["Inner"]
    );
}

#[test]
fn local_imports_bind_inside_their_function() {
    let source =
        "def f():\n    from models import Base\n    class C(Base): ...\nclass D(Base): ...\n";
    assert_eq!(
        inherits_targets(&parse_python("api", "m.py", source).graph),
        ["models.Base", "Base"]
    );
}

#[test]
fn module_level_assignments_become_variables() {
    let source = "settings = get_settings()\nMETADATA: MetaData = MetaData()\nT: TypeVar\na, (b, *rest) = 1, (2, 3)\nx = y = 0\nobj.attr = 1\nd[k] = 2\ncount += 1\ndef f():\n    local = 1\nclass C:\n    attr = 1\n";
    let graph = parse_python("api", "config.py", source).graph;
    let variables: Vec<(&str, usize)> = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Variable)
        .map(|node| (node.id.as_str(), node.span.start_line))
        .collect();
    assert_eq!(
        variables,
        [
            ("api:config.settings", 1),
            ("api:config.METADATA", 2),
            ("api:config.T", 3),
            ("api:config.a", 4),
            ("api:config.b", 4),
            ("api:config.rest", 4),
            ("api:config.x", 5),
            ("api:config.y", 5),
        ]
    );
    assert!(contains(&graph).contains(&("api:config", "api:config.settings")));
}

#[test]
fn a_variable_assigned_in_both_branches_is_one_node() {
    let graph = parse_python("api", "m.py", "if X:\n    v = 1\nelse:\n    v = 2\n").graph;
    let variables: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Variable)
        .collect();
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].span.start_line, 2);
}

#[test]
fn definitions_win_over_variables_of_the_same_name() {
    let graph = parse_python("api", "m.py", "def f(): ...\nf = wrap(f)\n").graph;
    assert_eq!(
        ids(&graph),
        [("api:m", NodeKind::Module), ("api:m.f", NodeKind::Function)]
    );
}

#[test]
fn variables_bind_names_for_bases() {
    let graph = parse_python(
        "api",
        "db.py",
        "Base = declarative_base()\nclass User(Base): ...\n",
    )
    .graph;
    assert_eq!(inherits_targets(&graph), ["db.Base"]);
}

#[test]
fn a_disambiguated_file_suffixes_every_id_with_its_path() {
    let file = SourceFile {
        project: "api",
        path: "pkg/x.pyi",
        src_is_package: false,
        disambiguator: Some("pkg/x.pyi"),
        modules: None,
    };
    let graph = parse_file(&file, "class Foo: ...\nv = 1\n").graph;
    assert_eq!(graph.module, "api:pkg.x@pkg/x.pyi");
    assert_eq!(graph.nodes[0].id, "api:pkg.x@pkg/x.pyi");
    assert_eq!(graph.nodes[1].id, "api:pkg.x.Foo@pkg/x.pyi");
    assert_eq!(graph.nodes[2].id, "api:pkg.x.v@pkg/x.pyi");
}

#[test]
fn very_long_dotted_bases_do_not_overflow_the_stack() {
    let dotted = format!("a{}", ".b".repeat(50_000));
    let graph = parse_python("api", "m.py", &format!("class C({dotted}): ...\n")).graph;
    assert_eq!(inherits_targets(&graph), [dotted.as_str()]);
}

#[test]
fn module_level_names_equal_to_a_submodule_get_the_path_suffix() {
    let modules: HashSet<String> = ["pkg", "pkg.settings", "pkg.Loader"]
        .map(str::to_owned)
        .into();
    let file = SourceFile {
        project: "api",
        path: "pkg/__init__.py",
        src_is_package: false,
        disambiguator: None,
        modules: Some(&modules),
    };
    let source = "settings = load()\nclass Loader:\n    def run(self): ...\nclass Other(Loader): ...\nother = 1\n";
    let graph = parse_file(&file, source).graph;
    let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "api:pkg",
            "api:pkg.Loader@pkg/__init__.py",
            "api:pkg.Loader@pkg/__init__.py.run",
            "api:pkg.Other",
            "api:pkg.settings@pkg/__init__.py",
            "api:pkg.other",
        ]
    );
    assert_eq!(inherits_targets(&graph), ["pkg.Loader@pkg/__init__.py"]);
}
