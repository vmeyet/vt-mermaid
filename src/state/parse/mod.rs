//! State diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand,
//! so a state diagram means here what it means to mermaid.

mod lexer;

use std::fmt;

use crate::error::{Error, MAX_SOURCE_BYTES};
use crate::source;

/// Why mermaid would refuse a state diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// The source does not open with `stateDiagram`.
    NoDiagram,
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// `state` names a composite state with more than one word.
    StateName { name: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiagram => write!(f, "no diagram type matches the text"),
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::StateName { name } => write!(f, "a state name must be a single word, found {name:?}"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}

/// Checks the state diagram in `source` reads as mermaid's lexer reads it.
pub(super) fn parse(source: &str) -> Result<(), Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let text = source::prepare(source).text;
    if !source::opens_with(&text, "stateDiagram") {
        return Err(Invalid::NoDiagram.into());
    }
    lexer::tokens(&text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("stateDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(parse(&source), Err(Error::TooLong { bytes: source.len() }));
    }

    #[test]
    fn a_source_without_a_state_header_is_invalid() {
        assert!(matches!(parse("flowchart LR\n  A --> B"), Err(Error::Invalid { .. })));
    }
}
