//! Flowcharts read as mermaid 12.3.0 reads them: its preparation of the text, its lexer, its grammar and its diagram
//! db, rewritten by hand, so a flowchart means here what it means to mermaid.

mod lexer;
mod prepare;
mod unicode;

use std::fmt;

/// Why mermaid would refuse a flowchart; lines are counted as mermaid counts them in its own errors, once the front
/// matter, directives and comment lines are left out.
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

/// A character of a JavaScript regex `\s`, the space mermaid's patterns mean.
fn is_space(char: char) -> bool {
    matches!(
        char,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
