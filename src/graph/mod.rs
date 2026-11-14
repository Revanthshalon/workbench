//! The code graph: a language-neutral model of a project's code.
//!
//! Every parser produces a [`FileGraph`] per file and every store consumes it. The
//! workspace and project nodes are not part of a file's graph: they come from the
//! workspace config, and the store links each file's module node to its project.

/// A node's id: `<project-id>:<qualified name>`, unique across the workspace.
pub type NodeId = String;

/// What a node stands for in the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Module,
    Class,
    Function,
    Method,
    /// A name assigned at module level.
    Variable,
}

/// Lines a node spans, 1-based and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
}

/// A named definition in a file: the module itself, a class, a function or a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub span: Span,
}

/// What an [`Edge`] means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// From a scope to a definition directly inside it.
    Contains,
}

/// A connection between two nodes of the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
}

/// What a [`Reference`] means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Imports,
    Inherits,
}

/// A connection to a dotted name the file alone cannot resolve to a node.
///
/// A later link step resolves it, within the project or across projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub from: NodeId,
    pub target: String,
    pub kind: ReferenceKind,
    pub line: usize,
}

/// Where a file sits: its project, its path in the project, and how the project
/// lays out its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceFile<'a> {
    pub project: &'a str,
    /// Relative to the project, with `/` separators.
    pub path: &'a str,
    /// Whether the project's top-level `src/` directory is itself a package (Python:
    /// it has an `__init__.py`). Then `src` is part of module names; otherwise it is
    /// only a folder holding the packages (the "src layout").
    pub src_is_package: bool,
    /// The file's own path when another file of the project has the same module name
    /// and wins it. Every id from this file then ends in `@<path>`.
    pub disambiguator: Option<&'a str>,
}

impl SourceFile<'_> {
    /// The file's dotted module name.
    pub fn module_name(&self) -> String {
        module_name(self.path, self.src_is_package)
    }
}

/// Everything parsed from one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGraph {
    /// Relative to the project, with `/` separators.
    pub path: String,
    /// The content hash, set by `index`.
    pub hash: u64,
    /// The id of the file's module node.
    pub module: NodeId,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub references: Vec<Reference>,
}

impl FileGraph {
    /// A graph with only the file's module node, for files whose language has no
    /// parser yet.
    pub fn module_only(file: &SourceFile<'_>, line_count: usize) -> Self {
        let name = file.module_name();
        let mut module = node_id(file.project, &name);
        if let Some(path) = file.disambiguator {
            module.push_str(&format!("@{path}"));
        }
        Self {
            path: file.path.to_owned(),
            hash: 0,
            nodes: vec![Node {
                id: module.clone(),
                kind: NodeKind::Module,
                name,
                span: Span {
                    start_line: 1,
                    end_line: line_count.max(1),
                },
            }],
            module,
            edges: Vec::new(),
            references: Vec::new(),
        }
    }
}

impl FileGraph {
    /// The graph as readable text: its nodes, edges and references, one per line.
    pub fn describe(&self) -> String {
        let nodes = self.nodes.iter().map(|node| {
            format!(
                "{:<8} {} (lines {}-{})",
                format!("{:?}", node.kind),
                node.id,
                node.span.start_line,
                node.span.end_line
            )
        });
        let edges = self.edges.iter().map(|edge| {
            format!(
                "{:<8} {} -> {}",
                format!("{:?}", edge.kind),
                edge.from,
                edge.to
            )
        });
        let references = self.references.iter().map(|reference| {
            format!(
                "{:<8} {} -> {} (line {})",
                format!("{:?}", reference.kind),
                reference.from,
                reference.target,
                reference.line
            )
        });
        [
            section("nodes", nodes),
            section("edges", edges),
            section("references", references),
        ]
        .join("\n")
    }
}

/// `title:` and its lines indented below it, or `title: none`.
fn section(title: &str, lines: impl Iterator<Item = String>) -> String {
    let lines: Vec<String> = lines.map(|line| format!("  {line}")).collect();
    if lines.is_empty() {
        format!("{title}: none")
    } else {
        format!("{title}:\n{}", lines.join("\n"))
    }
}

/// The dotted module name of a file, from its `/`-separated path in the project.
///
/// The extension is dropped and `pkg/__init__.py` is the package `pkg`. A leading
/// `src/` is part of the name only when `src_is_package`; otherwise it is the "src
/// layout" folder and is dropped. A root `__init__.py` keeps the name `__init__`, so
/// a module name is never empty.
pub fn module_name(relative_path: &str, src_is_package: bool) -> String {
    let path = if src_is_package {
        relative_path
    } else {
        relative_path.strip_prefix("src/").unwrap_or(relative_path)
    };
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if let Some(last) = parts.last_mut() {
        *last = last.rsplit_once('.').map_or(*last, |(stem, _)| stem);
    }
    if parts.len() > 1 && parts.last() == Some(&"__init__") {
        parts.pop();
    }
    parts.join(".")
}

/// The id of a node: the project id and the node's qualified name.
pub fn node_id(project: &str, qualified_name: &str) -> NodeId {
    format!("{project}:{qualified_name}")
}

#[cfg(test)]
mod tests;
