//! Mermaid's ER db (`erDb.ts`): what each statement adds to the diagram or merges into it.

use std::collections::HashMap;

use super::Limits;
use super::lexer::{Cardinality, Direction};
use crate::error::Error;
use crate::source::is_space;

/// An ER diagram as mermaid's db holds it once the source is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::er) struct ErDiagram {
    pub(in crate::er) direction: Direction,
    /// In the order the source first names them.
    pub(in crate::er) entities: Vec<Entity>,
    pub(in crate::er) relationships: Vec<Relationship>,
    /// In the order they close: an inner one before the one holding it.
    pub(in crate::er) subgraphs: Vec<Subgraph>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::er) struct Entity {
    pub(in crate::er) name: String,
    /// `entity-<name>-<place>`, what a relationship names it by.
    pub(in crate::er) id: String,
    pub(in crate::er) alias: String,
    pub(in crate::er) attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::er) struct Attribute {
    pub(in crate::er) kind: String,
    pub(in crate::er) name: String,
    pub(in crate::er) keys: Vec<String>,
    pub(in crate::er) comment: String,
}

/// A relationship from `from` to `to`, each end an entity's id or a subgraph's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::er) struct Relationship {
    pub(in crate::er) from: String,
    pub(in crate::er) to: String,
    pub(in crate::er) role: String,
    /// How many of `from` there are for one `to`, written before the line: mermaid's `cardB`.
    pub(in crate::er) from_cardinality: Cardinality,
    /// How many of `to` there are for one `from`, written after the line: mermaid's `cardA`.
    pub(in crate::er) to_cardinality: Cardinality,
    pub(in crate::er) identifying: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::er) struct Subgraph {
    pub(in crate::er) id: String,
    pub(in crate::er) title: String,
    /// What it holds and no subgraph closed before holds: entities' and inner subgraphs' names.
    pub(in crate::er) nodes: Vec<String>,
    pub(in crate::er) direction: Option<Direction>,
}

/// What a statement of a subgraph's document hands it: a name, or a direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Held {
    Name(String),
    Direction(Direction),
}

/// The db while the grammar reads the source.
#[derive(Debug, Clone)]
pub(super) struct Db {
    diagram: ErDiagram,
    entity_index: HashMap<String, usize>,
    limits: Limits,
}

impl Db {
    pub(super) fn new(limits: Limits) -> Self {
        let diagram = ErDiagram { direction: Direction::TB, entities: Vec::new(), relationships: Vec::new(), subgraphs: Vec::new() };
        Self { diagram, entity_index: HashMap::new(), limits }
    }

    pub(super) fn into_diagram(self) -> ErDiagram {
        self.diagram
    }

    pub(super) fn set_direction(&mut self, direction: Direction) {
        self.diagram.direction = direction;
    }

    /// `addEntity`: an entity is added the first time it is named; an alias given later counts while it has none.
    pub(super) fn add_entity(&mut self, name: &str, alias: &str) -> Result<usize, Error> {
        if let Some(&index) = self.entity_index.get(name) {
            let entity = &mut self.diagram.entities[index];
            if entity.alias.is_empty() {
                alias.clone_into(&mut entity.alias);
            }
            return Ok(index);
        }
        let index = self.diagram.entities.len();
        self.entity_index.insert(name.to_owned(), index);
        self.diagram.entities.push(Entity {
            name: name.to_owned(),
            id: format!("entity-{name}-{index}"),
            alias: alias.to_owned(),
            attributes: Vec::new(),
        });
        let nodes = self.diagram.entities.len() + self.diagram.subgraphs.len();
        if nodes > self.limits.nodes { Err(Error::TooManyNodes { nodes }) } else { Ok(index) }
    }

    pub(super) fn add_attributes(&mut self, name: &str, attributes: Vec<Attribute>) -> Result<(), Error> {
        let index = self.add_entity(name, "")?;
        self.diagram.entities[index].attributes.extend(attributes);
        Ok(())
    }

    /// `addRelationship`: an end named like a subgraph closed before is that subgraph.
    pub(super) fn add_relationship(&mut self, from: &str, role: &str, to: &str, ends: Ends) -> Result<(), Error> {
        let from = self.end_id(from)?;
        let to = self.end_id(to)?;
        self.diagram.relationships.push(Relationship {
            from,
            to,
            role: role.to_owned(),
            from_cardinality: ends.before,
            to_cardinality: ends.after,
            identifying: ends.identifying,
        });
        let edges = self.diagram.relationships.len();
        if edges > self.limits.edges { Err(Error::TooManyEdges { edges }) } else { Ok(()) }
    }

    fn end_id(&mut self, name: &str) -> Result<String, Error> {
        if self.diagram.subgraphs.iter().any(|subgraph| subgraph.id == name) {
            return Ok(name.to_owned());
        }
        let index = self.add_entity(name, "")?;
        Ok(self.diagram.entities[index].id.clone())
    }

    /// `addSubGraph`: it holds each name its document hands it once, but those a subgraph closed before holds, and
    /// flows as the last direction among them says.
    pub(super) fn add_subgraph(&mut self, id: &str, held: &[Held], title: &str) -> Result<String, Error> {
        let taken: Vec<&str> = self.diagram.subgraphs.iter().flat_map(|subgraph| subgraph.nodes.iter().map(String::as_str)).collect();
        let mut nodes: Vec<String> = Vec::new();
        let mut direction = None;
        for item in held {
            match item {
                Held::Direction(flow) => direction = Some(*flow),
                Held::Name(name) => {
                    let trimmed = name.trim_matches(is_space);
                    if !trimmed.is_empty() && !nodes.iter().any(|node| node.trim_matches(is_space) == trimmed) {
                        nodes.push(name.clone());
                    }
                }
            }
        }
        nodes.retain(|node| !taken.contains(&node.as_str()));
        let id = id.trim_matches(is_space).to_owned();
        self.diagram.subgraphs.push(Subgraph { id: id.clone(), title: title.trim_matches(is_space).to_owned(), nodes, direction });
        let nodes = self.diagram.entities.len() + self.diagram.subgraphs.len();
        if nodes > self.limits.nodes { Err(Error::TooManyNodes { nodes }) } else { Ok(id) }
    }
}

/// A relationship's two cardinalities, as written before and after its line, and its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ends {
    pub(super) before: Cardinality,
    pub(super) after: Cardinality,
    pub(super) identifying: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alias_given_later_counts_only_while_the_entity_has_none() {
        let mut db = Db::new(Limits::DRAWN);

        db.add_entity("A", "").unwrap();
        db.add_entity("A", "first").unwrap();
        db.add_entity("A", "second").unwrap();

        assert_eq!(db.into_diagram().entities[0].alias, "first");
    }

    #[test]
    fn a_relationship_ends_on_a_subgraph_closed_before_else_on_an_entity_it_adds() {
        let mut db = Db::new(Limits::DRAWN);
        let ends = Ends { before: Cardinality::OnlyOne, after: Cardinality::ZeroOrMore, identifying: true };

        db.add_subgraph("Sales", &[], "Sales").unwrap();
        db.add_relationship("CUSTOMER", "buys", "Sales", ends).unwrap();
        db.add_attributes(
            "CUSTOMER",
            vec![Attribute { kind: "string".into(), name: "name".into(), keys: Vec::new(), comment: String::new() }],
        )
        .unwrap();
        db.set_direction(Direction::LR);

        let diagram = db.into_diagram();
        assert_eq!((diagram.relationships[0].from.as_str(), diagram.relationships[0].to.as_str()), ("entity-CUSTOMER-0", "Sales"));
        assert_eq!((diagram.entities[0].attributes.len(), diagram.direction), (1, Direction::LR));
    }

    #[test]
    fn a_subgraph_leaves_out_what_one_closed_before_holds_and_takes_its_last_direction() {
        let mut db = Db::new(Limits::DRAWN);
        let names = |names: &[&str]| names.iter().map(|name| Held::Name((*name).to_owned())).collect::<Vec<_>>();

        let turned = [names(&["A", "B", "A"]), vec![Held::Direction(Direction::LR)]].concat();

        db.add_subgraph("inner", &turned, "inner").unwrap();
        db.add_subgraph("outer", &names(&["inner", "B", "C"]), "Outer").unwrap();

        let subgraphs: Vec<(Vec<String>, Option<Direction>)> =
            db.into_diagram().subgraphs.into_iter().map(|subgraph| (subgraph.nodes, subgraph.direction)).collect();
        assert_eq!(
            subgraphs,
            [(vec!["A".to_owned(), "B".to_owned()], Some(Direction::LR)), (vec!["inner".to_owned(), "C".to_owned()], None)]
        );
    }
}
