/// What a cell of a diagram is drawn for; the caller maps each role to a colour.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Role {
    /// Empty cells and anything without a role of its own.
    #[default]
    Plain,
    /// The border of a node, a participant's box or figure.
    Node,
    /// A line between nodes, a lifeline or a message, and its tips.
    Edge,
    /// Text: a node's name, an edge's label, a message.
    Label,
    /// The frame around a group of nodes, a sequence block, a note, and its title.
    Group,
}

impl vt_canvas::Role for Role {}
