//! The nodes and edges mermaid's state renderer gets, turned into the graph drawn: states, dots, choices and bars as
//! nodes, composite states as framed groups, their concurrent regions as groups split by dashed lines, and each state
//! with notes in a bare group with them, laid out beside it.

use std::collections::HashMap;

use vt_canvas::{Direction, LineKind, Marker, Shape};

use super::parse::{self, Limits, Shape as Kind};
use crate::error::Error;
use crate::graph::diagram::{Diagram, Edge, End, Figure, Group, GroupKind, Head, Invisible, Node, without_cycles};
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
    /// A note's group, standing for the bare group of the state the note is about.
    NoteGroup,
    Nothing,
}

fn graph(state: &parse::Diagram) -> Diagram {
    let notes = noted_states(state);
    let mut becomes: Vec<Becomes> = Vec::with_capacity(state.nodes.len());
    let (mut nodes, mut groups) = (Vec::new(), Vec::new());
    for node in &state.nodes {
        becomes.push(match node.shape {
            Some(Kind::NoteGroup) => Becomes::NoteGroup,
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
    let mut bare: HashMap<&str, usize> = HashMap::new();
    for id in state.nodes.iter().map(|node| node.id.as_str()).filter(|id| notes.contains_key(id)) {
        bare.insert(id, groups.len());
        groups.push(Group {
            id: format!("{id} notes"),
            title: Label::default(),
            parent: None,
            flow: Direction::Right,
            kind: GroupKind::Bare,
        });
    }
    let parent_of = |node: &parse::Node| -> Option<usize> {
        let parent = node.parent.as_deref()?;
        match index.get(parent)? {
            Becomes::Group(group) => Some(*group),
            Becomes::NoteGroup => {
                notes.iter().find(|(_, held)| held.contains(&node.id.as_str())).and_then(|(state, _)| bare.get(state).copied())
            }
            Becomes::Node(_) | Becomes::Nothing => None,
        }
    };
    for (node, becomes) in state.nodes.iter().zip(&becomes) {
        let parent = parent_of(node);
        let held = bare.get(node.id.as_str()).copied();
        match *becomes {
            Becomes::Node(index) => nodes[index].group = held.or(parent),
            Becomes::Group(index) => groups[index].parent = held.or(parent),
            Becomes::NoteGroup | Becomes::Nothing => {}
        }
        if let Some(held) = held {
            groups[held].parent = parent;
        }
    }
    let parents = without_cycles(groups.iter().map(|group| group.parent).collect());
    let groups: Vec<Group> = groups.into_iter().zip(parents).map(|(group, parent)| Group { parent, ..group }).collect();
    let end = |id: &str| match index.get(id)? {
        Becomes::Node(node) => Some(End::Node(*node)),
        Becomes::Group(group) => Some(End::Group(*group)),
        Becomes::NoteGroup | Becomes::Nothing => None,
    };
    let edges = state.edges.iter().filter_map(|edge| Some(self::edge(end(&edge.start)?, end(&edge.end)?, edge))).collect();
    let invisible = regions_in_turn(&groups);
    Diagram { nodes, groups, edges, invisible, flow: direction(state.direction), wrap: Wrap::wide(LongWords::Break) }
}

/// Each concurrent region linked to the next of its composite state, unseen, so they follow one another along its
/// flow and each divider runs across the whole frame.
fn regions_in_turn(groups: &[Group]) -> Vec<Invisible> {
    let regions: Vec<usize> = (0..groups.len()).filter(|&index| groups[index].kind == GroupKind::Region).collect();
    regions
        .iter()
        .enumerate()
        .filter_map(|(at, &region)| {
            let next = regions[at + 1..].iter().find(|&&next| groups[next].parent == groups[region].parent)?;
            Some(Invisible { from: End::Group(region), to: End::Group(*next) })
        })
        .collect()
}

/// Each state with notes, and the ids of its notes, from the links between them.
fn noted_states(state: &parse::Diagram) -> HashMap<&str, Vec<&str>> {
    let is_note = |id: &str| state.nodes.iter().any(|node| node.id == id && node.shape == Some(Kind::Note));
    let mut notes: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in state.edges.iter().filter(|edge| edge.note) {
        let (state, note) = if is_note(&edge.start) { (&edge.end, &edge.start) } else { (&edge.start, &edge.end) };
        notes.entry(state.as_str()).or_default().push(note.as_str());
    }
    notes
}

fn group(node: &parse::Node) -> Group {
    let kind = if node.shape == Some(Kind::Divider) { GroupKind::Region } else { GroupKind::Framed };
    let title = if kind == GroupKind::Framed { Label::single_line(&entity::decoded(&node.label)) } else { Label::default() };
    let flow = node.direction.map_or(Direction::Down, direction);
    Group { id: node.id.clone(), title, parent: None, flow, kind }
}

/// A state is a rectangle, its descriptions under its label below a rule, as mermaid draws them; a note is rounded.
fn node(node: &parse::Node, shape: Option<Kind>, order: usize, bar: u16) -> Node {
    let figure = match shape {
        Some(Kind::Start) => Figure::Marker(Marker::Dot),
        Some(Kind::End) => Figure::Marker(Marker::RingedDot),
        Some(Kind::Choice) => Figure::Marker(Marker::Diamond),
        Some(Kind::Fork | Kind::Join) => Figure::Bar { length: bar },
        Some(Kind::Note) => Figure::Shape(Shape::Rounded),
        _ => Figure::Shape(Shape::Rectangle),
    };
    let text = entity::decoded(&node.label);
    let label = if figure.shows_label() { Label::wrapped(&text) } else { Label::default() };
    let descriptions: Vec<String> = node.descriptions.iter().flat_map(|line| Label::wrapped(&entity::decoded(line)).lines).collect();
    let compartments = if descriptions.is_empty() || !figure.shows_label() { Vec::new() } else { vec![descriptions] };
    Node { id: node.id.clone(), text, label, compartments, figure, group: None, order }
}

/// A bar has a cell for each edge on its busier side, two apart, and a cell past them at each end.
fn bar_length(state: &parse::Diagram, id: &str) -> u16 {
    let arriving = state.edges.iter().filter(|edge| edge.end == id).count();
    let leaving = state.edges.iter().filter(|edge| edge.start == id).count();
    u16::try_from(2 * arriving.max(leaving) + 1).unwrap_or(u16::MAX).max(SHORTEST_BAR)
}

/// A transition points at its target; a note's link has no head and is dashed.
fn edge(from: End, to: End, edge: &parse::Edge) -> Edge {
    let shown = entity::decoded(&edge.label);
    Edge {
        from,
        to,
        label: (!shown.is_empty()).then(|| Label::single_line(&shown)),
        start_label: None,
        end_label: None,
        kind: if edge.note { LineKind::Dashed } else { LineKind::Light },
        start: Head::None,
        end: if edge.note { Head::None } else { Head::Arrow },
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
    fn a_state_with_notes_stands_with_them_in_a_bare_group_flowing_sideways() {
        let diagram = read("stateDiagram-v2\nstate Outer {\n  A --> B\n  note right of A : hi\n}\n");

        let bare = diagram.groups.iter().position(|group| group.kind == GroupKind::Bare).unwrap();
        let held: Vec<&str> = diagram.nodes.iter().filter(|node| node.group == Some(bare)).map(|node| node.id.as_str()).collect();
        assert_eq!(held, ["A", "A----note-1"]);
        assert_eq!((diagram.groups[bare].flow, diagram.groups[bare].parent), (Direction::Right, Some(0)));
        assert_eq!(diagram.edges[1].kind, LineKind::Dashed);
        assert_eq!(diagram.edges[1].end, Head::None);
    }

    #[test]
    fn concurrent_regions_follow_one_another_inside_their_composite_state() {
        let diagram = read("stateDiagram-v2\nstate A {\n  B\n  --\n  C\n  --\n  D\n}\n");

        let kinds: Vec<(GroupKind, Option<usize>)> = diagram.groups.iter().map(|group| (group.kind, group.parent)).collect();
        assert_eq!(
            kinds,
            [(GroupKind::Framed, None), (GroupKind::Region, Some(0)), (GroupKind::Region, Some(0)), (GroupKind::Region, Some(0))]
        );
        assert_eq!(
            diagram.invisible,
            [Invisible { from: End::Group(1), to: End::Group(2) }, Invisible { from: End::Group(2), to: End::Group(3) }]
        );
    }

    /// Found by trying: a composite state named again inside itself was its own parent, and never placed.
    #[test]
    fn a_composite_state_holding_itself_is_drawn() {
        for source in
            ["stateDiagram-v2\nstate A {\n  state A {\n    B\n  }\n}\n", "stateDiagram-v2\nstate A {\n  B\n}\nstate B {\n  A\n}\n"]
        {
            assert!(crate::render(source, 80).is_ok(), "{source:?}");
        }
    }

    #[test]
    fn a_transition_naming_a_composite_state_ends_on_its_frame() {
        let diagram = read("stateDiagram-v2\n[*] --> A\nstate A {\n  B\n}\n");

        assert_eq!(diagram.edges[0].to, End::Group(0));
    }

    #[test]
    fn descriptions_stand_under_the_label_and_entities_are_shown() {
        let diagram = read("stateDiagram-v2\nA : one #amp; two\nA : three\n");

        assert_eq!(
            (diagram.nodes[0].label.lines.as_slice(), diagram.nodes[0].compartments.as_slice()),
            (["one & two".to_owned()].as_slice(), [vec!["three".to_owned()]].as_slice())
        );
    }
}
