//! State diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand,
//! so a state diagram means here what it means to mermaid.

mod db;
mod grammar;
mod lexer;

use std::fmt;

pub(super) use db::Diagram;

use crate::error::{Error, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};
use crate::source;

/// Longest source, most nodes and most edges read before the diagram is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    pub(super) bytes: usize,
    /// Nodes mermaid's renderer is handed (states, start and end dots, notes and their groups, regions) and, while
    /// the source is read, each composite state's opening.
    pub(super) nodes: usize,
    /// Transitions and links between a note and its state.
    pub(super) edges: usize,
}

impl Limits {
    /// What `render` draws.
    pub(super) const DRAWN: Self = Self { bytes: MAX_SOURCE_BYTES, nodes: MAX_NODES, edges: MAX_EDGES };
}

/// Why mermaid would refuse a state diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// The source does not open with `stateDiagram`.
    NoDiagram,
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// The grammar allows no such token there.
    Unexpected { line: usize, found: String },
    /// `state` names a composite state with more than one word.
    StateName { name: String },
    /// A composite state has a description besides its label.
    GroupDescription { id: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiagram => write!(f, "no diagram type matches the text"),
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Unexpected { line, found } => write!(f, "unexpected {found} on line {line}"),
            Self::StateName { name } => write!(f, "a state name must be a single word, found {name:?}"),
            Self::GroupDescription { id } => write!(f, "the composite state {id} may only have a label"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}

/// The state diagram in `source`, refused as mermaid refuses it or once it holds more than `limits` allow.
pub(super) fn parse(source: &str, limits: Limits) -> Result<Diagram, Error> {
    if source.len() > limits.bytes {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let text = source::prepare(source).text;
    if !source::opens_with(&text, "stateDiagram") {
        return Err(Invalid::NoDiagram.into());
    }
    let tokens = lexer::tokens(&text)?;
    let root = grammar::parse(&text, &tokens, limits.nodes)?;
    db::apply(root, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(count: usize) -> String {
        format!("stateDiagram-v2\n{}", (0..count).map(|index| format!("  s{index}\n")).collect::<String>())
    }

    fn transitions(count: usize) -> String {
        format!("stateDiagram-v2\n{}", "  A --> B\n".repeat(count))
    }

    #[test]
    fn a_diagram_at_the_node_limit_is_read() {
        assert_eq!(parse(&states(MAX_NODES), Limits::DRAWN).map(|diagram| diagram.nodes.len()), Ok(MAX_NODES));
    }

    #[test]
    fn a_diagram_over_the_node_limit_is_refused() {
        assert_eq!(parse(&states(MAX_NODES + 1), Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn notes_and_their_groups_count_towards_the_node_limit() {
        let source = format!("{}note right of s0 : hi\n", states(MAX_NODES - 1));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn a_diagram_at_the_edge_limit_is_read() {
        assert_eq!(parse(&transitions(MAX_EDGES), Limits::DRAWN).map(|diagram| diagram.edges.len()), Ok(MAX_EDGES));
    }

    #[test]
    fn a_diagram_over_the_edge_limit_is_refused() {
        assert_eq!(parse(&transitions(MAX_EDGES + 1), Limits::DRAWN), Err(Error::TooManyEdges { edges: MAX_EDGES + 1 }));
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("stateDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooLong { bytes: source.len() }));
    }

    #[test]
    fn a_source_without_a_state_header_is_invalid() {
        assert!(matches!(parse("flowchart LR\n  A --> B", Limits::DRAWN), Err(Error::Invalid { .. })));
    }
}
