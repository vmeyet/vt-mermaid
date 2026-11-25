//! The flowchart as mermaid-rs-renderer parses and lays it out: the only module that sees the crate.

use std::collections::{BTreeMap, HashMap, HashSet};

use mermaid_rs_renderer::{self as mmdr, DiagramKind, EdgeStyle, Graph, LayoutConfig, NodeShape, Theme};
use vt_canvas::{Direction, LineKind, Shape};

use super::label::Label;
use super::shape;
use crate::error::{Error, MAX_EDGES, MAX_NODES};

/// Pixels of a column once mmdr measures a stand-in label: `n` is 0.512 em at its 16 px measuring size.
pub(super) const PX_PER_COLUMN: f32 = 8.192;
/// Pixels of a row: one label line, 1.5 times the same 16 px.
pub(super) const PX_PER_ROW: f32 = 24.0;

/// What mmdr knows of a flowchart, its positions in layout pixels and its text already cut into lines.
#[derive(Debug, Clone)]
pub(super) struct Diagram {
    pub(super) nodes: Vec<Node>,
    pub(super) groups: Vec<Group>,
    pub(super) edges: Vec<Edge>,
    pub(super) flow: Direction,
}

#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) id: String,
    pub(super) label: Label,
    pub(super) shape: Shape,
    /// Top-left corner in layout pixels.
    pub(super) x: f32,
    pub(super) y: f32,
    /// The innermost group holding it.
    pub(super) group: Option<usize>,
}

/// A subgraph.
#[derive(Debug, Clone)]
pub(super) struct Group {
    pub(super) id: String,
    pub(super) title: Label,
    pub(super) parent: Option<usize>,
    /// The way its ranks go: its own direction, else its parent's.
    pub(super) flow: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum End {
    Node(usize),
    Group(usize),
}

#[derive(Debug, Clone)]
pub(super) struct Edge {
    pub(super) from: End,
    pub(super) to: End,
    pub(super) label: Option<Label>,
    pub(super) kind: LineKind,
    pub(super) arrow_start: bool,
    pub(super) arrow_end: bool,
}

impl Diagram {
    /// Parses and lays out `source`, refusing anything but a flowchart within [`MAX_NODES`] and [`MAX_EDGES`].
    pub(super) fn read(source: &str) -> Result<Self, Error> {
        Ok(Self::new(&parse(source)?))
    }

    /// Like [`read`](Self::read), its ranks going down whatever the source says: a left-to-right flowchart too wide
    /// for the terminal often fits once turned.
    pub(super) fn read_top_down(source: &str) -> Result<Self, Error> {
        let graph = parse(source)?;
        Ok(Self::new(&Graph { direction: mmdr::Direction::TopDown, ..graph }))
    }

    pub(super) fn is_horizontal(&self) -> bool {
        matches!(self.flow, Direction::Left | Direction::Right)
    }

    fn new(graph: &Graph) -> Self {
        let labels: HashMap<&str, (Label, Shape)> =
            graph.nodes.iter().map(|(id, node)| (id.as_str(), (Label::wrapped(&node.label), shape_of(node.shape)))).collect();
        let positions = layout(graph, &labels);
        let nodes = nodes(graph, &labels, &positions);
        let groups = groups(graph, &nodes);
        let nodes = with_innermost_groups(nodes, graph, &groups);
        let edges = edges(graph, &nodes, &groups);
        Self { nodes, groups, edges, flow: direction(graph.direction) }
    }

    /// The direction ranks go in the group holding `end`.
    pub(super) fn flow_around(&self, end: End) -> Direction {
        let container = match end {
            End::Node(node) => self.nodes[node].group,
            End::Group(group) => self.groups[group].parent,
        };
        container.map_or(self.flow, |group| self.groups[group].flow)
    }

    pub(super) fn name(&self, end: End) -> &str {
        match end {
            End::Node(node) => &self.nodes[node].id,
            End::Group(group) => &self.groups[group].id,
        }
    }
}

fn parse(source: &str) -> Result<Graph, Error> {
    let graph = mmdr::parse_mermaid(source).map_err(|error| Error::Invalid { reason: error.to_string() })?.graph;
    check_bounds(&graph)?;
    Ok(graph)
}

fn check_bounds(graph: &Graph) -> Result<(), Error> {
    if graph.kind != DiagramKind::Flowchart {
        return Err(Error::Unsupported);
    }
    let nodes = graph.nodes.len() + graph.subgraphs.len();
    if nodes > MAX_NODES {
        return Err(Error::TooManyNodes { nodes });
    }
    if graph.edges.len() > MAX_EDGES {
        return Err(Error::TooManyEdges { edges: graph.edges.len() });
    }
    Ok(())
}

/// mmdr reads both `((text))` and `(((text)))` as a double circle; the single circle is the one people write.
fn shape_of(shape: NodeShape) -> Shape {
    match shape {
        NodeShape::RoundRect => Shape::Rounded,
        NodeShape::Stadium => Shape::Stadium,
        NodeShape::Circle | NodeShape::DoubleCircle => Shape::Circle,
        NodeShape::Subroutine => Shape::Subroutine,
        NodeShape::Cylinder => Shape::Cylinder,
        NodeShape::Diamond => Shape::Diamond,
        NodeShape::Hexagon => Shape::Hexagon,
        NodeShape::Parallelogram => Shape::LeanRight,
        NodeShape::ParallelogramAlt => Shape::LeanLeft,
        NodeShape::Trapezoid => Shape::Trapezoid,
        NodeShape::TrapezoidAlt => Shape::InvertedTrapezoid,
        NodeShape::Asymmetric => Shape::Asymmetric,
        _ => Shape::Rectangle,
    }
}

fn direction(direction: mmdr::Direction) -> Direction {
    match direction {
        mmdr::Direction::TopDown => Direction::Down,
        mmdr::Direction::BottomTop => Direction::Up,
        mmdr::Direction::LeftRight => Direction::Right,
        mmdr::Direction::RightLeft => Direction::Left,
    }
}

/// Top-left corners of the visible nodes, in layout pixels.
///
/// mmdr lays out a stand-in graph whose boxes are the nodes' cell sizes in `n`s, so its pixels map to cells and it
/// never measures text with the system fonts, which would make the drawing depend on the machine.
fn layout(graph: &Graph, labels: &HashMap<&str, (Label, Shape)>) -> BTreeMap<String, (f32, f32)> {
    let mut stand_in = graph.clone();
    // mmdr adds a node for an edge end it does not know, as a subgraph's name: add it first, to give it a stand-in.
    let unknown: Vec<String> =
        graph.edges.iter().flat_map(|edge| [&edge.from, &edge.to]).filter(|id| !graph.nodes.contains_key(*id)).cloned().collect();
    for id in unknown {
        stand_in.ensure_node(&id, None, None);
    }
    for (id, node) in &mut stand_in.nodes {
        let (label, shape) = label_and_shape(labels, id);
        node.label = stand_in_node(&label, shape);
        node.shape = NodeShape::Rectangle;
    }
    for edge in &mut stand_in.edges {
        edge.label = edge.label.as_deref().map(|text| stand_in_lines(Label::single_line(text).width, 1));
        (edge.start_label, edge.end_label) = (None, None);
    }
    for subgraph in &mut stand_in.subgraphs {
        subgraph.label = stand_in_lines(Label::single_line(&subgraph.label).width, 1);
    }
    let layout = mmdr::compute_layout(&stand_in, &Theme::modern(), &config(graph.direction));
    layout.nodes.into_iter().filter(|(_, node)| !node.hidden).map(|(id, node)| (id, (node.x, node.y))).collect()
}

/// A node mmdr added for an edge end is a box named after its id.
fn label_and_shape(labels: &HashMap<&str, (Label, Shape)>, id: &str) -> (Label, Shape) {
    labels.get(id).cloned().unwrap_or_else(|| (Label::wrapped(id), Shape::Rectangle))
}

/// The `n`s filling the space inside a node's border.
fn stand_in_node(label: &Label, shape: Shape) -> String {
    let size = shape::size(shape, label);
    stand_in_lines(size.width.saturating_sub(4), size.height.saturating_sub(2))
}

fn stand_in_lines(columns: u16, rows: u16) -> String {
    vec!["n".repeat(usize::from(columns)); usize::from(rows.max(1))].join("<br/>")
}

/// Padding of one row and two columns around a label, gaps between nodes as wide as the snap's.
fn config(direction: mmdr::Direction) -> LayoutConfig {
    let vertical = matches!(direction, mmdr::Direction::TopDown | mmdr::Direction::BottomTop);
    let (rank_spacing, node_spacing) = if vertical { (3.0 * PX_PER_ROW, 4.0 * PX_PER_COLUMN) } else { (6.0 * PX_PER_COLUMN, PX_PER_ROW) };
    LayoutConfig {
        fast_text_metrics: true,
        max_label_width_chars: 1_000,
        node_padding_x: 2.0 * PX_PER_COLUMN,
        node_padding_y: PX_PER_ROW,
        rank_spacing,
        node_spacing,
        ..LayoutConfig::default()
    }
}

fn nodes(graph: &Graph, labels: &HashMap<&str, (Label, Shape)>, positions: &BTreeMap<String, (f32, f32)>) -> Vec<Node> {
    let anchors: HashSet<&str> = graph.subgraphs.iter().filter_map(|subgraph| anchor(subgraph, positions)).collect();
    positions
        .iter()
        .filter(|(id, _)| !anchors.contains(id.as_str()))
        .map(|(id, &(x, y))| {
            let (label, shape) = label_and_shape(labels, id);
            Node { id: id.clone(), label, shape, x, y, group: None }
        })
        .collect()
}

/// The node mmdr adds for a subgraph an edge names, as `subgraph_anchor_id` picks it.
fn anchor<'a>(subgraph: &'a mmdr::Subgraph, positions: &BTreeMap<String, (f32, f32)>) -> Option<&'a str> {
    [subgraph.id.as_deref(), Some(subgraph.label.as_str())]
        .into_iter()
        .flatten()
        .find(|id| positions.contains_key(*id) && !subgraph.nodes.iter().any(|member| member == id))
}

/// A group's parent is the smallest other group holding all its nodes, and more of them or its name.
fn groups(graph: &Graph, nodes: &[Node]) -> Vec<Group> {
    let visible: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let members: Vec<HashSet<&str>> = graph
        .subgraphs
        .iter()
        .map(|subgraph| subgraph.nodes.iter().map(String::as_str).filter(|id| visible.contains(id)).collect())
        .collect();
    let ids: Vec<String> = graph.subgraphs.iter().map(|subgraph| subgraph.id.clone().unwrap_or_else(|| subgraph.label.clone())).collect();
    let holds = |outer: usize, inner: usize| {
        outer != inner
            && members[inner].is_subset(&members[outer])
            && (members[inner].len() < members[outer].len() || graph.subgraphs[outer].nodes.contains(&ids[inner]))
    };
    let parents: Vec<Option<usize>> = (0..graph.subgraphs.len())
        .map(|inner| (0..graph.subgraphs.len()).filter(|&outer| holds(outer, inner)).min_by_key(|&outer| (members[outer].len(), outer)))
        .collect();
    let flows = flows(graph, &parents);
    graph
        .subgraphs
        .iter()
        .zip(ids)
        .enumerate()
        .map(|(index, (subgraph, id))| Group { id, title: Label::single_line(&subgraph.label), parent: parents[index], flow: flows[index] })
        .collect()
}

fn flows(graph: &Graph, parents: &[Option<usize>]) -> Vec<Direction> {
    let flow_of = |start: usize| {
        let mut group = Some(start);
        let mut hops = 0;
        while let Some(index) = group
            && hops <= parents.len()
        {
            if let Some(own) = graph.subgraphs[index].direction {
                return direction(own);
            }
            group = parents[index];
            hops += 1;
        }
        direction(graph.direction)
    };
    (0..parents.len()).map(flow_of).collect()
}

/// The deepest group listing a node holds it.
fn with_innermost_groups(nodes: Vec<Node>, graph: &Graph, groups: &[Group]) -> Vec<Node> {
    let depth = |group: usize| std::iter::successors(Some(group), |&index| groups[index].parent).take(groups.len() + 1).count();
    nodes
        .into_iter()
        .map(|node| {
            let group =
                (0..groups.len()).filter(|&index| graph.subgraphs[index].nodes.contains(&node.id)).max_by_key(|&index| depth(index));
            Node { group, ..node }
        })
        .collect()
}

fn edges(graph: &Graph, nodes: &[Node], groups: &[Group]) -> Vec<Edge> {
    let end = |id: &str| {
        nodes
            .iter()
            .position(|node| node.id == id)
            .map(End::Node)
            .or_else(|| groups.iter().position(|group| group.id == id).map(End::Group))
            .or_else(|| graph.subgraphs.iter().position(|subgraph| subgraph.label == id).map(End::Group))
    };
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some(Edge {
                from: end(&edge.from)?,
                to: end(&edge.to)?,
                label: edge.label.as_deref().map(Label::single_line),
                kind: match edge.style {
                    EdgeStyle::Solid => LineKind::Light,
                    EdgeStyle::Dotted => LineKind::Dashed,
                    EdgeStyle::Thick => LineKind::Heavy,
                },
                arrow_start: edge.arrow_start,
                arrow_end: edge.arrow_end,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(source: &str) -> Graph {
        mmdr::parse_mermaid(source).unwrap().graph
    }

    fn node<'a>(diagram: &'a Diagram, id: &str) -> &'a Node {
        diagram.nodes.iter().find(|node| node.id == id).unwrap()
    }

    #[test]
    fn a_column_of_layout_pixels_is_one_label_character() {
        let diagram = Diagram::read("flowchart TB\n  A[nnnnnnnnnn]\n  B[nnnnnnnnnn]\n  C --> A & B").unwrap();

        let (a, b) = (node(&diagram, "A"), node(&diagram, "B"));
        let between = (b.x - a.x).abs() / PX_PER_COLUMN - 14.0;

        assert!((between - 4.0).abs() < 0.5, "{between} columns between siblings");
    }

    #[test]
    fn a_row_of_layout_pixels_is_one_label_line() {
        let rank_distance = |label: &str| {
            let diagram = Diagram::read(&format!("flowchart TB\n  A[{label}] --> B")).unwrap();
            (node(&diagram, "B").y - node(&diagram, "A").y) / PX_PER_ROW
        };

        let two_more_lines = rank_distance("one<br>two<br>three") - rank_distance("one");

        assert!((two_more_lines - 2.0).abs() < 0.1, "{two_more_lines} rows for two more lines");
    }

    #[test]
    fn a_nested_group_has_its_parent_and_inherits_its_direction() {
        let source = "flowchart LR\n  subgraph Outer\n    direction TB\n    subgraph Inner\n      A --> B\n    end\n    C\n  end";

        let diagram = Diagram::read(source).unwrap();

        let groups: Vec<(&str, Option<usize>, Direction)> =
            diagram.groups.iter().map(|group| (group.id.as_str(), group.parent, group.flow)).collect();
        let inner = groups.iter().position(|(id, ..)| *id == "Inner").unwrap();
        let outer = groups.iter().position(|(id, ..)| *id == "Outer").unwrap();
        assert_eq!(groups[inner].1, Some(outer));
        assert_eq!((groups[inner].2, groups[outer].2), (Direction::Down, Direction::Down));
        assert_eq!((node(&diagram, "A").group, node(&diagram, "C").group), (Some(inner), Some(outer)));
    }

    #[test]
    fn an_edge_naming_a_group_ends_on_the_group() {
        let diagram = Diagram::read("flowchart LR\n  subgraph one\n    A\n  end\n  B --> one").unwrap();

        assert_eq!(diagram.edges[0].to, End::Group(0));
        assert!(diagram.nodes.iter().all(|node| node.id != "one"));
    }

    #[test]
    fn edge_styles_become_line_kinds_and_arrows() {
        let diagram = Diagram::read("flowchart LR\n  A --> B\n  B -.- C\n  C <==> D").unwrap();

        let edges: Vec<(LineKind, bool, bool)> = diagram.edges.iter().map(|edge| (edge.kind, edge.arrow_start, edge.arrow_end)).collect();
        assert_eq!(edges, [(LineKind::Light, false, true), (LineKind::Dashed, false, false), (LineKind::Heavy, true, true)]);
    }

    #[test]
    fn a_diagram_of_another_kind_is_unsupported() {
        assert!(matches!(Diagram::read("pie\n  \"a\" : 1"), Err(Error::Unsupported)));
    }

    #[test]
    fn text_without_a_diagram_header_is_invalid() {
        assert!(matches!(Diagram::read("not a diagram"), Err(Error::Invalid { .. })));
    }

    #[test]
    fn a_flowchart_at_the_node_limit_is_accepted() {
        let source = format!("flowchart LR\n{}", (0..MAX_NODES).map(|index| format!("  n{index}\n")).collect::<String>());

        assert_eq!(check_bounds(&graph(&source)), Ok(()));
    }

    #[test]
    fn a_flowchart_over_the_node_limit_is_refused() {
        let source = format!("flowchart LR\n{}", (0..=MAX_NODES).map(|index| format!("  n{index}\n")).collect::<String>());

        assert_eq!(check_bounds(&graph(&source)), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn a_flowchart_at_the_edge_limit_is_accepted() {
        let source = format!("flowchart LR\n{}", "  A --> B\n".repeat(MAX_EDGES));

        assert_eq!(check_bounds(&graph(&source)), Ok(()));
    }

    #[test]
    fn a_flowchart_over_the_edge_limit_is_refused() {
        let source = format!("flowchart LR\n{}", "  A --> B\n".repeat(MAX_EDGES + 1));

        assert_eq!(check_bounds(&graph(&source)), Err(Error::TooManyEdges { edges: MAX_EDGES + 1 }));
    }
}
