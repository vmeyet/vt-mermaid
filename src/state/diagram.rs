//! The nodes and edges mermaid's state renderer gets, turned into the graph drawn: states, dots, choices and bars as
//! nodes, composite states as framed groups.

use std::collections::HashMap;

use vt_canvas::{Direction, LineKind, Marker, Shape};

use super::parse::{self, Limits, Shape as Kind};
use crate::error::Error;
use crate::graph::diagram::{Diagram, Edge, End, Figure, Group, Head, Node};
use crate::label::{Label, Wrap};
use crate::settings::LongWords;
use crate::source::entity;

/// Cells of a fork or a join's bar at the least: room for a junction each side of the middle.
const SHORTEST_BAR: u16 = 5;

/// Reads `source`, refusing anything but a state diagram within the limits `render` draws.
pub(super) fn read(source: &str) -> Result<Diagram, Error> {
    Ok(graph(&parse::parse(source, Limits::DRAWN)?))
}

/// What each db node becomes in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Becomes {
    Node(usize),
    Group(usize),
    Nothing,
}

/// Notes are left out for now, and so are their groups and links.
fn graph(state: &parse::Diagram) -> Diagram {
    let mut becomes: Vec<Becomes> = Vec::with_capacity(state.nodes.len());
    let (mut nodes, mut groups) = (Vec::new(), Vec::new());
    for node in &state.nodes {
        becomes.push(match node.shape {
            Some(Kind::NoteGroup | Kind::Note) => Becomes::Nothing,
            Some(Kind::Divider) if !node.group => Becomes::Nothing,
            _ if node.group => {
                groups.push(group(node));
                Becomes::Group(groups.len() - 1)
            }
            shape => {
                nodes.push(self::node(node, shape, nodes.len(), bar_length(state, &node.id)));
                Becomes::Node(nodes.len() - 1)
            }
        });
    }
    let index: HashMap<&str, Becomes> = state.nodes.iter().map(|node| node.id.as_str()).zip(becomes.iter().copied()).collect();
    for (node, becomes) in state.nodes.iter().zip(&becomes) {
        let parent = node.parent.as_deref().and_then(|parent| match index.get(parent)? {
            Becomes::Group(group) => Some(*group),
            Becomes::Node(_) | Becomes::Nothing => None,
        });
        match *becomes {
            Becomes::Node(index) => nodes[index].group = parent,
            Becomes::Group(index) => groups[index].parent = parent,
            Becomes::Nothing => {}
        }
    }
    let end = |id: &str| match index.get(id)? {
        Becomes::Node(node) => Some(End::Node(*node)),
        Becomes::Group(group) => Some(End::Group(*group)),
        Becomes::Nothing => None,
    };
    let edges = state.edges.iter().filter_map(|edge| Some(self::edge(end(&edge.start)?, end(&edge.end)?, edge))).collect();
    Diagram { nodes, groups, edges, invisible: Vec::new(), flow: direction(state.direction), wrap: Wrap::wide(LongWords::Break) }
}

fn group(node: &parse::Node) -> Group {
    let flow = node.direction.map_or(Direction::Down, direction);
    Group { id: node.id.clone(), title: Label::single_line(&entity::decoded(&node.label)), parent: None, flow }
}

/// A state is a rectangle with its descriptions as lines under its label.
fn node(node: &parse::Node, shape: Option<Kind>, order: usize, bar: u16) -> Node {
    let figure = match shape {
        Some(Kind::Start) => Figure::Marker(Marker::Dot),
        Some(Kind::End) => Figure::Marker(Marker::RingedDot),
        Some(Kind::Choice) => Figure::Marker(Marker::Diamond),
        Some(Kind::Fork | Kind::Join) => Figure::Bar { length: bar },
        _ => Figure::Shape(Shape::Rectangle),
    };
    let text = std::iter::once(&node.label).chain(&node.descriptions).map(|line| entity::decoded(line)).collect::<Vec<_>>().join("<br>");
    let label = if figure.shows_label() { Label::wrapped(&text) } else { Label::default() };
    Node { id: node.id.clone(), text, label, compartments: Vec::new(), figure, group: None, order }
}

/// A bar has a cell for each edge on its busier side, two apart, and a cell past them at each end.
fn bar_length(state: &parse::Diagram, id: &str) -> u16 {
    let arriving = state.edges.iter().filter(|edge| edge.end == id).count();
    let leaving = state.edges.iter().filter(|edge| edge.start == id).count();
    u16::try_from(2 * arriving.max(leaving) + 1).unwrap_or(u16::MAX).max(SHORTEST_BAR)
}

/// A transition points at its target.
fn edge(from: End, to: End, edge: &parse::Edge) -> Edge {
    let shown = entity::decoded(&edge.label);
    Edge {
        from,
        to,
        label: (!shown.is_empty()).then(|| Label::single_line(&shown)),
        start_label: None,
        end_label: None,
        kind: LineKind::Light,
        start: Head::None,
        end: Head::Arrow,
    }
}

fn direction(direction: parse::Direction) -> Direction {
    match direction {
        parse::Direction::TB => Direction::Down,
        parse::Direction::BT => Direction::Up,
        parse::Direction::LR => Direction::Right,
        parse::Direction::RL => Direction::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(source: &str) -> Diagram {
        super::read(source).unwrap()
    }

    fn figures(diagram: &Diagram) -> Vec<(&str, Figure)> {
        diagram.nodes.iter().map(|node| (node.id.as_str(), node.figure)).collect()
    }

    #[test]
    fn starts_ends_choices_and_bars_are_drawn_without_their_label() {
        let diagram = read("stateDiagram-v2\nstate c <<choice>>\nstate f <<fork>>\n[*] --> c\nc --> f\nf --> A\nf --> B\nA --> [*]\n");

        assert_eq!(
            figures(&diagram),
            [
                ("c", Figure::Marker(Marker::Diamond)),
                ("f", Figure::Bar { length: SHORTEST_BAR }),
                ("root_start", Figure::Marker(Marker::Dot)),
                ("A", Figure::Shape(Shape::Rectangle)),
                ("B", Figure::Shape(Shape::Rectangle)),
                ("root_end", Figure::Marker(Marker::RingedDot)),
            ]
        );
        assert!(diagram.nodes.iter().filter(|node| !node.figure.shows_label()).all(|node| node.label.lines.is_empty()));
    }

    #[test]
    fn a_bar_has_a_cell_for_each_edge_of_its_busier_side() {
        let diagram = read("stateDiagram-v2\nstate j <<join>>\nA --> j\nB --> j\nC --> j\nj --> D\n");

        assert_eq!(diagram.nodes[0].figure, Figure::Bar { length: 7 });
    }

    #[test]
    fn a_transition_naming_a_composite_state_ends_on_its_frame() {
        let diagram = read("stateDiagram-v2\n[*] --> A\nstate A {\n  B\n}\n");

        assert_eq!(diagram.edges[0].to, End::Group(0));
    }

    #[test]
    fn descriptions_are_lines_under_the_label_and_entities_are_shown() {
        let diagram = read("stateDiagram-v2\nA : one #amp; two\nA : three\n");

        assert_eq!(diagram.nodes[0].label.lines, ["one & two", "three"]);
    }
}
