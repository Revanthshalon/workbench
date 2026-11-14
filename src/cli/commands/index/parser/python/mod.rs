//! Python source → code graph, with tree-sitter.
//!
//! The walk keeps only definitions: one node per module, class, function and method,
//! with `Contains` edges from each scope to what is defined directly inside it.
//! Definitions inside `if`, `try`, `with` and other blocks belong to the enclosing
//! scope; only a class or a function opens a new one.

use std::collections::HashMap;

use tree_sitter::{Node as Syntax, Parser as TreeSitter, Tree};

use crate::graph::{Edge, EdgeKind, FileGraph, Node, NodeId, NodeKind, Span, node_id};

#[cfg(test)]
mod tests;

/// Stub files describe the same module as the source; their ids get this suffix so
/// the two never collide.
const STUB_EXTENSION: &str = ".pyi";
const STUB_SUFFIX: &str = "@pyi";

/// The graph of one Python file, and the lines tree-sitter could not parse.
#[derive(Debug)]
pub struct PythonParse {
    pub graph: FileGraph,
    pub error_lines: Vec<usize>,
}

/// Parses `source`, the contents of `relative_path` in `project`, into its graph.
pub fn parse_python(project: &str, relative_path: &str, source: &str) -> PythonParse {
    let stub = relative_path.ends_with(STUB_EXTENSION);
    let mut graph = FileGraph::module_only(project, relative_path, source.lines().count());
    if stub {
        graph.module.push_str(STUB_SUFFIX);
        if let Some(module) = graph.nodes.first_mut() {
            module.id.clone_from(&graph.module);
        }
    }
    let module_name = graph
        .nodes
        .first()
        .map(|module| module.name.clone())
        .unwrap_or_default();

    if let Some(tree) = syntax_tree(source) {
        let mut walk = Walk {
            project,
            stub,
            source: source.as_bytes(),
            graph: &mut graph,
        };
        let mut module_scope = Scope::new(module_name, walk.graph.module.clone(), false);
        walk.visit(tree.root_node(), &mut module_scope);
    }
    PythonParse {
        graph,
        error_lines: Vec::new(),
    }
}

fn syntax_tree(source: &str) -> Option<Tree> {
    let mut parser = TreeSitter::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

/// A class, function or module whose body is being walked.
struct Scope {
    qualified_name: String,
    id: NodeId,
    is_class: bool,
    /// How often each name was defined here, for the `#2`, `#3` suffixes.
    name_counts: HashMap<String, usize>,
}

impl Scope {
    fn new(qualified_name: String, id: NodeId, is_class: bool) -> Self {
        Self {
            qualified_name,
            id,
            is_class,
            name_counts: HashMap::new(),
        }
    }
}

struct Walk<'a> {
    project: &'a str,
    stub: bool,
    source: &'a [u8],
    graph: &'a mut FileGraph,
}

impl Walk<'_> {
    /// Visits the children of `parent`, which belong to `scope`.
    fn visit(&mut self, parent: Syntax<'_>, scope: &mut Scope) {
        let mut cursor = parent.walk();
        let children: Vec<Syntax<'_>> = parent.named_children(&mut cursor).collect();
        for child in children {
            self.visit_child(child, scope);
        }
    }

    fn visit_child(&mut self, child: Syntax<'_>, scope: &mut Scope) {
        match child.kind() {
            "class_definition" | "function_definition" => self.define(child, child, scope),
            // The span starts at the first decorator.
            "decorated_definition" => match child.child_by_field_name("definition") {
                Some(definition) => self.define(definition, child, scope),
                None => self.visit(child, scope),
            },
            _ => self.visit(child, scope),
        }
    }

    /// Adds the node for a class or function definition, then walks its body.
    fn define(&mut self, definition: Syntax<'_>, extent: Syntax<'_>, scope: &mut Scope) {
        let Some(name) = definition
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(self.source).ok())
        else {
            return;
        };
        let kind = match (definition.kind(), scope.is_class) {
            ("class_definition", _) => NodeKind::Class,
            (_, true) => NodeKind::Method,
            (_, false) => NodeKind::Function,
        };

        let count = scope.name_counts.entry(name.to_owned()).or_insert(0);
        *count += 1;
        let suffix = if *count > 1 {
            format!("#{count}")
        } else {
            String::new()
        };
        let qualified_name = format!("{}.{name}{suffix}", scope.qualified_name);
        let mut id = node_id(self.project, &qualified_name);
        if self.stub {
            id.push_str(STUB_SUFFIX);
        }

        self.graph.nodes.push(Node {
            id: id.clone(),
            kind,
            name: name.to_owned(),
            span: span(extent),
        });
        self.graph.edges.push(Edge {
            from: scope.id.clone(),
            to: id.clone(),
            kind: EdgeKind::Contains,
        });

        if let Some(body) = definition.child_by_field_name("body") {
            let mut inner = Scope::new(qualified_name, id, kind == NodeKind::Class);
            self.visit(body, &mut inner);
        }
    }
}

/// The 1-based lines a syntax node covers.
fn span(node: Syntax<'_>) -> Span {
    Span {
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
    }
}
