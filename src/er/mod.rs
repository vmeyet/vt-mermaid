//! ER diagrams: entities as tables, laid out in layers like a flowchart.

#[cfg_attr(not(test), expect(dead_code, reason = "read by the fuzzer until ER diagrams are drawn"))]
mod parse;

#[cfg(feature = "fuzz")]
use crate::error::Error;

/// How much an ER diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Entities and subgraphs.
    pub nodes: usize,
    /// Relationships.
    pub edges: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let diagram = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: diagram.entities.len() + diagram.subgraphs.len(), edges: diagram.relationships.len() })
}
