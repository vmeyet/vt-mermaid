use std::fmt;

/// Longest source [`render`](crate::render) reads, in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Most nodes a flowchart may have.
pub const MAX_NODES: usize = 200;
/// Most edges a flowchart may have: each one is a path search over the whole grid.
pub const MAX_EDGES: usize = 150;
/// Most cells of the grid edges are routed on, margins included.
pub const MAX_AREA: usize = 250_000;

/// Why a diagram was not drawn.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The source is longer than [`MAX_SOURCE_BYTES`].
    TooLong {
        /// Its length in bytes.
        bytes: usize,
    },
    /// The source is not Mermaid the parser reads.
    Invalid {
        /// The parser's reason, untrusted: it may quote the source.
        reason: String,
    },
    /// The source is a Mermaid diagram of a kind not drawn yet.
    Unsupported,
    /// The flowchart has more than [`MAX_NODES`] nodes.
    TooManyNodes {
        /// How many it has.
        nodes: usize,
    },
    /// The flowchart has more than [`MAX_EDGES`] edges.
    TooManyEdges {
        /// How many it has.
        edges: usize,
    },
    /// The drawing would cover more than [`MAX_AREA`] cells.
    TooLarge {
        /// Its columns.
        width: usize,
        /// Its rows.
        height: usize,
    },
    /// The drawing needs more columns than the caller allows.
    TooWide {
        /// Columns the drawing needs.
        width: usize,
        /// Columns allowed.
        max_width: u16,
    },
    /// No path on the grid joins two nodes without crossing a third.
    Unroutable {
        /// The id of the node the edge leaves.
        from: String,
        /// The id of the node it points to.
        to: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { bytes } => write!(f, "the diagram is {bytes} bytes, over the {MAX_SOURCE_BYTES} byte limit"),
            Self::Invalid { reason } => write!(f, "the diagram does not parse: {reason}"),
            Self::Unsupported => write!(f, "this kind of diagram is not drawn as text yet"),
            Self::TooManyNodes { nodes } => write!(f, "the flowchart has {nodes} nodes, over the {MAX_NODES} limit"),
            Self::TooManyEdges { edges } => write!(f, "the flowchart has {edges} edges, over the {MAX_EDGES} limit"),
            Self::TooLarge { width, height } => write!(f, "a {width}x{height} drawing is over the {MAX_AREA} cell limit"),
            Self::TooWide { width, max_width } => write!(f, "the drawing needs {width} columns, more than the {max_width} allowed"),
            Self::Unroutable { from, to } => write!(f, "no path joins {from} to {to}"),
        }
    }
}

impl std::error::Error for Error {}
