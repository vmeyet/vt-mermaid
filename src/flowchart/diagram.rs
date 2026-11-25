//! The flowchart as mermaid-rs-renderer parses and lays it out: the only module that sees the crate.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

use mermaid_rs_renderer::{self as mmdr, DiagramKind, EdgeStyle, Graph, LayoutConfig, NodeShape, Theme};
use vt_canvas::{Direction, LineKind, Shape};

use super::attributes;
use super::invisible::{self, Link};
use super::label::Label;
use super::shape;
use super::source;
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
    /// Links that shape the layout and are never drawn (`A ~~~ B`).
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the layered layout, not drawn with yet"))]
    pub(super) invisible: Vec<Invisible>,
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
    /// Its place among the nodes in the order the source first names them.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the layered layout, not drawn with yet"))]
    pub(super) order: usize,
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

/// An invisible link between two nodes or groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Invisible {
    pub(super) from: End,
    pub(super) to: End,
}

/// mmdr's graph, the invisible links it cannot read, which shape the layout and are never drawn, and the shapes
/// attribute blocks name, which it cannot read either.
#[derive(Debug, Clone)]
struct Parsed {
    graph: Graph,
    invisible: Vec<InvisibleLink>,
    shapes: HashMap<String, Shape>,
}

#[derive(Debug, Clone)]
struct InvisibleLink {
    from: String,
    to: String,
}

impl Diagram {
    /// Parses and lays out `source`, refusing anything but a flowchart within [`MAX_NODES`] and [`MAX_EDGES`].
    pub(super) fn read(source: &str) -> Result<Self, Error> {
        let parsed = parse(source)?;
        let positions = layout(&parsed, &labels(&parsed));
        Ok(Self::new(&parsed, &positions))
    }

    /// Parses `source` like [`read`](Self::read) and leaves every node at the origin, for a layout that needs no
    /// pixels.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the layered layout, not drawn with yet"))]
    pub(super) fn parsed(source: &str) -> Result<Self, Error> {
        let parsed = parse(source)?;
        Ok(Self::new(&parsed, &unplaced(&parsed.graph)))
    }

    /// Its ranks and those of its groups going down where they go sideways.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the layered layout, not drawn with yet"))]
    pub(super) fn turned_top_down(&self) -> Self {
        let turned = |flow: Direction| if matches!(flow, Direction::Left | Direction::Right) { Direction::Down } else { flow };
        let groups = self.groups.iter().map(|group| Group { flow: turned(group.flow), ..group.clone() }).collect();
        Self { groups, flow: turned(self.flow), ..self.clone() }
    }

    /// Like [`read`](Self::read), its ranks and those of its groups going down where the source says sideways: a
    /// left-to-right flowchart too wide for the terminal often fits once turned.
    pub(super) fn read_top_down(source: &str) -> Result<Self, Error> {
        let parsed = parse(source)?;
        let subgraphs =
            parsed.graph.subgraphs.into_iter().map(|subgraph| mmdr::Subgraph { direction: subgraph.direction.map(top_down), ..subgraph });
        let graph = Graph { direction: top_down(parsed.graph.direction), subgraphs: subgraphs.collect(), ..parsed.graph };
        let parsed = Parsed { graph, ..parsed };
        let positions = layout(&parsed, &labels(&parsed));
        Ok(Self::new(&parsed, &positions))
    }

    /// Whether its ranks, or those of one of its groups, go left or right.
    pub(super) fn flows_sideways(&self) -> bool {
        let sideways = |flow: Direction| matches!(flow, Direction::Left | Direction::Right);
        sideways(self.flow) || self.groups.iter().any(|group| sideways(group.flow))
    }

    fn new(parsed: &Parsed, positions: &BTreeMap<String, (f32, f32)>) -> Self {
        let graph = &parsed.graph;
        let nodes = nodes(graph, &labels(parsed), positions);
        let groups = groups(graph, &nodes);
        let nodes = with_innermost_groups(nodes, graph, &groups);
        let ends = Ends { graph, nodes: &nodes, groups: &groups };
        let invisible = parsed
            .invisible
            .iter()
            .filter_map(|link| Some(Invisible { from: ends.named(&link.from)?, to: ends.named(&link.to)? }))
            .collect();
        Self { edges: edges(&ends), invisible, flow: direction(graph.direction), nodes, groups }
    }

    /// The direction ranks go in the group holding `end`.
    pub(super) fn flow_around(&self, end: End) -> Direction {
        let container = match end {
            End::Node(node) => self.nodes[node].group,
            End::Group(group) => self.groups[group].parent,
        };
        container.map_or(self.flow, |group| self.groups[group].flow)
    }

    /// The direction ranks go in the innermost group holding both ends, which is where an edge between them is
    /// laid out.
    pub(super) fn flow_between(&self, from: End, to: End) -> Direction {
        let around_to = self.holders(to);
        self.holders(from).into_iter().find(|group| around_to.contains(group)).map_or(self.flow, |group| self.groups[group].flow)
    }

    /// How many group frames an edge between the ends crosses.
    pub(super) fn frames_between(&self, from: End, to: End) -> usize {
        let (around_from, around_to) = (self.holders(from), self.holders(to));
        let only = |groups: &[usize], others: &[usize]| groups.iter().filter(|group| !others.contains(group)).count();
        only(&around_from, &around_to) + only(&around_to, &around_from)
    }

    /// The groups holding `end`, the innermost first.
    fn holders(&self, end: End) -> Vec<usize> {
        let container = match end {
            End::Node(node) => self.nodes[node].group,
            End::Group(group) => self.groups[group].parent,
        };
        std::iter::successors(container, |&group| self.groups[group].parent).take(self.groups.len() + 1).collect()
    }

    pub(super) fn name(&self, end: End) -> &str {
        match end {
            End::Node(node) => &self.nodes[node].id,
            End::Group(group) => &self.groups[group].id,
        }
    }
}

/// Each node's label cut into lines and its shape, the one its attribute block names first.
fn labels(parsed: &Parsed) -> HashMap<&str, (Label, Shape)> {
    let shape = |id: &str, node: &mmdr::Node| parsed.shapes.get(id).copied().unwrap_or_else(|| shape_of(node.shape));
    parsed.graph.nodes.iter().map(|(id, node)| (id.as_str(), (Label::wrapped(&node.label), shape(id, node)))).collect()
}

/// Every node the layout would place, at the origin: the graph's own and the ends edges name without declaring.
#[cfg_attr(not(test), expect(dead_code, reason = "read by the layered layout, not drawn with yet"))]
fn unplaced(graph: &Graph) -> BTreeMap<String, (f32, f32)> {
    let ends = graph.edges.iter().flat_map(|edge| [&edge.from, &edge.to]);
    graph.nodes.keys().chain(ends).map(|id| (id.clone(), (0.0, 0.0))).collect()
}

/// Rewrites what mmdr misreads before it reads the source, then finds the nodes each invisible link joins.
fn parse(source: &str) -> Result<Parsed, Error> {
    let rewritten = attributes::rewrite(&source::with_direction(source));
    let split = invisible::split(&rewritten.source);
    let graph = mmdr::parse_mermaid(&split.source).map_err(|error| Error::Invalid { reason: error.to_string() })?.graph;
    check_bounds(&graph, split.links.len())?;
    let invisible = split
        .links
        .iter()
        .filter_map(ends)
        .filter(|link| graph.nodes.contains_key(&link.from) && graph.nodes.contains_key(&link.to))
        .collect();
    Ok(Parsed { graph, invisible, shapes: rewritten.shapes })
}

/// The last node of the statement before the link and the first node of the one after, as mmdr reads each alone.
fn ends(link: &Link) -> Option<InvisibleLink> {
    let statement = |text: &str| mmdr::parse_mermaid(&format!("flowchart TD\n{text}")).ok().map(|parsed| parsed.graph);
    let (before, after) = (statement(&link.before)?, statement(&link.after)?);
    let from = before.edges.last().map(|edge| edge.to.clone()).or_else(|| before.nodes.keys().next().cloned())?;
    let to = after.edges.first().map(|edge| edge.from.clone()).or_else(|| after.nodes.keys().next().cloned())?;
    Some(InvisibleLink { from, to })
}

/// Invisible links count as edges: mmdr lays them out.
fn check_bounds(graph: &Graph, invisible: usize) -> Result<(), Error> {
    if graph.kind != DiagramKind::Flowchart {
        return Err(Error::Unsupported);
    }
    let nodes = graph.nodes.len() + graph.subgraphs.len();
    if nodes > MAX_NODES {
        return Err(Error::TooManyNodes { nodes });
    }
    let edges = graph.edges.len() + invisible;
    if edges > MAX_EDGES {
        return Err(Error::TooManyEdges { edges });
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

fn top_down(direction: mmdr::Direction) -> mmdr::Direction {
    match direction {
        mmdr::Direction::LeftRight | mmdr::Direction::RightLeft => mmdr::Direction::TopDown,
        vertical => vertical,
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
fn layout(parsed: &Parsed, labels: &HashMap<&str, (Label, Shape)>) -> BTreeMap<String, (f32, f32)> {
    let graph = &parsed.graph;
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
    stand_in.edges.extend(parsed.invisible.iter().map(stand_in_edge));
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

fn stand_in_edge(link: &InvisibleLink) -> mmdr::Edge {
    mmdr::Edge {
        from: link.from.clone(),
        to: link.to.clone(),
        label: None,
        start_label: None,
        end_label: None,
        directed: true,
        arrow_start: false,
        arrow_end: false,
        arrow_start_kind: None,
        arrow_end_kind: None,
        start_decoration: None,
        end_decoration: None,
        style: EdgeStyle::Solid,
    }
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
            let order = graph.node_order.get(id).copied().unwrap_or(usize::MAX);
            Node { id: id.clone(), label, shape, x, y, group: None, order }
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

/// A group's parent is the smallest other group holding all its nodes, and more of them or its name; of two groups
/// holding the same nodes, the one the source opens first holds the other.
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
            && (members[inner].len() < members[outer].len()
                || graph.subgraphs[outer].nodes.contains(&ids[inner])
                || (outer < inner && !members[inner].is_empty()))
    };
    let parents = without_cycles(
        (0..graph.subgraphs.len())
            .map(|inner| {
                (0..graph.subgraphs.len()).filter(|&outer| holds(outer, inner)).min_by_key(|&outer| (members[outer].len(), Reverse(outer)))
            })
            .collect(),
    );
    let flows = flows(graph, &parents);
    graph
        .subgraphs
        .iter()
        .zip(ids)
        .enumerate()
        .map(|(index, (subgraph, id))| Group { id, title: Label::single_line(&subgraph.label), parent: parents[index], flow: flows[index] })
        .collect()
}

/// Two subgraphs sharing a name can each hold the other; a group in a cycle would never be placed, so the first of
/// each cycle loses its parent.
fn without_cycles(parents: Vec<Option<usize>>) -> Vec<Option<usize>> {
    let mut parents = parents;
    for start in 0..parents.len() {
        let in_cycle = std::iter::successors(parents[start], |&index| parents[index]).take(parents.len()).any(|index| index == start);
        if in_cycle {
            parents[start] = None;
        }
    }
    parents
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

/// What an edge's end may name: a node, a group's id or its title.
#[derive(Debug, Clone, Copy)]
struct Ends<'a> {
    graph: &'a Graph,
    nodes: &'a [Node],
    groups: &'a [Group],
}

impl Ends<'_> {
    fn named(&self, id: &str) -> Option<End> {
        self.nodes
            .iter()
            .position(|node| node.id == id)
            .map(End::Node)
            .or_else(|| self.groups.iter().position(|group| group.id == id).map(End::Group))
            .or_else(|| self.graph.subgraphs.iter().position(|subgraph| subgraph.label == id).map(End::Group))
    }
}

fn edges(ends: &Ends<'_>) -> Vec<Edge> {
    ends.graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some(Edge {
                from: ends.named(&edge.from)?,
                to: ends.named(&edge.to)?,
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
    fn of_two_groups_holding_the_same_nodes_the_first_opened_holds_the_other() {
        let source = "flowchart TB\n  subgraph Outer\n    subgraph Middle\n      subgraph Inner\n        A\n      end\n    end\n  end";

        let diagram = Diagram::read(source).unwrap();

        let parents: Vec<(&str, Option<usize>)> = diagram.groups.iter().map(|group| (group.id.as_str(), group.parent)).collect();
        assert_eq!(parents, [("Outer", None), ("Middle", Some(0)), ("Inner", Some(1))]);
        assert_eq!(node(&diagram, "A").group, Some(2));
    }

    #[test]
    fn an_edge_between_groups_follows_the_flow_of_the_group_holding_both() {
        let source = "flowchart TB\n  A --> B\n  subgraph Side\n    direction LR\n    B --> C\n  end";

        let diagram = Diagram::read(source).unwrap();

        let end = |id: &str| End::Node(diagram.nodes.iter().position(|node| node.id == id).unwrap());
        assert_eq!(diagram.flow_between(end("A"), end("B")), Direction::Down);
        assert_eq!(diagram.flow_between(end("B"), end("C")), Direction::Right);
        assert_eq!((diagram.frames_between(end("A"), end("B")), diagram.frames_between(end("B"), end("C"))), (1, 0));
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

    fn ids(diagram: &Diagram) -> Vec<&str> {
        diagram.nodes.iter().map(|node| node.id.as_str()).collect()
    }

    #[test]
    fn an_invisible_link_keeps_both_nodes_and_draws_no_edge() {
        let diagram = Diagram::read("flowchart LR\n  A ~~~ B").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
        assert!(diagram.edges.is_empty());
    }

    #[test]
    fn an_invisible_link_places_its_nodes_one_rank_apart() {
        let diagram = Diagram::read("flowchart LR\n  A ~~~ B").unwrap();

        assert!(node(&diagram, "B").x > node(&diagram, "A").x);
        assert!((node(&diagram, "B").y - node(&diagram, "A").y).abs() < 1.0);
    }

    #[test]
    fn invisible_links_in_a_chain_keep_every_node_and_every_other_edge() {
        let diagram = Diagram::read("flowchart LR\n  A ~~~ B[Bee] ~~~ C --> D ~~~ E").unwrap();

        assert_eq!(ids(&diagram), ["A", "B", "C", "D", "E"]);
        assert_eq!(node(&diagram, "B").label.lines, ["Bee"]);
        assert_eq!(diagram.edges.iter().map(|edge| (diagram.name(edge.from), diagram.name(edge.to))).collect::<Vec<_>>(), [("C", "D")]);
    }

    #[test]
    fn a_header_without_a_direction_is_not_a_node() {
        let diagram = Diagram::read("flowchart\n  A --> B").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
    }

    #[test]
    fn tildes_inside_a_label_or_the_front_matter_are_not_a_link() {
        let source = "---\ntitle: a ~~~ b\n---\nflowchart LR\n  A[\"x ~~~ y\"] --> B";

        let diagram = Diagram::read(source).unwrap();

        assert_eq!(node(&diagram, "A").label.lines, ["x ~~~ y"]);
        assert_eq!(diagram.edges.len(), 1);
    }

    #[test]
    fn invisible_links_at_the_edge_limit_are_accepted() {
        let source = format!("flowchart LR\n{}", "  A ~~~ B\n".repeat(MAX_EDGES));

        assert_eq!(parse(&source).map(|parsed| parsed.invisible.len()).ok(), Some(MAX_EDGES));
    }

    #[test]
    fn invisible_links_count_towards_the_edge_limit() {
        let source = format!("flowchart LR\n  A --> B\n{}", "  A ~~~ B\n".repeat(MAX_EDGES));

        assert!(matches!(parse(&source), Err(Error::TooManyEdges { edges }) if edges == MAX_EDGES + 1));
    }

    #[test]
    fn an_attribute_block_gives_its_node_a_shape_and_a_label() {
        let diagram = Diagram::read("flowchart LR\n  A@{ shape: lean-r, label: \"In\" } --> B@{ shape: dbl-circ }").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
        assert_eq!(
            (node(&diagram, "A").shape, node(&diagram, "A").label.lines.as_slice()),
            (Shape::LeanRight, ["In".to_owned()].as_slice())
        );
        assert_eq!(node(&diagram, "B").shape, Shape::DoubleCircle);
    }

    #[test]
    fn an_invisible_link_is_kept_between_its_ends() {
        let diagram = Diagram::read("flowchart LR\n  A ~~~ B --> C").unwrap();

        assert_eq!(diagram.invisible, [Invisible { from: End::Node(0), to: End::Node(1) }]);
    }

    #[test]
    fn nodes_know_the_order_the_source_names_them_in() {
        let diagram = Diagram::read("flowchart LR\n  Z --> A --> M").unwrap();

        assert_eq!(diagram.nodes.iter().map(|node| (node.id.as_str(), node.order)).collect::<Vec<_>>(), [("A", 1), ("M", 2), ("Z", 0)]);
    }

    #[test]
    fn a_diagram_parsed_without_a_layout_has_the_same_parts_at_the_origin() {
        let source = "flowchart LR\n  subgraph one\n    A\n  end\n  B --> one\n  B -->|x| C ~~~ D";

        let (placed, parsed) = (Diagram::read(source).unwrap(), Diagram::parsed(source).unwrap());

        assert_eq!(ids(&parsed), ids(&placed));
        assert_eq!(
            parsed.edges.iter().map(|edge| (edge.from, edge.to)).collect::<Vec<_>>(),
            [(End::Node(1), End::Group(0)), (End::Node(1), End::Node(2))]
        );
        assert_eq!(parsed.invisible, placed.invisible);
        assert!(parsed.nodes.iter().all(|node| (node.x, node.y) == (0.0, 0.0)));
    }

    #[test]
    fn turning_top_down_changes_only_the_flows_going_sideways() {
        let source = "flowchart LR\n  subgraph Up\n    direction BT\n    A --> B\n  end\n  subgraph Side\n    C --> D\n  end";

        let turned = Diagram::parsed(source).unwrap().turned_top_down();

        assert_eq!(turned.flow, Direction::Down);
        assert_eq!(turned.groups.iter().map(|group| group.flow).collect::<Vec<_>>(), [Direction::Up, Direction::Down]);
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

        assert_eq!(check_bounds(&graph(&source), 0), Ok(()));
    }

    #[test]
    fn a_flowchart_over_the_node_limit_is_refused() {
        let source = format!("flowchart LR\n{}", (0..=MAX_NODES).map(|index| format!("  n{index}\n")).collect::<String>());

        assert_eq!(check_bounds(&graph(&source), 0), Err(Error::TooManyNodes { nodes: MAX_NODES + 1 }));
    }

    #[test]
    fn a_flowchart_at_the_edge_limit_is_accepted() {
        let source = format!("flowchart LR\n{}", "  A --> B\n".repeat(MAX_EDGES));

        assert_eq!(check_bounds(&graph(&source), 0), Ok(()));
    }

    #[test]
    fn a_flowchart_over_the_edge_limit_is_refused() {
        let source = format!("flowchart LR\n{}", "  A --> B\n".repeat(MAX_EDGES + 1));

        assert_eq!(check_bounds(&graph(&source), 0), Err(Error::TooManyEdges { edges: MAX_EDGES + 1 }));
    }
}
