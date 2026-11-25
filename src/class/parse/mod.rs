//! Class diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand, so
//! a class diagram means here what it means to mermaid.

mod db;
mod lexer;
mod member;

use std::fmt;

use crate::error::{Error, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};

/// Longest source, most blocks (classes, lollipops, notes and namespaces) and most edges (relations and the links of
/// notes) read before the diagram is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    pub(super) bytes: usize,
    pub(super) nodes: usize,
    pub(super) edges: usize,
}

impl Limits {
    /// What `render` draws.
    pub(super) const DRAWN: Self = Self { bytes: MAX_SOURCE_BYTES, nodes: MAX_NODES, edges: MAX_EDGES };
}

/// Why mermaid would refuse a class diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// An annotation, or a namespace once read, names a class that is not there.
    NoClass { id: String },
    /// A method's text has no `(` after its name.
    Method { text: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::NoClass { id } => write!(f, "there is no class {id}"),
            Self::Method { text } => write!(f, "the method {text} has no parameters"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}
