//! A graph as a diagram kind reads it: nodes, groups, edges and invisible links, laid out and drawn alike whatever
//! the kind.

use vt_canvas::{Direction, LineKind, Shape, Tip};

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
    pub(crate) shape: Shape,
    /// The innermost group holding it.
    pub(crate) group: Option<usize>,
    /// Its place among the nodes in the order the source first names them.
    pub(crate) order: usize,
}

/// A group of nodes framed together: a subgraph, a composite state.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub(crate) id: String,
    pub(crate) title: Label,
    pub(crate) parent: Option<usize>,
    /// The way its ranks go: its own direction, else its parent's.
    pub(crate) flow: Direction,
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

/// An invisible link between two nodes or groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invisible {
    pub(crate) from: End,
    pub(crate) to: End,
}

impl Diagram {
    /// Its nodes' text cut into lines again, as `wrap` says.
    pub(crate) fn wrapped(&self, wrap: Wrap) -> Self {
        let nodes = self.nodes.iter().map(|node| Node { label: Label::wrapped_as(&node.text, wrap), ..node.clone() }).collect();
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
