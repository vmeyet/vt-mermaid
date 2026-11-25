//! Flowcharts read as mermaid 12.3.0 reads them: its preparation of the text, its lexer, its grammar and its diagram
//! db, rewritten by hand, so a flowchart means here what it means to mermaid.

#[cfg(test)]
mod agreement;
mod db;
mod grammar;
mod lexer;
mod link;
mod prepare;
mod shapes;
mod unicode;

use std::fmt;

use crate::source;
pub(super) use crate::source::is_space;

pub(super) use db::{Edge, Flowchart, Subgraph};
pub(super) use lexer::Direction;
pub(super) use link::{Arrow, Stroke};

use crate::error::{Error, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};

/// Longest source, most nodes, subgraphs included, and most edges read before the flowchart is refused.
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

/// Why mermaid would refuse a flowchart; lines are counted as mermaid counts them in its own errors, once the front
/// matter, directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// The source opens no diagram mermaid knows.
    NoDiagram,
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// The grammar allows no such token there.
    Unexpected { line: usize, found: String },
    /// A `subgraph` with neither id nor title.
    UntitledSubgraph,
    /// A block names a shape mermaid does not have.
    Shape { name: String },
    /// A block is not the YAML mermaid reads.
    ShapeData,
    /// A `linkStyle` names an edge that is not there.
    LinkStyle { index: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiagram => write!(f, "no diagram type matches the text"),
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Unexpected { line, found } => write!(f, "unexpected {found} on line {line}"),
            Self::UntitledSubgraph => write!(f, "a subgraph has neither an id nor a title"),
            Self::Shape { name } => write!(f, "no such shape: {name}"),
            Self::ShapeData => write!(f, "a node's @{{ }} block is not valid YAML"),
            Self::LinkStyle { index } => write!(f, "linkStyle {index} names no edge"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}

/// The flowchart in `source`, refused as mermaid refuses it or once it holds more than `limits` allow.
pub(super) fn parse(source: &str, limits: Limits) -> Result<Flowchart, Error> {
    if source.len() > limits.bytes {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let prepared = prepare::prepare(source);
    let text = prepared.text;
    if !source::opens_with(&text, "graph") && !source::opens_with(&text, "flowchart") {
        return Err(Invalid::NoDiagram.into());
    }
    let tokens = lexer::tokens(&text)?;
    grammar::parse(&text, &tokens, limits, prepared.config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(count: usize) -> String {
        format!("flowchart LR\n{}", (0..count).map(|index| format!("  n{index}\n")).collect::<String>())
    }

    #[test]
    fn a_flowchart_at_the_node_limit_is_read() {
        assert_eq!(parse(&nodes(MAX_NODES), Limits::DRAWN).map(|flowchart| flowchart.nodes.len()), Ok(MAX_NODES));
    }

    #[test]
    fn a_flowchart_over_the_node_limit_is_refused() {
        assert_eq!(parse(&nodes(MAX_NODES + 1), Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn subgraphs_count_towards_the_node_limit() {
        let source = format!("{}subgraph s\nend\n", nodes(MAX_NODES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn a_flowchart_at_the_edge_limit_is_read() {
        let source = format!("flowchart LR\n{}", "  A --> B\n  A ~~~ B\n".repeat(MAX_EDGES / 2));

        assert_eq!(parse(&source, Limits::DRAWN).map(|flowchart| flowchart.edges.len()), Ok(MAX_EDGES));
    }

    #[test]
    fn invisible_links_count_towards_the_edge_limit() {
        let source = format!("flowchart LR\n  A --> B\n{}", "  A ~~~ B\n".repeat(MAX_EDGES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyEdges { edges: MAX_EDGES + 1 }));
    }

    #[test]
    fn edges_between_groups_are_counted_before_they_are_made() {
        let group = (0..100).map(|index| format!("n{index}")).collect::<Vec<_>>().join(" & ");
        let source = format!("flowchart LR\n  {group} --> {group}\n");

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyEdges { edges: 100 * 100 }));
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("flowchart LR\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooLong { bytes: source.len() }));
    }

    #[test]
    fn a_source_without_a_flowchart_header_is_invalid() {
        assert!(matches!(parse("sequenceDiagram\n  A->>B: hi", Limits::DRAWN), Err(Error::Invalid { .. })));
    }
}
