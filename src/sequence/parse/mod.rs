//! Sequence diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand,
//! so a sequence diagram means here what it means to mermaid.

mod arrow;
mod db;
mod grammar;
mod lexer;

use std::fmt;

use crate::error::Error;

/// Why mermaid would refuse a sequence diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// The grammar allows no such token there.
    Unexpected { line: usize, found: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Unexpected { line, found } => write!(f, "unexpected {found} on line {line}"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}
