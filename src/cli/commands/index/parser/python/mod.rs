//! Python source → code graph, with tree-sitter.
//!
//! The walk keeps only definitions: one node per module, class, function and method,
//! with `Contains` edges from each scope to what is defined directly inside it.
//! Definitions inside `if`, `try`, `with` and other blocks belong to the enclosing
//! scope; only a class or a function opens a new one.
//!
//! Imports and base classes become references, resolved as far as the file alone
//! allows: relative imports against the file's package, and base classes through the
//! names bound where the class is defined. The walk keeps those bindings per scope,
//! in source order, as Python does.

use std::collections::HashMap;

use tree_sitter::{Node as Syntax, Parser as TreeSitter, Tree};

use super::{ParseOutcome, Parser};
use crate::graph::{
    Edge, EdgeKind, FileGraph, Node, NodeId, NodeKind, Reference, ReferenceKind, SourceFile, Span,
    node_id,
};

#[cfg(test)]
mod tests;

/// Stub files describe the same module as the source; their ids get this suffix so
/// the two never collide.
const STUB_EXTENSION: &str = ".pyi";
const STUB_SUFFIX: &str = "@pyi";
/// `__init__.py` (or `.pyi`) makes its directory a package.
const PACKAGE_FILE_STEM: &str = "__init__";
/// The statements a definition or import can sit inside, besides a class or function.
/// The walk only descends into these (and into tree-sitter's `ERROR` nodes), never
/// into expressions, so deeply nested expressions cannot overflow the stack.
const STATEMENT_CONTAINERS: [&str; 13] = [
    "block",
    "if_statement",
    "elif_clause",
    "else_clause",
    "try_statement",
    "except_clause",
    "finally_clause",
    "with_statement",
    "for_statement",
    "while_statement",
    "match_statement",
    "case_clause",
    "ERROR",
];

/// Parses Python files with tree-sitter.
#[derive(Debug)]
pub struct PythonParser;

impl Parser for PythonParser {
    fn parse(&self, file: &SourceFile<'_>, contents: &[u8]) -> ParseOutcome {
        let source = match std::str::from_utf8(contents) {
            Ok(source) => source,
            Err(error) => return ParseOutcome::Failed(format!("not valid UTF-8 text: {error}")),
        };
        let Some(parse) = parse_python_checked(file, source) else {
            return ParseOutcome::Failed("the parser gave up".to_owned());
        };
        if parse.error_lines.is_empty() {
            return ParseOutcome::Parsed(parse.graph);
        }
        let lines: Vec<String> = parse.error_lines.iter().map(ToString::to_string).collect();
        ParseOutcome::Partial(
            parse.graph,
            format!("syntax errors at lines {}", lines.join(", ")),
        )
    }
}

/// The graph of one Python file, and the lines tree-sitter could not parse.
#[derive(Debug)]
pub struct PythonParse {
    pub graph: FileGraph,
    pub error_lines: Vec<usize>,
}

/// Parses `source`, the contents of `file`, into its graph, or `None` when tree-sitter
/// gives up.
fn parse_python_checked(file: &SourceFile<'_>, source: &str) -> Option<PythonParse> {
    let tree = syntax_tree(source)?;
    let mut graph = module_graph(file, source);
    let module_name = graph
        .nodes
        .first()
        .map(|module| module.name.clone())
        .unwrap_or_default();
    let is_package = file
        .path
        .rsplit('/')
        .next()
        .is_some_and(|file| file.starts_with(&format!("{PACKAGE_FILE_STEM}.")));
    let module = ModuleContext {
        name: &module_name,
        package: package_parts(&module_name, is_package),
        source: source.as_bytes(),
    };
    let mut walk = Walk {
        project: file.project,
        stub: file.path.ends_with(STUB_EXTENSION),
        module: &module,
        frames: vec![Frame::default()],
        graph: &mut graph,
    };
    let mut module_scope = Scope::new(module_name.clone(), walk.graph.module.clone(), false);
    walk.visit(tree.root_node(), &mut module_scope);
    Some(PythonParse {
        graph,
        error_lines: error_lines(tree.root_node()),
    })
}

/// The graph with only the file's module node, with the stub suffix for `.pyi` files.
fn module_graph(file: &SourceFile<'_>, source: &str) -> FileGraph {
    let mut graph = FileGraph::module_only(file, source.lines().count());
    if file.path.ends_with(STUB_EXTENSION) {
        graph.module.push_str(STUB_SUFFIX);
        if let Some(module) = graph.nodes.first_mut() {
            module.id.clone_from(&graph.module);
        }
    }
    graph
}

/// The sorted 1-based lines where tree-sitter found an error or a missing token.
///
/// Only subtrees that contain an error are walked.
fn error_lines(root: Syntax<'_>) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() || node.is_missing() {
            lines.push(node.start_position().row + 1);
        }
        let mut cursor = node.walk();
        pending.extend(node.children(&mut cursor).filter(|child| child.has_error()));
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn syntax_tree(source: &str) -> Option<Tree> {
    let mut parser = TreeSitter::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

/// What the whole file shares while it is walked.
struct ModuleContext<'a> {
    /// The dotted module name, without the stub suffix.
    name: &'a str,
    /// The package relative imports resolve against.
    package: Vec<&'a str>,
    source: &'a [u8],
}

impl ModuleContext<'_> {
    fn text(&self, node: Syntax<'_>) -> String {
        node.utf8_text(self.source).unwrap_or_default().to_owned()
    }
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
    /// A reference from this scope to `target`, at the line where `at` starts.
    fn reference(&self, target: String, kind: ReferenceKind, at: Syntax<'_>) -> Reference {
        Reference {
            from: self.id.clone(),
            target,
            kind,
            line: at.start_position().row + 1,
        }
    }

    fn new(qualified_name: String, id: NodeId, is_class: bool) -> Self {
        Self {
            qualified_name,
            id,
            is_class,
            name_counts: HashMap::new(),
        }
    }
}

/// The names one scope has bound so far, mapped to the dotted names they stand for.
#[derive(Default)]
struct Frame {
    names: HashMap<String, String>,
    /// A class body: its names are not visible to functions nested inside it.
    is_class: bool,
}

struct Walk<'a> {
    project: &'a str,
    stub: bool,
    module: &'a ModuleContext<'a>,
    /// One frame per scope being walked; the first is the module.
    frames: Vec<Frame>,
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
            "import_statement" | "import_from_statement" => {
                for import in imports(child, self.module) {
                    if let Some((local, dotted)) = import.binding {
                        self.bind(local, dotted);
                    }
                    self.refer(scope.reference(import.target, ReferenceKind::Imports, child));
                }
            }
            kind if STATEMENT_CONTAINERS.contains(&kind) => self.visit(child, scope),
            _ => {}
        }
    }

    /// Adds the node for a class or function definition, then walks its body.
    fn define(&mut self, definition: Syntax<'_>, extent: Syntax<'_>, scope: &mut Scope) {
        let Some(name) = definition
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(self.module.source).ok())
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

        // Bases are evaluated before the class's own name is bound, as in Python.
        let is_class = kind == NodeKind::Class;
        let bases = if is_class {
            self.base_classes(definition)
        } else {
            Vec::new()
        };
        self.bind(name.to_owned(), qualified_name.clone());

        let mut inner = Scope::new(qualified_name, id, is_class);
        for base in bases {
            self.refer(inner.reference(base, ReferenceKind::Inherits, definition));
        }
        if let Some(body) = definition.child_by_field_name("body") {
            self.frames.push(Frame {
                names: HashMap::new(),
                is_class,
            });
            self.visit(body, &mut inner);
            self.frames.pop();
        }
    }

    /// Binds `local` in the scope being walked; a later binding replaces an earlier one.
    fn bind(&mut self, local: String, dotted: String) {
        if let Some(frame) = self.frames.last_mut() {
            frame.names.insert(local, dotted);
        }
    }

    /// What `name` stands for where the walk is: the current scope, then enclosing
    /// function scopes outwards (class bodies further out are not visible), then the
    /// module.
    fn resolve(&self, name: &str) -> Option<&str> {
        let (current, outer) = self.frames.split_last()?;
        if let Some(found) = current.names.get(name) {
            return Some(found);
        }
        outer
            .iter()
            .enumerate()
            .rev()
            .filter(|(index, frame)| *index == 0 || !frame.is_class)
            .find_map(|(_, frame)| frame.names.get(name))
            .map(String::as_str)
    }

    /// The dotted names a class inherits from, resolved through the bindings in effect.
    ///
    /// Keyword arguments such as `metaclass=M` are not bases; `Generic[T]` counts as
    /// `Generic`.
    fn base_classes(&self, class: Syntax<'_>) -> Vec<String> {
        let Some(arguments) = class.child_by_field_name("superclasses") else {
            return Vec::new();
        };
        let mut cursor = arguments.walk();
        arguments
            .named_children(&mut cursor)
            .filter_map(|argument| match argument.kind() {
                "subscript" => argument.child_by_field_name("value"),
                _ => Some(argument),
            })
            .filter_map(|base| dotted_name(base, self.module))
            .map(|dotted| {
                let (first, rest) = dotted.split_once('.').unwrap_or((&dotted, ""));
                let resolved = self.resolve(first).unwrap_or(first);
                join(resolved, rest)
            })
            .collect()
    }

    fn refer(&mut self, reference: Reference) {
        self.graph.references.push(reference);
    }
}

/// The 1-based lines a syntax node covers.
fn span(node: Syntax<'_>) -> Span {
    Span {
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
    }
}

/// One name an import statement brings in.
struct Import {
    /// The dotted name imported, resolved as far as the file allows.
    target: String,
    /// The local name it binds and the dotted name that name stands for, if any
    /// (`*` binds nothing nameable).
    binding: Option<(String, String)>,
}

/// The names an `import` or `from ... import` statement brings in, in order.
fn imports(statement: Syntax<'_>, module: &ModuleContext<'_>) -> Vec<Import> {
    let mut cursor = statement.walk();
    let names: Vec<Syntax<'_>> = statement
        .children_by_field_name("name", &mut cursor)
        .collect();
    if statement.kind() == "import_statement" {
        names
            .into_iter()
            .map(|name| plain_import(name, module))
            .collect()
    } else {
        from_imports(statement, names, module)
    }
}

/// One name of `import a.b` or `import a.b as c`.
fn plain_import(name: Syntax<'_>, module: &ModuleContext<'_>) -> Import {
    if name.kind() == "aliased_import" {
        let target = name
            .child_by_field_name("name")
            .map(|dotted| module.text(dotted))
            .unwrap_or_default();
        let binding = name
            .child_by_field_name("alias")
            .map(|alias| (module.text(alias), target.clone()));
        return Import { target, binding };
    }
    // `import a.b` binds `a`, which stands for the package `a`.
    let target = module.text(name);
    let first = target.split('.').next().unwrap_or_default().to_owned();
    Import {
        binding: Some((first.clone(), first)),
        target,
    }
}

/// The names of `from m import a, b as c` or `from m import *`.
fn from_imports(
    statement: Syntax<'_>,
    names: Vec<Syntax<'_>>,
    module: &ModuleContext<'_>,
) -> Vec<Import> {
    let base = statement
        .child_by_field_name("module_name")
        .map(|source_module| match source_module.kind() {
            "relative_import" => resolve_relative(source_module, module),
            _ => module.text(source_module),
        })
        .unwrap_or_default();
    let mut cursor = statement.walk();
    let wildcard = statement
        .named_children(&mut cursor)
        .any(|child| child.kind() == "wildcard_import");
    if wildcard {
        return vec![Import {
            target: base,
            binding: None,
        }];
    }
    names
        .into_iter()
        .map(|name| {
            let (imported, alias) = match name.kind() {
                "aliased_import" => (
                    name.child_by_field_name("name")
                        .map(|dotted| module.text(dotted))
                        .unwrap_or_default(),
                    name.child_by_field_name("alias")
                        .map(|alias| module.text(alias)),
                ),
                _ => (module.text(name), None),
            };
            let target = join(&base, &imported);
            Import {
                binding: Some((alias.unwrap_or(imported), target.clone())),
                target,
            }
        })
        .collect()
}

/// `.x`, `..`, `...c` against the file's package. Going above the top package keeps
/// the target as written, dots included.
fn resolve_relative(relative: Syntax<'_>, module: &ModuleContext<'_>) -> String {
    let mut cursor = relative.walk();
    let mut dots = 0;
    let mut rest = String::new();
    for child in relative.named_children(&mut cursor) {
        let text = module.text(child);
        match child.kind() {
            "import_prefix" => dots = text.chars().filter(|character| *character == '.').count(),
            _ => rest = text,
        }
    }
    let levels_up = dots.saturating_sub(1);
    let package = &module.package;
    // Climbing to or past the top package leaves no package to name: keep the dots.
    if levels_up > 0 && levels_up >= package.len() {
        return format!("{}{rest}", ".".repeat(dots));
    }
    join(&package[..package.len() - levels_up].join("."), &rest)
}

/// `a.b` + `c` → `a.b.c`; an empty side or a trailing dot joins without a dot.
fn join(base: &str, name: &str) -> String {
    match (base.is_empty(), name.is_empty()) {
        (true, _) => name.to_owned(),
        (_, true) => base.to_owned(),
        _ if base.ends_with('.') => format!("{base}{name}"),
        _ => format!("{base}.{name}"),
    }
}

/// The package a module's relative imports start from: the module's parent, or the
/// module itself for a package's `__init__`. A root `__init__` (named `__init__`)
/// sits at the top, so its package is empty.
fn package_parts(module_name: &str, is_package: bool) -> Vec<&str> {
    if module_name == PACKAGE_FILE_STEM {
        return Vec::new();
    }
    let mut parts: Vec<&str> = module_name
        .split('.')
        .filter(|part| !part.is_empty())
        .collect();
    if !is_package {
        parts.pop();
    }
    parts
}

/// The dotted name an `identifier` or `attribute` expression spells, built from its
/// identifiers so whitespace and comments inside it never reach the name. Any other
/// expression, such as a call, has no dotted name.
fn dotted_name(node: Syntax<'_>, module: &ModuleContext<'_>) -> Option<String> {
    match node.kind() {
        "identifier" => Some(module.text(node)),
        "attribute" => {
            let object = dotted_name(node.child_by_field_name("object")?, module)?;
            let attribute = module.text(node.child_by_field_name("attribute")?);
            Some(format!("{object}.{attribute}"))
        }
        _ => None,
    }
}
