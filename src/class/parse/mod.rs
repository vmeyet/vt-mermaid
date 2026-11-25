//! Class diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand, so
//! a class diagram means here what it means to mermaid.

mod db;
mod grammar;
mod lexer;
mod member;

use std::fmt;

pub(super) use db::ClassDiagram;

use crate::error::{Error, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};
use crate::source;

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
    /// The source does not open with `classDiagram`.
    NoDiagram,
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// The grammar allows no such token there.
    Unexpected { line: usize, found: String },
    /// An annotation, or a namespace once read, names a class that is not there.
    NoClass { id: String },
    /// A method's text has no `(` after its name.
    Method { text: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiagram => write!(f, "no diagram type matches the text"),
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Unexpected { line, found } => write!(f, "unexpected {found} on line {line}"),
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

/// The class diagram in `source`, refused as mermaid refuses it or once it holds more than `limits` allow.
pub(super) fn parse(source: &str, limits: Limits) -> Result<ClassDiagram, Error> {
    if source.len() > limits.bytes {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let text = source::prepare(source).text;
    if !source::opens_with(&text, "classDiagram") {
        return Err(Invalid::NoDiagram.into());
    }
    let tokens = lexer::tokens(&text)?;
    grammar::parse(&text, &tokens, db::Db::new(limits)).map(db::Db::into_diagram)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(count: usize) -> String {
        format!("classDiagram\n{}", (0..count).map(|index| format!("  class C{index}\n")).collect::<String>())
    }

    fn relations(count: usize) -> String {
        format!("classDiagram\n{}", "  A --> B\n".repeat(count))
    }

    #[test]
    fn a_diagram_at_the_node_limit_is_read() {
        assert_eq!(parse(&classes(MAX_NODES), Limits::DRAWN).map(|diagram| diagram.classes.len()), Ok(MAX_NODES));
    }

    #[test]
    fn a_diagram_over_the_node_limit_is_refused() {
        assert_eq!(parse(&classes(MAX_NODES + 1), Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn notes_and_namespaces_count_towards_the_node_limit() {
        let source = format!("{}note \"n\"\n", classes(MAX_NODES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn a_diagram_at_the_edge_limit_is_read() {
        assert_eq!(parse(&relations(MAX_EDGES), Limits::DRAWN).map(|diagram| diagram.relations.len()), Ok(MAX_EDGES));
    }

    #[test]
    fn a_note_on_a_class_counts_towards_the_edge_limit() {
        let source = format!("{}note for A \"n\"\n", relations(MAX_EDGES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyEdges { edges: MAX_EDGES + 1 }));
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("classDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooLong { bytes: source.len() }));
    }

    #[test]
    fn a_source_without_a_class_header_is_invalid() {
        assert!(matches!(parse("flowchart LR\n  A --> B", Limits::DRAWN), Err(Error::Invalid { .. })));
    }
}
