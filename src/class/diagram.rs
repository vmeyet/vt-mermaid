//! The class diagram as the parser reads it, turned into the graph drawn: classes as boxes of compartments,
//! lollipops and notes as nodes, namespaces as groups, relations as edges with their heads.

use std::collections::HashMap;

use vt_canvas::{Direction, LineKind, Shape, Tip};

use super::parse::{self, Class, ClassDiagram, End as RelationType, Limits, Member, Relation};
use crate::error::Error;
use crate::graph::diagram::{Diagram, Edge, End, Figure, Group, GroupKind, Head, Node};
use crate::label::{Label, Wrap};
use crate::settings::LongWords;
use crate::source::entity;

/// Reads `source`, refusing anything but a class diagram within the limits `render` draws.
pub(super) fn read(source: &str) -> Result<Diagram, Error> {
    Ok(graph(&parse::parse(source, Limits::DRAWN)?))
}

/// Classes first, then lollipops, then notes, each in the order the source names them.
fn graph(diagram: &ClassDiagram) -> Diagram {
    let flow = direction(diagram.direction);
    let group_index: HashMap<&str, usize> =
        diagram.namespaces.iter().enumerate().map(|(index, namespace)| (namespace.id.as_str(), index)).collect();
    let group_of = |namespace: Option<&str>| namespace.and_then(|id| group_index.get(id).copied());
    let groups = diagram
        .namespaces
        .iter()
        .map(|namespace| Group {
            id: namespace.id.clone(),
            title: Label::single_line(&entity::decoded(&namespace.label)),
            parent: group_of(namespace.parent.as_deref()),
            flow,
            kind: GroupKind::Framed,
        })
        .collect();
    let classes = diagram.classes.iter().map(|class| class_node(class, group_of(class.namespace.as_deref())));
    let interfaces = diagram.interfaces.iter().map(|interface| node(&interface.id, &interface.label, Shape::Stadium, None));
    let notes = diagram.notes.iter().map(|note| node(&note.id, &note.text, Shape::RoundedRectangle, group_of(note.namespace.as_deref())));
    let nodes: Vec<Node> = classes.chain(interfaces).chain(notes).enumerate().map(|(order, node)| Node { order, ..node }).collect();
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(position, node)| (node.id.as_str(), position)).collect();
    let end = |id: &str| index.get(id).copied().map(End::Node);
    let relations = diagram.relations.iter().filter_map(|relation| Some(edge(relation, end(&relation.from)?, end(&relation.to)?)));
    let note_links = diagram.notes.iter().filter_map(|note| Some(note_link(end(&note.id)?, end(note.class.as_deref()?)?)));
    let edges = relations.chain(note_links).collect();
    Diagram { nodes, groups, edges, invisible: Vec::new(), flow, wrap: Wrap::wide(LongWords::Break) }
}

/// A box: its first annotation over its name and generic type, then its attributes and its methods, each under a
/// rule when it has any.
fn class_node(class: &Class, group: Option<usize>) -> Node {
    let generic = if class.generic.is_empty() { String::new() } else { format!("<{}>", class.generic) };
    let name = format!("{}{generic}", class.label);
    let header = class.annotations.first().map_or_else(|| name.clone(), |annotation| format!("«{annotation}»<br>{name}"));
    let members = |members: &[Member]| members.iter().map(|member| entity::decoded(&member.shown())).collect::<Vec<_>>();
    let compartments = [members(&class.attributes), members(&class.methods)].into_iter().filter(|lines| !lines.is_empty()).collect();
    Node { compartments, ..node(&class.id, &header, Shape::Rectangle, group) }
}

fn node(id: &str, text: &str, shape: Shape, group: Option<usize>) -> Node {
    let text = entity::decoded(text);
    Node { id: id.to_owned(), label: Label::wrapped(&text), text, compartments: Vec::new(), figure: Figure::Shape(shape), group, order: 0 }
}

/// Dotted (`..`) is dashed; each end draws its relation's head, its quoted text beside it unless mermaid's `none`.
fn edge(relation: &Relation, from: End, to: End) -> Edge {
    let text = |text: &str| (text != "none" && !text.is_empty()).then(|| Label::single_line(&entity::decoded(text)));
    Edge {
        from,
        to,
        label: relation.label.as_deref().filter(|label| !label.is_empty()).map(|label| Label::single_line(&entity::decoded(label))),
        start_label: text(&relation.from_text),
        end_label: text(&relation.to_text),
        kind: if relation.dotted { LineKind::Dashed } else { LineKind::Light },
        start: head(relation.from_end),
        end: head(relation.to_end),
    }
}

/// A dashed line without heads from a note to its class.
fn note_link(note: End, class: End) -> Edge {
    Edge {
        from: note,
        to: class,
        label: None,
        start_label: None,
        end_label: None,
        kind: LineKind::Dashed,
        start: Head::None,
        end: Head::None,
    }
}

/// The head of each relation type, as UML draws it.
fn head(end: Option<RelationType>) -> Head {
    match end {
        None => Head::None,
        Some(RelationType::Extension) => Head::Tips(&[Tip::Triangle]),
        Some(RelationType::Composition) => Head::Tips(&[Tip::FilledDiamond]),
        Some(RelationType::Aggregation) => Head::Tips(&[Tip::Diamond]),
        Some(RelationType::Dependency) => Head::Arrow,
        Some(RelationType::Lollipop) => Head::Tips(&[Tip::Ring]),
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

    fn node<'a>(diagram: &'a Diagram, id: &str) -> &'a Node {
        diagram.nodes.iter().find(|node| node.id == id).unwrap()
    }

    #[test]
    fn a_class_is_its_annotation_and_name_over_its_attributes_and_methods() {
        let diagram = read("classDiagram\n  class Shape~T~ {\n    <<interface>>\n    -int sides\n    +area() double$\n  }").unwrap();

        let shape = node(&diagram, "Shape");
        assert_eq!(shape.label.lines, ["«interface»", "Shape<T>"]);
        assert_eq!(shape.compartments, [vec!["-int sides".to_owned()], vec!["+area() : double$".to_owned()]]);
    }

    #[test]
    fn a_class_without_attributes_draws_only_its_methods() {
        let diagram = read("classDiagram\n  class A {\n    +run()\n  }\n  class B").unwrap();

        assert_eq!((node(&diagram, "A").compartments.len(), node(&diagram, "B").compartments.len()), (1, 0));
    }

    #[test]
    fn each_relation_type_draws_its_head_and_a_dotted_line_is_dashed() {
        let diagram = read("classDiagram\n  A <|-- B\n  C *.. D\n  E o--> F\n  G ()-- H").unwrap();

        let heads: Vec<(Head, LineKind, Head)> = diagram.edges.iter().map(|edge| (edge.start, edge.kind, edge.end)).collect();
        assert_eq!(
            heads,
            [
                (Head::Tips(&[Tip::Triangle]), LineKind::Light, Head::None),
                (Head::Tips(&[Tip::FilledDiamond]), LineKind::Dashed, Head::None),
                (Head::Tips(&[Tip::Diamond]), LineKind::Light, Head::Arrow),
                (Head::Tips(&[Tip::Ring]), LineKind::Light, Head::None),
            ]
        );
    }

    #[test]
    fn cardinalities_stand_at_their_ends_and_a_note_links_to_its_class() {
        let diagram = read("classDiagram\n  Customer \"1\" --> \"*\" Ticket : buys\n  note for Ticket \"one per seat\"").unwrap();

        let relation = &diagram.edges[0];
        let texts = |label: &Option<Label>| label.as_ref().map(|label| label.lines.clone());
        assert_eq!((texts(&relation.start_label), texts(&relation.end_label)), (Some(vec!["1".to_owned()]), Some(vec!["*".to_owned()])));
        assert_eq!((diagram.edges[1].kind, diagram.edges[1].end), (LineKind::Dashed, Head::None));
        assert_eq!(diagram.name(diagram.edges[1].from), "note0");
    }

    #[test]
    fn a_namespace_is_a_group_holding_its_classes() {
        let diagram = read("classDiagram\n  namespace Shapes {\n    class Square\n  }\n  Square <|-- Cube").unwrap();

        assert_eq!(diagram.groups[0].id, "Shapes");
        assert_eq!((node(&diagram, "Square").group, node(&diagram, "Cube").group), (Some(0), None));
    }
}
