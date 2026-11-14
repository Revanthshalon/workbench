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
    pub fn module_only(project: &str, relative_path: &str, line_count: usize) -> Self {
        let name = module_name(relative_path);
        let module = node_id(project, &name);
        Self {
            path: relative_path.to_owned(),
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

/// The dotted module name of a file, from its `/`-separated path in the project.
///
/// The extension is dropped, `pkg/__init__.py` is the package `pkg`, and a leading
/// `src/` (the "src layout") is not part of the name. A root `__init__.py` keeps the
/// name `__init__`, so a module name is never empty.
pub fn module_name(relative_path: &str) -> String {
    let path = relative_path.strip_prefix("src/").unwrap_or(relative_path);
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
