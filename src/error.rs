use std::fmt;

/// Longest source [`render`](crate::render) reads, in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Most nodes a flowchart, a state, a class or an ER diagram may have.
pub const MAX_NODES: usize = 200;
/// Most edges a flowchart, a state, a class or an ER diagram may have: each one is a path search over the whole grid.
pub const MAX_EDGES: usize = 150;
/// Most cells of the grid edges are routed on, margins included.
pub const MAX_AREA: usize = 250_000;
/// Most cells a drawing may cover, width times height: a narrow drawing can be tall.
pub const MAX_CELLS: usize = vt_canvas::MAX_CELLS;
/// Most columns a drawing may take: a wider `max_width` counts as this one.
pub const MAX_WIDTH: u16 = vt_canvas::MAX_WIDTH;
/// Most participants a sequence diagram may have.
pub const MAX_PARTICIPANTS: usize = 50;
/// Most messages a sequence diagram may have, notes, activations and the starts, sections and ends of blocks included.
pub const MAX_MESSAGES: usize = 1_000;

/// Why a diagram was not drawn.
///
/// The `String` fields quote the source, which is untrusted: sanitize them before they reach a terminal.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The source is longer than [`MAX_SOURCE_BYTES`].
    #[non_exhaustive]
    TooLong {
        /// Its length in bytes.
        bytes: usize,
    },
    /// The source is not Mermaid the parser reads.
    #[non_exhaustive]
    Invalid {
        /// The parser's reason, untrusted: it may quote the source.
        reason: String,
    },
    /// The source is a Mermaid diagram of a kind not drawn yet.
    #[non_exhaustive]
    Unsupported {
        /// The word the source opens with, such as `pie` or `xychart-beta`, untrusted: at most 32 ASCII letters,
        /// digits and `-`.
        kind: String,
    },
    /// The flowchart, state, class or ER diagram has more than [`MAX_NODES`] nodes.
    #[non_exhaustive]
    TooManyNodes {
        /// Nodes read when reading stopped, at the first over the limit: a lower bound of what the source holds.
        nodes: usize,
    },
    /// The flowchart, state, class or ER diagram has more than [`MAX_EDGES`] edges.
    #[non_exhaustive]
    TooManyEdges {
        /// Edges read when reading stopped, at the first link over the limit: a lower bound of what the source holds.
        edges: usize,
    },
    /// The sequence diagram has more than [`MAX_PARTICIPANTS`] participants.
    #[non_exhaustive]
    TooManyParticipants {
        /// Participants read when reading stopped, at the first over the limit: a lower bound of what the source holds.
        participants: usize,
    },
    /// The sequence diagram has more than [`MAX_MESSAGES`] messages.
    #[non_exhaustive]
    TooManyMessages {
        /// Messages read when reading stopped, at the first over the limit: a lower bound of what the source holds.
        messages: usize,
    },
    /// The drawing would cover more cells than a limit allows: [`MAX_AREA`] on a graph's routing grid, [`MAX_CELLS`]
    /// on the canvas.
    #[non_exhaustive]
    TooLarge {
        /// Its columns.
        width: usize,
        /// Its rows.
        height: usize,
        /// The limit it is over.
        max_cells: usize,
    },
    /// The drawing needs more columns than the caller allows.
    #[non_exhaustive]
    TooWide {
        /// Columns the drawing needs.
        width: usize,
        /// Columns allowed.
        max_width: usize,
    },
    /// No path on the grid joins two nodes without crossing a third.
    #[non_exhaustive]
    Unroutable {
        /// The id of the node the edge leaves, untrusted: it quotes the source.
        from: String,
        /// The id of the node it points to, untrusted: it quotes the source.
        to: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { bytes } => write!(f, "the diagram is {bytes} bytes, over the {MAX_SOURCE_BYTES} byte limit"),
            Self::Invalid { reason } => write!(f, "the diagram does not parse: {reason}"),
            Self::Unsupported { kind } => write!(f, "{kind} diagrams are not drawn as text yet"),
            Self::TooManyNodes { nodes } => write!(f, "the diagram has over {MAX_NODES} nodes: reading stopped at {nodes}"),
            Self::TooManyEdges { edges } => write!(f, "the diagram has over {MAX_EDGES} edges: reading stopped at {edges}"),
            Self::TooManyParticipants { participants } => {
                write!(f, "the sequence diagram has over {MAX_PARTICIPANTS} participants: reading stopped at {participants}")
            }
            Self::TooManyMessages { messages } => {
                write!(f, "the sequence diagram has over {MAX_MESSAGES} messages: reading stopped at {messages}")
            }
            Self::TooLarge { width, height, max_cells } => write!(f, "a {width}x{height} drawing is over the {max_cells} cell limit"),
            Self::TooWide { width, max_width } => write!(f, "the drawing needs {width} columns, more than the {max_width} allowed"),
            Self::Unroutable { from, to } => write!(f, "no path joins {from} to {to}"),
        }
    }
}

impl std::error::Error for Error {}
