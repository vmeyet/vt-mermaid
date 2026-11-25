//! The flowchart as mermaid-rs-renderer parses it: the only module that sees the crate.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};

use mermaid_rs_renderer::{self as mmdr, DiagramKind, EdgeStyle, Graph, NodeShape};
use vt_canvas::{Direction, LineKind, Shape};

use super::attributes;
use super::invisible::{self, Link};
use super::label::Label;
use super::source;
use crate::error::{Error, MAX_EDGES, MAX_NODES};

/// What mmdr knows of a flowchart, its text already cut into lines.
#[derive(Debug, Clone)]
pub(super) struct Diagram {
    pub(super) nodes: Vec<Node>,
    pub(super) groups: Vec<Group>,
    pub(super) edges: Vec<Edge>,
    /// Links that shape the layout and are never drawn (`A ~~~ B`).
    pub(super) invisible: Vec<Invisible>,
    pub(super) flow: Direction,
}

#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) id: String,
    pub(super) label: Label,
    pub(super) shape: Shape,
    /// The innermost group holding it.
    pub(super) group: Option<usize>,
    /// Its place among the nodes in the order the source first names them.
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
    /// Parses `source`, refusing anything but a flowchart within [`MAX_NODES`] and [`MAX_EDGES`].
    pub(super) fn read(source: &str) -> Result<Self, Error> {
        Ok(Self::new(&parse(source)?))
    }

    /// Its ranks and those of its groups going down where they go sideways.
    pub(super) fn turned_top_down(&self) -> Self {
        let turned = |flow: Direction| if matches!(flow, Direction::Left | Direction::Right) { Direction::Down } else { flow };
        let groups = self.groups.iter().map(|group| Group { flow: turned(group.flow), ..group.clone() }).collect();
        Self { groups, flow: turned(self.flow), ..self.clone() }
    }

    /// Whether its ranks, or those of one of its groups, go left or right.
    pub(super) fn flows_sideways(&self) -> bool {
        let sideways = |flow: Direction| matches!(flow, Direction::Left | Direction::Right);
        sideways(self.flow) || self.groups.iter().any(|group| sideways(group.flow))
    }

    fn new(parsed: &Parsed) -> Self {
        let graph = &parsed.graph;
        let nodes = nodes(graph, &labels(parsed));
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

/// Every node's id: the graph's own and the ends edges name without declaring.
fn ids(graph: &Graph) -> BTreeSet<&str> {
    let ends = graph.edges.iter().flat_map(|edge| [&edge.from, &edge.to]);
    graph.nodes.keys().chain(ends).map(String::as_str).collect()
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

/// Invisible links count as edges: the layout ranks their ends as it does an edge's.
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

fn direction(direction: mmdr::Direction) -> Direction {
    match direction {
        mmdr::Direction::TopDown => Direction::Down,
        mmdr::Direction::BottomTop => Direction::Up,
        mmdr::Direction::LeftRight => Direction::Right,
        mmdr::Direction::RightLeft => Direction::Left,
    }
}

/// A node mmdr added for an edge end is a box named after its id.
fn label_and_shape(labels: &HashMap<&str, (Label, Shape)>, id: &str) -> (Label, Shape) {
    labels.get(id).cloned().unwrap_or_else(|| (Label::wrapped(id), Shape::Rectangle))
}

/// The nodes in the order of their ids, but for those that only stand for a group an edge names.
fn nodes(graph: &Graph, labels: &HashMap<&str, (Label, Shape)>) -> Vec<Node> {
    let ids = ids(graph);
    let anchors: HashSet<&str> = graph.subgraphs.iter().filter_map(|subgraph| anchor(subgraph, &ids)).collect();
    ids.iter()
        .filter(|id| !anchors.contains(*id))
        .map(|&id| {
            let (label, shape) = label_and_shape(labels, id);
            let order = graph.node_order.get(id).copied().unwrap_or(usize::MAX);
            Node { id: id.to_owned(), label, shape, group: None, order }
        })
        .collect()
}

/// The name of a subgraph an edge ends on, which is no node: its id first, then its title, unless it holds a node
/// of that name.
fn anchor<'a>(subgraph: &'a mmdr::Subgraph, ids: &BTreeSet<&str>) -> Option<&'a str> {
    [subgraph.id.as_deref(), Some(subgraph.label.as_str())]
        .into_iter()
        .flatten()
        .find(|id| ids.contains(*id) && !subgraph.nodes.iter().any(|member| member == id))
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
    fn turning_top_down_changes_only_the_flows_going_sideways() {
        let source = "flowchart LR\n  subgraph Up\n    direction BT\n    A --> B\n  end\n  subgraph Side\n    C --> D\n  end";

        let turned = Diagram::read(source).unwrap().turned_top_down();

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
