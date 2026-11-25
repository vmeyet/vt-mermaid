//! The ER diagram as the parser reads it, turned into the graph drawn: entities as tables, subgraphs as groups,
//! relationships as edges ending on crow's feet.

use std::collections::HashMap;

use vt_canvas::{Direction, LineKind, Shape, Tip};

use super::parse::{self, Attribute, Cardinality, Entity, ErDiagram, Limits, Relationship};
use crate::error::Error;
use crate::graph::diagram::{Diagram, Edge, End, Figure, Group, GroupKind, Head, Node, without_cycles};
use crate::label::{Label, Wrap};
use crate::settings::LongWords;
use crate::source::entity;

/// Reads `source`, refusing anything but an ER diagram within the limits `render` draws.
pub(super) fn read(source: &str) -> Result<Diagram, Error> {
    Ok(graph(&parse::parse(source, Limits::DRAWN)?))
}

/// An entity named like a subgraph is that subgraph, as mermaid draws it; a subgraph or an entity belongs to the
/// first subgraph listing it, and of subgraphs holding each other, as a repeated id makes them, the first is held by
/// none.
fn graph(diagram: &ErDiagram) -> Diagram {
    let flow = direction(diagram.direction);
    let group_index: HashMap<&str, usize> =
        diagram.subgraphs.iter().enumerate().map(|(index, subgraph)| (subgraph.id.as_str(), index)).collect();
    let holder: HashMap<&str, usize> = diagram
        .subgraphs
        .iter()
        .enumerate()
        .rev()
        .flat_map(|(index, subgraph)| subgraph.nodes.iter().map(move |node| (node.as_str(), index)))
        .collect();
    let parents = without_cycles(
        diagram
            .subgraphs
            .iter()
            .enumerate()
            .map(|(index, subgraph)| holder.get(subgraph.id.as_str()).copied().filter(|&parent| parent != index))
            .collect(),
    );
    let flows = flows(diagram, &parents, flow);
    let groups = diagram
        .subgraphs
        .iter()
        .zip(parents.iter().zip(&flows))
        .map(|(subgraph, (&parent, &flow))| Group {
            id: subgraph.id.clone(),
            title: Label::single_line(&entity::decoded(&subgraph.title)),
            parent,
            flow,
            kind: GroupKind::Framed,
        })
        .collect();
    let nodes: Vec<Node> = diagram
        .entities
        .iter()
        .filter(|entity| !group_index.contains_key(entity.name.as_str()))
        .enumerate()
        .map(|(order, entity)| Node { order, ..table(entity, holder.get(entity.name.as_str()).copied()) })
        .collect();
    let node_index: HashMap<&str, usize> = diagram
        .entities
        .iter()
        .filter_map(|entity| Some((entity.id.as_str(), nodes.iter().position(|node| node.id == entity.name)?)))
        .collect();
    let end = |id: &str| node_index.get(id).map(|&node| End::Node(node)).or_else(|| group_index.get(id).map(|&group| End::Group(group)));
    let edges = diagram
        .relationships
        .iter()
        .filter_map(|relationship| Some(edge(relationship, end(&relationship.from)?, end(&relationship.to)?)))
        .collect();
    Diagram { nodes, groups, edges, invisible: Vec::new(), flow, wrap: Wrap::wide(LongWords::Break) }
}

/// A subgraph flows its own way, else its parent's.
fn flows(diagram: &ErDiagram, parents: &[Option<usize>], flow: Direction) -> Vec<Direction> {
    (0..diagram.subgraphs.len())
        .map(|start| {
            let mut chain = std::iter::successors(Some(start), |&index| parents[index]).take(parents.len() + 1);
            chain.find_map(|index| diagram.subgraphs[index].direction.map(direction)).unwrap_or(flow)
        })
        .collect()
}

/// A table: its alias, else its name, over one row per attribute, its type, name, keys and comment each in a column.
fn table(entity: &Entity, group: Option<usize>) -> Node {
    let text = entity::decoded(if entity.alias.is_empty() { &entity.name } else { &entity.alias });
    let rows = rows(&entity.attributes);
    let compartments = if rows.is_empty() { Vec::new() } else { vec![rows] };
    Node {
        id: entity.name.clone(),
        label: Label::wrapped(&text),
        text,
        compartments,
        figure: Figure::Shape(Shape::Rectangle),
        group,
        order: 0,
    }
}

/// The attributes' columns, each as wide as its widest cell and [`GAP`] apart; a column no attribute fills is left
/// out, and comments wrap at spaces onto rows of their own so a row stays within [`ROW_COLUMNS`] while they keep
/// [`COMMENT_COLUMNS`].
fn rows(attributes: &[Attribute]) -> Vec<String> {
    let cells: Vec<[String; 4]> = attributes
        .iter()
        .map(|attribute| {
            [&attribute.kind, &attribute.name, &attribute.keys.join(","), &attribute.comment].map(|cell| entity::decoded(cell))
        })
        .collect();
    let widths: [usize; 4] = std::array::from_fn(|column| cells.iter().map(|row| vt_canvas::width(&row[column])).max().unwrap_or(0));
    let before_comments: usize = widths[..3].iter().filter(|&&width| width > 0).map(|width| width + GAP.len()).sum();
    let comment_columns = ROW_COLUMNS.saturating_sub(before_comments).max(COMMENT_COLUMNS).min(widths[3]);
    let widths = [widths[0], widths[1], widths[2], comment_columns];
    cells.iter().flat_map(|row| table_rows(row, widths)).collect()
}

/// Columns between two of an attribute's cells.
const GAP: &str = "  ";
/// Columns a row of attributes keeps within, comments wrapping to fit.
const ROW_COLUMNS: usize = 60;
/// Columns a comment keeps however wide the cells before it.
const COMMENT_COLUMNS: usize = 12;

/// One attribute's cells padded to their columns, its comment wrapped over as many rows as it takes.
fn table_rows(row: &[String; 4], widths: [usize; 4]) -> Vec<String> {
    let comment = Label::wrapped_as(&row[3], Wrap::to(widths[3].max(1), LongWords::Break).tight()).lines;
    let lines = comment.len().max(1);
    (0..lines)
        .map(|line| {
            let cells = [&row[0], &row[1], &row[2]].map(|cell| if line == 0 { cell.as_str() } else { "" });
            let comment = comment.get(line).map_or("", String::as_str);
            let columns = cells.into_iter().chain([comment]).zip(widths).filter(|&(_, width)| width > 0);
            let padded: Vec<String> =
                columns.map(|(cell, width)| format!("{cell}{}", " ".repeat(width.saturating_sub(vt_canvas::width(cell))))).collect();
            padded.join(GAP).trim_end().to_owned()
        })
        .collect()
}

/// Identifying is solid, non-identifying dashed; each end draws the crow's foot of its cardinality, the role on the
/// line.
fn edge(relationship: &Relationship, from: End, to: End) -> Edge {
    let role = entity::decoded(&relationship.role);
    Edge {
        from,
        to,
        label: (!role.trim().is_empty()).then(|| Label::single_line(&role)),
        start_label: None,
        end_label: None,
        kind: if relationship.identifying { LineKind::Light } else { LineKind::Dashed },
        start: crow(relationship.from_cardinality),
        end: crow(relationship.to_cardinality),
    }
}

/// A crow's foot, from the entity outwards: its most, a bar for one or a fork for many, then its least, a ring for
/// zero or a bar for one; mermaid draws no mark for the parent of a generalisation.
fn crow(cardinality: Cardinality) -> Head {
    match cardinality {
        Cardinality::OnlyOne => Head::Tips(&[Tip::Bar, Tip::Bar]),
        Cardinality::ZeroOrOne => Head::Tips(&[Tip::Bar, Tip::Ring]),
        Cardinality::OneOrMore => Head::Tips(&[Tip::Fork, Tip::Bar]),
        Cardinality::ZeroOrMore => Head::Tips(&[Tip::Fork, Tip::Ring]),
        Cardinality::MdParent => Head::None,
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
    fn an_entity_is_a_table_of_its_attributes_columns_aligned() {
        let source = "erDiagram\n  CUSTOMER[Customer] {\n    string name PK \"full name\"\n    int age\n    string email UK\n  }";

        let diagram = read(source).unwrap();

        let customer = node(&diagram, "CUSTOMER");
        assert_eq!(customer.label.lines, ["Customer"]);
        assert_eq!(
            customer.compartments,
            [vec!["string  name   PK  full name".to_owned(), "int     age".to_owned(), "string  email  UK".to_owned()]]
        );
    }

    #[test]
    fn a_long_comment_wraps_onto_rows_of_its_own() {
        let source =
            "erDiagram\n  PERSON {\n    string(99) firstName \"Only 99 characters are allowed in a first name here\"\n    int age PK\n  }";

        let diagram = read(source).unwrap();

        assert_eq!(
            node(&diagram, "PERSON").compartments,
            [vec![
                "string(99)  firstName      Only 99 characters are allowed in".to_owned(),
                "                           a first name here".to_owned(),
                "int         age        PK".to_owned()
            ]]
        );
    }

    #[test]
    fn each_cardinality_draws_its_crow_s_foot_and_a_non_identifying_line_is_dashed() {
        let diagram = read("erDiagram\n  A ||--o{ B : has\n  B |o..|{ C : may").unwrap();

        let ends: Vec<(Head, LineKind, Head)> = diagram.edges.iter().map(|edge| (edge.start, edge.kind, edge.end)).collect();
        assert_eq!(
            ends,
            [
                (Head::Tips(&[Tip::Bar, Tip::Bar]), LineKind::Light, Head::Tips(&[Tip::Fork, Tip::Ring])),
                (Head::Tips(&[Tip::Bar, Tip::Ring]), LineKind::Dashed, Head::Tips(&[Tip::Fork, Tip::Bar])),
            ]
        );
        assert_eq!(diagram.edges[0].label.as_ref().map(|label| label.lines.clone()), Some(vec!["has".to_owned()]));
    }

    #[test]
    fn a_subgraph_is_a_group_holding_its_entities_and_a_relationship_may_end_on_it() {
        let diagram = read("erDiagram\n  subgraph Sales [Sales area]\n    ORDER\n  end\n  CUSTOMER ||--o{ Sales : buys").unwrap();

        assert_eq!((diagram.groups[0].id.as_str(), diagram.groups[0].title.lines[0].as_str()), ("Sales", "Sales area"));
        assert_eq!(node(&diagram, "ORDER").group, Some(0));
        assert_eq!(diagram.edges[0].to, End::Group(0));
    }

    /// Found by fuzzing: subgraphs sharing an id held each other, so none was placed and its frame overflowed.
    #[test]
    fn subgraphs_named_alike_never_hold_each_other() {
        let source = "erDiagram\n   TOP\n tion LR\n    subgraph TOP\n  n LR\n    subgraph TOP\n tion LR\n        end\n    e     subgraph B3    \ndirection LR\n    subgraph TOP\n tion LR\n    subgraph TOP\n   directio    \n        end\n  \n        end\n    end\nn LR\n    subgraph TOP\n tion \n     z  end\n    e     subgraph B1    \n      subgraphLR\n    subgrLR\n        end\n    e     subgraph B2   \n         end\n    end\nNia";

        let diagram = read(source).unwrap();

        let parents = diagram.groups.iter().map(|group| group.parent).collect::<Vec<_>>();
        assert!(crate::graph::render(&diagram, crate::settings::Settings::new(100)).is_ok(), "{parents:?}");
    }
}
