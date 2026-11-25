//! A graph as a diagram kind reads it: nodes, groups, edges and invisible links, laid out and drawn alike whatever
//! the kind.

use vt_canvas::{Direction, LineKind, Marker, Shape, Tip};

use crate::label::{Label, Wrap};

/// A graph as a diagram kind reads it, its text already cut into lines.
#[derive(Debug, Clone)]
pub(crate) struct Diagram {
    pub(crate) nodes: Vec<Node>,
    pub(crate) groups: Vec<Group>,
    pub(crate) edges: Vec<Edge>,
    /// Links that shape the layout and are never drawn (a flowchart's `A ~~~ B`).
    pub(crate) invisible: Vec<Invisible>,
    pub(crate) flow: Direction,
    /// How its nodes' text is cut into lines.
    pub(crate) wrap: Wrap,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) id: String,
    /// Its text as the source gives it, before it is cut into lines.
    pub(crate) text: String,
    pub(crate) label: Label,
    /// Lines under its label, each list in a compartment of its own below a rule, left-aligned: a class's
    /// attributes and methods, an entity's attributes.
    pub(crate) compartments: Vec<Vec<String>>,
    pub(crate) figure: Figure,
    /// The innermost group holding it.
    pub(crate) group: Option<usize>,
    /// Its place among the nodes in the order the source first names them.
    pub(crate) order: usize,
}

/// What a node is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Figure {
    /// An outline around its label.
    Shape(Shape),
    /// One cell, its label left out: every edge joins that cell, from any side.
    Marker(Marker),
    /// A heavy bar across the flow of the group holding it, `length` cells long, its label left out.
    Bar { length: u16 },
}

impl Figure {
    /// Whether the node draws its label.
    pub(crate) fn shows_label(self) -> bool {
        matches!(self, Self::Shape(_))
    }
}

/// A group of nodes framed together: a subgraph, a composite state.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub(crate) id: String,
    pub(crate) title: Label,
    pub(crate) parent: Option<usize>,
    /// The way its ranks go: its own direction, else its parent's.
    pub(crate) flow: Direction,
    pub(crate) kind: GroupKind,
}

/// How a group shows what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupKind {
    /// A frame with its title: a subgraph, a composite state.
    Framed,
    /// A concurrent region of the group holding it: no frame, a dashed line between it and the regions beside it.
    Region,
    /// No frame and no room around: it only keeps its blocks together, as a state and its notes.
    Bare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Node(usize),
    Group(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct Edge {
    pub(crate) from: End,
    pub(crate) to: End,
    pub(crate) label: Option<Label>,
    /// One line of text drawn beside it near its source, and near its target: a class relation's cardinalities.
    pub(crate) start_label: Option<Label>,
    pub(crate) end_label: Option<Label>,
    pub(crate) kind: LineKind,
    /// What it draws where it leaves its source, and where it meets its target.
    pub(crate) start: Head,
    pub(crate) end: Head,
}

/// What an edge draws where it meets a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Head {
    /// The line runs on to the node's border.
    None,
    /// An arrow on the cell next to the node, a flowchart's `-->`.
    Arrow,
    /// Tips on the cells next to the node, the nearest first: a class relation's head, a crow's foot.
    #[expect(dead_code, reason = "class diagrams draw tips once drawn")]
    Tips(&'static [Tip]),
}

impl Head {
    /// Cells next to the node the edge runs straight through, so its tips stand on a straight line: none for an
    /// arrow, which may stand where the line turns.
    pub(crate) const fn straight(self) -> u16 {
        match self {
            Self::Tips(tips) => tips.len() as u16,
            Self::None | Self::Arrow => 0,
        }
    }
}

/// An invisible link between two nodes or groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invisible {
    pub(crate) from: End,
    pub(crate) to: End,
}

impl Diagram {
    /// Its nodes' text cut into lines again, as `wrap` says.
    pub(crate) fn wrapped(&self, wrap: Wrap) -> Self {
        let nodes = self.nodes.iter().map(|node| Node { label: node.label_wrapped_as(wrap), ..node.clone() }).collect();
        Self { nodes, wrap, ..self.clone() }
    }

    /// Its ranks and those of its groups going down where they go sideways.
    pub(crate) fn turned_top_down(&self) -> Self {
        let turned = |flow: Direction| if matches!(flow, Direction::Left | Direction::Right) { Direction::Down } else { flow };
        let groups = self.groups.iter().map(|group| Group { flow: turned(group.flow), ..group.clone() }).collect();
        Self { groups, flow: turned(self.flow), ..self.clone() }
    }

    /// Whether its ranks, or those of one of its groups, go left or right.
    pub(crate) fn flows_sideways(&self) -> bool {
        let sideways = |flow: Direction| matches!(flow, Direction::Left | Direction::Right);
        sideways(self.flow) || self.groups.iter().any(|group| sideways(group.flow))
    }

    /// The direction ranks go in the group holding `end`.
    pub(crate) fn flow_around(&self, end: End) -> Direction {
        let container = match end {
            End::Node(node) => self.nodes[node].group,
            End::Group(group) => self.groups[group].parent,
        };
        container.map_or(self.flow, |group| self.groups[group].flow)
    }

    /// The direction ranks go in the innermost group holding both ends, which is where an edge between them is
    /// laid out.
    pub(crate) fn flow_between(&self, from: End, to: End) -> Direction {
        let around_to = self.holders(to);
        self.holders(from).into_iter().find(|group| around_to.contains(group)).map_or(self.flow, |group| self.groups[group].flow)
    }

    /// The groups holding `end`, the innermost first.
    fn holders(&self, end: End) -> Vec<usize> {
        let container = match end {
            End::Node(node) => self.nodes[node].group,
            End::Group(group) => self.groups[group].parent,
        };
        std::iter::successors(container, |&group| self.groups[group].parent).take(self.groups.len() + 1).collect()
    }

    pub(crate) fn name(&self, end: End) -> &str {
        match end {
            End::Node(node) => &self.nodes[node].id,
            End::Group(group) => &self.groups[group].id,
        }
    }
}

impl Node {
    /// Its text cut into lines as `wrap` says, none when its figure leaves its label out.
    fn label_wrapped_as(&self, wrap: Wrap) -> Label {
        if self.figure.shows_label() { Label::wrapped_as(&self.text, wrap) } else { Label::default() }
    }
}
