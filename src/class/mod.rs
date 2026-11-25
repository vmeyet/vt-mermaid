//! Class diagrams: classes as boxes of compartments, laid out in layers like a flowchart.

#[cfg_attr(not(test), expect(dead_code, reason = "read by the fuzzer until class diagrams are drawn"))]
mod parse;

#[cfg(feature = "fuzz")]
use crate::error::Error;

/// How much a class diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Classes, lollipops, notes and namespaces.
    pub nodes: usize,
    /// Relations.
    pub edges: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn read(source: &str) -> Result<Counts, Error> {
    let diagram = parse::parse(source, parse::Limits::DRAWN)?;
    let nodes = diagram.classes.len() + diagram.interfaces.len() + diagram.notes.len() + diagram.namespaces.len();
    Ok(Counts { nodes, edges: diagram.relations.len() })
}
