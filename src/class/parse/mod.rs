//! Class diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand, so
//! a class diagram means here what it means to mermaid.

mod lexer;

use std::fmt;

/// Why mermaid would refuse a class diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
        }
    }
}
