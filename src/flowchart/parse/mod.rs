//! Flowcharts read as mermaid 12.3.0 reads them: its preparation of the text, its lexer, its grammar and its diagram
//! db, rewritten by hand, so a flowchart means here what it means to mermaid.

mod db;
mod entity;
mod lexer;
mod link;
mod prepare;
mod shape_data;
mod unicode;

use std::fmt;

pub(super) use lexer::Direction;

use crate::error::Error;

/// Longest source, most nodes, subgraphs included, and most edges read before the flowchart is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    pub(super) bytes: usize,
    pub(super) nodes: usize,
    pub(super) edges: usize,
}

/// Why mermaid would refuse a flowchart; lines are counted as mermaid counts them in its own errors, once the front
/// matter, directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// A block names a shape mermaid does not have.
    Shape { name: String },
    /// A block is not the YAML mermaid reads.
    ShapeData,
    /// A `linkStyle` names an edge that is not there.
    LinkStyle { index: String },
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Shape { name } => write!(f, "no such shape: {name}"),
            Self::ShapeData => write!(f, "a node's @{{ }} block is not valid YAML"),
            Self::LinkStyle { index } => write!(f, "linkStyle {index} names no edge"),
        }
    }
}

/// A character of a JavaScript regex `\s`, the space mermaid's patterns mean.
fn is_space(char: char) -> bool {
    matches!(
        char,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
