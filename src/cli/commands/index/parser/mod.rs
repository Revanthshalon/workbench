//! Turning a file's contents into graph nodes.
//!
//! Only a placeholder exists for now: it checks that a file is text. A real parser
//! (tree-sitter for Python and TypeScript) implements the same trait.

use std::path::Path;

/// The result of parsing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    Parsed,
    /// Parsed, but part of the file could not be understood. Not a failure.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only a real parser reports partial files")
    )]
    Partial(String),
    /// The file could not be parsed at all. Counts as a failure.
    Failed(String),
}

pub trait Parser {
    fn parse(&self, path: &Path, contents: &[u8]) -> ParseOutcome;
}

/// Placeholder parser: a file parses when it is valid UTF-8 text.
#[derive(Debug)]
pub struct TextParser;

impl Parser for TextParser {
    fn parse(&self, _path: &Path, contents: &[u8]) -> ParseOutcome {
        match std::str::from_utf8(contents) {
            Ok(_) => ParseOutcome::Parsed,
            Err(error) => ParseOutcome::Failed(format!("not valid UTF-8 text: {error}")),
        }
    }
}

pub mod python;
