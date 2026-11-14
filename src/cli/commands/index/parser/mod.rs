//! Turning a file's contents into its code graph.
//!
//! Python files are parsed with tree-sitter ([`python::PythonParser`]). Languages
//! without a parser yet use [`TextParser`], which only checks that the file is text
//! and returns a graph with just the file's module node.

use crate::graph::{FileGraph, SourceFile};

/// The tree-sitter parser for Python.
pub mod python;

/// The result of parsing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    Parsed(FileGraph),
    /// Parsed, but part of the file could not be understood. Not a failure: the
    /// graph holds everything outside the broken parts, and the text says where they
    /// are.
    Partial(FileGraph, String),
    /// The file could not be parsed at all. Counts as a failure.
    Failed(String),
}

/// Parses one file of a project into its graph.
pub trait Parser {
    /// Node ids include `file.project`. The returned graph's hash is left for the
    /// caller to set.
    fn parse(&self, file: &SourceFile<'_>, contents: &[u8]) -> ParseOutcome;
}

/// For languages without a parser: a file parses when it is valid UTF-8 text.
#[derive(Debug)]
pub struct TextParser;

impl Parser for TextParser {
    fn parse(&self, file: &SourceFile<'_>, contents: &[u8]) -> ParseOutcome {
        match std::str::from_utf8(contents) {
            Ok(text) => ParseOutcome::Parsed(FileGraph::module_only(file, text.lines().count())),
            Err(error) => ParseOutcome::Failed(format!("not valid UTF-8 text: {error}")),
        }
    }
}
