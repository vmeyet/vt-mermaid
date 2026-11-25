//! The flowchart as the parser reads it, turned into the graph drawn: nodes, groups, edges and invisible links.

use std::collections::HashSet;

use vt_canvas::{Direction, LineKind};

use super::parse::{self, Arrow, Flowchart, Limits, Stroke};
use super::shape;
use crate::error::Error;
use crate::graph::diagram::{Diagram, Edge, End, Group, Invisible, Node};
use crate::label::{Label, Wrap};
use crate::settings::LongWords;

/// Reads `source`, refusing anything but a flowchart within the limits `render` draws.
pub(crate) fn read(source: &str) -> Result<Diagram, Error> {
    Ok(diagram(&parse::parse(source, Limits::DRAWN)?))
}

/// A node named like a subgraph is that subgraph, as mermaid merges them.
fn diagram(flowchart: &Flowchart) -> Diagram {
    let mut subgraphs: Vec<&parse::Subgraph> = flowchart.subgraphs.iter().collect();
    subgraphs.sort_by_key(|subgraph| subgraph.opened);
    let flow = direction(flowchart.direction);
    let groups = groups(&subgraphs, flow);
    let named_groups: HashSet<&str> = groups.iter().map(|group| group.id.as_str()).collect();
    let nodes: Vec<Node> = flowchart
        .nodes
        .iter()
        .filter(|node| !named_groups.contains(node.id.as_str()))
        .enumerate()
        .map(|(order, node)| Node {
            id: node.id.clone(),
            text: node.text.shown(),
            label: Label::wrapped(&node.text.shown()),
            shape: shape::named(node.shape.as_deref()),
            group: innermost(&subgraphs, &groups, &node.id),
            order,
        })
        .collect();
    let ends = Ends { nodes: &nodes, groups: &groups };
    let links: Vec<(End, End, &parse::Edge)> =
        flowchart.edges.iter().filter_map(|edge| Some((ends.named(&edge.start)?, ends.named(&edge.end)?, edge))).collect();
    let (hidden, drawn): (Vec<_>, Vec<_>) = links.into_iter().partition(|(_, _, edge)| edge.link.stroke == Stroke::Invisible);
    let edges = drawn.into_iter().map(|(from, to, edge)| self::edge(from, to, edge)).collect();
    let invisible = hidden.into_iter().map(|(from, to, _)| Invisible { from, to }).collect();
    Diagram { nodes, groups, edges, invisible, flow, wrap: Wrap::wide(LongWords::Break) }
}

/// A group's parent is the group listing its id; of two groups listing each other, as a repeated id can make them,
/// the first loses its parent. A group flows its own way, else its parent's.
fn groups(subgraphs: &[&parse::Subgraph], flow: Direction) -> Vec<Group> {
    let parents = without_cycles(
        subgraphs
            .iter()
            .enumerate()
            .map(|(index, subgraph)| (0..subgraphs.len()).find(|&outer| outer != index && subgraphs[outer].nodes.contains(&subgraph.id)))
            .collect(),
    );
    let flow_of = |start: usize| {
        let mut chain = std::iter::successors(Some(start), |&index| parents[index]).take(parents.len() + 1);
        chain.find_map(|index| subgraphs[index].direction.map(direction)).unwrap_or(flow)
    };
    subgraphs
        .iter()
        .enumerate()
        .map(|(index, subgraph)| Group {
            id: subgraph.id.clone(),
            title: Label::single_line(&subgraph.title.shown()),
            parent: parents[index],
            flow: flow_of(index),
        })
        .collect()
}

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

/// The deepest group listing the node: mermaid lists a node in one subgraph, but a repeated id can list it twice.
fn innermost(subgraphs: &[&parse::Subgraph], groups: &[Group], id: &str) -> Option<usize> {
    let depth = |group: usize| std::iter::successors(Some(group), |&index| groups[index].parent).take(groups.len() + 1).count();
    (0..groups.len()).filter(|&group| subgraphs[group].nodes.iter().any(|member| member == id)).max_by_key(|&group| depth(group))
}

fn direction(direction: parse::Direction) -> Direction {
    match direction {
        parse::Direction::TB | parse::Direction::TD => Direction::Down,
        parse::Direction::BT => Direction::Up,
        parse::Direction::LR => Direction::Right,
        parse::Direction::RL => Direction::Left,
    }
}

/// What an edge's end names: a node or a group.
#[derive(Debug, Clone, Copy)]
struct Ends<'a> {
    nodes: &'a [Node],
    groups: &'a [Group],
}

impl Ends<'_> {
    fn named(&self, id: &str) -> Option<End> {
        let node = self.nodes.iter().position(|node| node.id == id).map(End::Node);
        node.or_else(|| self.groups.iter().position(|group| group.id == id).map(End::Group))
    }
}

/// Only a pointed head draws an arrow: an open, circle or cross end draws none.
fn edge(from: End, to: End, edge: &parse::Edge) -> Edge {
    let shown = edge.text.shown();
    Edge {
        from,
        to,
        label: (!shown.is_empty()).then(|| Label::single_line(&shown)),
        kind: match edge.link.stroke {
            Stroke::Thick => LineKind::Heavy,
            Stroke::Dotted => LineKind::Dashed,
            Stroke::Normal | Stroke::Invisible | Stroke::Invalid => LineKind::Light,
        },
        arrow_start: edge.link.arrow == Arrow::DoublePoint,
        arrow_end: matches!(edge.link.arrow, Arrow::Point | Arrow::DoublePoint),
    }
}

#[cfg(test)]
mod tests {
    use vt_canvas::Shape;

    use super::*;

    fn node<'a>(diagram: &'a Diagram, id: &str) -> &'a Node {
        diagram.nodes.iter().find(|node| node.id == id).unwrap()
    }

    #[test]
    fn a_nested_group_has_its_parent_and_inherits_its_direction() {
        let source = "flowchart LR\n  subgraph Outer\n    direction TB\n    subgraph Inner\n      A --> B\n    end\n    C\n  end";

        let diagram = read(source).unwrap();

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

        let diagram = read(source).unwrap();

        let parents: Vec<(&str, Option<usize>)> = diagram.groups.iter().map(|group| (group.id.as_str(), group.parent)).collect();
        assert_eq!(parents, [("Outer", None), ("Middle", Some(0)), ("Inner", Some(1))]);
        assert_eq!(node(&diagram, "A").group, Some(2));
    }

    #[test]
    fn an_edge_between_groups_follows_the_flow_of_the_group_holding_both() {
        let source = "flowchart TB\n  A --> B\n  subgraph Side\n    direction LR\n    B --> C\n  end";

        let diagram = read(source).unwrap();

        let end = |id: &str| End::Node(diagram.nodes.iter().position(|node| node.id == id).unwrap());
        assert_eq!(diagram.flow_between(end("A"), end("B")), Direction::Down);
        assert_eq!(diagram.flow_between(end("B"), end("C")), Direction::Right);
    }

    #[test]
    fn an_edge_naming_a_group_ends_on_the_group() {
        let diagram = read("flowchart LR\n  subgraph one\n    A\n  end\n  B --> one").unwrap();

        assert_eq!(diagram.edges[0].to, End::Group(0));
        assert!(diagram.nodes.iter().all(|node| node.id != "one"));
    }

    #[test]
    fn edge_styles_become_line_kinds_and_arrows() {
        let diagram = read("flowchart LR\n  A --> B\n  B -.- C\n  C <==> D").unwrap();

        let edges: Vec<(LineKind, bool, bool)> = diagram.edges.iter().map(|edge| (edge.kind, edge.arrow_start, edge.arrow_end)).collect();
        assert_eq!(edges, [(LineKind::Light, false, true), (LineKind::Dashed, false, false), (LineKind::Heavy, true, true)]);
    }

    fn ids(diagram: &Diagram) -> Vec<&str> {
        diagram.nodes.iter().map(|node| node.id.as_str()).collect()
    }

    #[test]
    fn an_invisible_link_keeps_both_nodes_and_draws_no_edge() {
        let diagram = read("flowchart LR\n  A ~~~ B").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
        assert!(diagram.edges.is_empty());
    }

    #[test]
    fn invisible_links_in_a_chain_keep_every_node_and_every_other_edge() {
        let diagram = read("flowchart LR\n  A ~~~ B[Bee] ~~~ C --> D ~~~ E").unwrap();

        assert_eq!(ids(&diagram), ["A", "B", "C", "D", "E"]);
        assert_eq!(node(&diagram, "B").label.lines, ["Bee"]);
        assert_eq!(diagram.edges.iter().map(|edge| (diagram.name(edge.from), diagram.name(edge.to))).collect::<Vec<_>>(), [("C", "D")]);
    }

    #[test]
    fn a_header_without_a_direction_is_not_a_node() {
        let diagram = read("flowchart\n  A --> B").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
    }

    #[test]
    fn tildes_inside_a_label_or_the_front_matter_are_not_a_link() {
        let source = "---\ntitle: a ~~~ b\n---\nflowchart LR\n  A[\"x ~~~ y\"] --> B";

        let diagram = read(source).unwrap();

        assert_eq!(node(&diagram, "A").label.lines, ["x ~~~ y"]);
        assert_eq!(diagram.edges.len(), 1);
    }

    #[test]
    fn an_attribute_block_gives_its_node_a_shape_and_a_label() {
        let diagram = read("flowchart LR\n  A@{ shape: lean-r, label: \"In\" } --> B@{ shape: dbl-circ }").unwrap();

        assert_eq!(ids(&diagram), ["A", "B"]);
        assert_eq!(
            (node(&diagram, "A").shape, node(&diagram, "A").label.lines.as_slice()),
            (Shape::LeanRight, ["In".to_owned()].as_slice())
        );
        assert_eq!(node(&diagram, "B").shape, Shape::DoubleCircle);
    }

    #[test]
    fn an_invisible_link_is_kept_between_its_ends() {
        let diagram = read("flowchart LR\n  A ~~~ B --> C").unwrap();

        assert_eq!(diagram.invisible, [Invisible { from: End::Node(0), to: End::Node(1) }]);
    }

    #[test]
    fn nodes_know_the_order_the_source_names_them_in() {
        let diagram = read("flowchart LR\n  Z --> A --> M").unwrap();

        assert_eq!(diagram.nodes.iter().map(|node| (node.id.as_str(), node.order)).collect::<Vec<_>>(), [("Z", 0), ("A", 1), ("M", 2)]);
    }

    #[test]
    fn turning_top_down_changes_only_the_flows_going_sideways() {
        let source = "flowchart LR\n  subgraph Up\n    direction BT\n    A --> B\n  end\n  subgraph Side\n    C --> D\n  end";

        let turned = read(source).unwrap().turned_top_down();

        assert_eq!(turned.flow, Direction::Down);
        assert_eq!(turned.groups.iter().map(|group| group.flow).collect::<Vec<_>>(), [Direction::Up, Direction::Down]);
    }

    #[test]
    fn a_subgraph_whose_id_an_edge_names_inside_another_moves_into_it_as_mermaid_draws_it() {
        let diagram = read("flowchart TB\n  subgraph B\n    C\n  end\n  subgraph A\n    D --> B\n  end").unwrap();

        let parents: Vec<(&str, Option<usize>)> = diagram.groups.iter().map(|group| (group.id.as_str(), group.parent)).collect();
        assert_eq!(parents, [("B", Some(1)), ("A", None)]);
    }

    #[test]
    fn a_double_circle_and_a_circle_keep_their_shapes() {
        let diagram = read("flowchart LR\n  A(((one))) --> B((two)) --o C --x D").unwrap();

        assert_eq!((node(&diagram, "A").shape, node(&diagram, "B").shape), (Shape::DoubleCircle, Shape::Circle));
        assert_eq!(diagram.edges.iter().map(|edge| edge.arrow_end).collect::<Vec<_>>(), [true, false, false]);
    }

    #[test]
    fn entities_in_labels_are_shown_as_their_characters() {
        let diagram = read("flowchart LR\n  A[\"#quot;hi#quot; &amp; #35;1\"] -- &lt;go&gt; --> B").unwrap();

        assert_eq!(node(&diagram, "A").label.lines, ["\"hi\" & #1"]);
        assert_eq!(diagram.edges[0].label.as_ref().map(|label| label.lines.clone()), Some(vec!["<go>".to_owned()]));
    }

    #[test]
    fn text_without_a_diagram_header_is_invalid() {
        assert!(matches!(read("not a diagram"), Err(Error::Invalid { .. })));
    }
}
