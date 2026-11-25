//! What mermaid's `stateDb.ts` makes of the parsed document for its renderer: `docTranslator` names the start and end
//! of each document and splits a composite state's body at its dividers, then `dataFetcher` walks the documents into
//! the nodes and edges drawn.

use std::collections::HashMap;

use super::lexer::Direction;
use super::{Invalid, Limits};
use crate::error::Error;

/// A statement of a document, as the grammar's actions build it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Statement {
    State(State),
    Relation {
        from: State,
        to: State,
        description: String,
    },
    Direction(Direction),
    /// A statement the drawing never reads (a class, a style, a click, a floating note…), kept because it still
    /// makes a concurrent region of its own.
    Other,
}

/// A state statement: a state named, described, given a note, or opening a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct State {
    pub(super) id: String,
    /// Mermaid's `type`; a note statement has none.
    pub(super) kind: Option<Kind>,
    pub(super) description: Description,
    pub(super) doc: Option<Vec<Statement>>,
    pub(super) note: Option<Note>,
    /// Whether a `[*]` is the start (`true`) or the end of its document.
    pub(super) start: Option<bool>,
}

impl State {
    pub(super) fn named(id: String) -> Self {
        Self { id, kind: Some(Kind::Default), description: Description::None, doc: None, note: None, start: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Default,
    Fork,
    Join,
    Choice,
    Divider,
}

/// A state's description: none, one text, or several, as `state "a" as b:c` gives two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Description {
    None,
    Text(String),
    Many(Vec<String>),
}

impl Description {
    /// An empty text describes nothing, as JavaScript reads `''` as false.
    pub(super) fn text(text: String) -> Self {
        if text.is_empty() { Self::None } else { Self::Text(text) }
    }

    fn lines(self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Text(text) => vec![text],
            Self::Many(lines) => lines,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Note {
    pub(super) side: Side,
    pub(super) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::state) enum Side {
    Left,
    Right,
}

/// The shape mermaid's renderer gives a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::state) enum Shape {
    Rect,
    RectWithTitle,
    Start,
    End,
    Fork,
    Join,
    Choice,
    /// A concurrent region of a composite state.
    Divider,
    /// A composite state.
    RoundedWithTitle,
    Note,
    /// The group mermaid lays a note out in.
    NoteGroup,
}

/// A state diagram as mermaid's renderer is handed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::state) struct Diagram {
    pub(in crate::state) direction: Direction,
    pub(in crate::state) nodes: Vec<Node>,
    pub(in crate::state) edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::state) struct Node {
    pub(in crate::state) id: String,
    /// None for a state a note names before anything else does, as mermaid leaves it.
    pub(in crate::state) shape: Option<Shape>,
    pub(in crate::state) label: String,
    /// The descriptions after the first, each a line under the label.
    pub(in crate::state) descriptions: Vec<String>,
    pub(in crate::state) parent: Option<String>,
    pub(in crate::state) group: bool,
    /// The way a group's ranks go.
    pub(in crate::state) direction: Option<Direction>,
    /// The side of its state a note, or a note's group, stands.
    pub(in crate::state) side: Option<Side>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::state) struct Edge {
    pub(in crate::state) start: String,
    pub(in crate::state) end: String,
    pub(in crate::state) label: String,
    /// A link between a note and its state, drawn without a head.
    pub(in crate::state) note: bool,
}

/// The nodes and edges of the root document, refused as mermaid refuses a composite state with a description or once
/// more than `limits` allow.
pub(super) fn apply(root: Vec<Statement>, limits: Limits) -> Result<Diagram, Error> {
    let direction = direction_of(&root).unwrap_or(Direction::TB);
    let mut regions = 0;
    let root = translated_doc("root", root, &mut regions);
    let mut fetcher = Fetcher { limits, known: HashMap::new(), nodes: Vec::new(), places: HashMap::new(), edges: Vec::new(), items: 0 };
    fetcher.doc(None, &root)?;
    let nodes = fetcher.nodes.into_iter().map(described).collect::<Result<_, _>>()?;
    Ok(Diagram { direction, nodes, edges: fetcher.edges })
}

/// Mermaid's `getDir`: the last `direction` of a document.
fn direction_of(doc: &[Statement]) -> Option<Direction> {
    doc.iter().rev().find_map(|statement| match statement {
        Statement::Direction(direction) => Some(*direction),
        _ => None,
    })
}

/// `docTranslator` on a document: each `[*]` named after the document holding it, `_start` where it leads and `_end`
/// where it is led to, and a body split into its concurrent regions.
fn translated_doc(parent: &str, doc: Vec<Statement>, regions: &mut usize) -> Vec<Statement> {
    split_into_regions(doc, regions)
        .into_iter()
        .map(|statement| match statement {
            Statement::State(state) => Statement::State(translated(parent, state, true, regions)),
            Statement::Relation { from, to, description } => Statement::Relation {
                from: translated(parent, from, true, regions),
                to: translated(parent, to, false, regions),
                description,
            },
            other => other,
        })
        .collect()
}

fn translated(parent: &str, state: State, first: bool, regions: &mut usize) -> State {
    let (id, start) = if state.id == "[*]" {
        (format!("{parent}_{}", if first { "start" } else { "end" }), Some(first))
    } else {
        (state.id.trim_matches(crate::source::is_space).to_owned(), state.start)
    };
    let doc = state.doc.map(|doc| translated_doc(&id, doc, regions));
    State { id, doc, start, ..state }
}

/// The statements before each divider become that divider's body, those after the last one a region of their own,
/// named as mermaid's random `generateId` would be; a body ending with a divider is left as written.
fn split_into_regions(doc: Vec<Statement>, regions: &mut usize) -> Vec<Statement> {
    let is_divider = |statement: &Statement| matches!(statement, Statement::State(State { kind: Some(Kind::Divider), .. }));
    if doc.last().is_none_or(is_divider) || !doc.iter().any(is_divider) {
        return doc;
    }
    let mut split = Vec::new();
    let mut current = Vec::new();
    for statement in doc {
        match statement {
            Statement::State(divider @ State { kind: Some(Kind::Divider), .. }) => {
                split.push(Statement::State(State { doc: Some(std::mem::take(&mut current)), ..divider }));
            }
            other => current.push(other),
        }
    }
    *regions += 1;
    let last = State { kind: Some(Kind::Divider), doc: Some(current), ..State::named(format!("id-region-{regions}")) };
    split.push(Statement::State(last));
    split
}

/// A node as `dataFetcher` keeps it between the statements naming it (its `nodeDb`).
#[derive(Debug, Clone)]
struct Known {
    shape: Option<Shape>,
    description: Description,
    group: bool,
    direction: Option<Direction>,
}

/// `dataFetcher`'s walk: the nodes it has made, indexed by id, and the edges.
#[derive(Debug)]
struct Fetcher {
    limits: Limits,
    known: HashMap<String, Known>,
    nodes: Vec<Node>,
    places: HashMap<String, usize>,
    edges: Vec<Edge>,
    /// Mermaid's `graphItemCount`: edges and notes made so far, which name each note.
    items: usize,
}

impl Fetcher {
    /// `setupDoc`: each state statement and both ends of each relation, then the relation's edge.
    fn doc(&mut self, parent: Option<&str>, doc: &[Statement]) -> Result<(), Error> {
        for statement in doc {
            match statement {
                Statement::State(state) => self.state(parent, state)?,
                Statement::Relation { from, to, description } => {
                    self.state(parent, from)?;
                    self.state(parent, to)?;
                    self.edge(Edge { start: from.id.clone(), end: to.id.clone(), label: description.clone(), note: false })?;
                    self.items += 1;
                }
                Statement::Direction(_) | Statement::Other => {}
            }
        }
        Ok(())
    }

    fn state(&mut self, parent: Option<&str>, state: &State) -> Result<(), Error> {
        if state.id != "root" {
            let node = self.node(parent, state);
            match &state.note {
                Some(note) => self.note(node, note)?,
                None => self.insert(node)?,
            }
        }
        match &state.doc {
            Some(doc) => self.doc(Some(&state.id), doc),
            None => Ok(()),
        }
    }

    /// The node a statement makes or changes: its shape, its descriptions, whether it is a group.
    fn node(&mut self, parent: Option<&str>, state: &State) -> Node {
        let shape = match (state.kind, state.start) {
            (Some(Kind::Default), Some(true)) => Some(Shape::Start),
            (Some(Kind::Default), Some(false)) => Some(Shape::End),
            (Some(Kind::Default), None) => Some(Shape::Rect),
            (Some(Kind::Fork), _) => Some(Shape::Fork),
            (Some(Kind::Join), _) => Some(Shape::Join),
            (Some(Kind::Choice), _) => Some(Shape::Choice),
            (Some(Kind::Divider), _) => Some(Shape::Divider),
            (None, _) => None,
        };
        let id = &state.id;
        let known = self.known.entry(id.clone()).or_insert_with(|| Known {
            shape,
            description: Description::Text(id.clone()),
            group: false,
            direction: None,
        });
        if state.description != Description::None {
            let added = state.description.clone().lines();
            (known.shape, known.description) = match std::mem::replace(&mut known.description, Description::None) {
                Description::Many(lines) => (Some(Shape::RectWithTitle), Description::Many([lines, added].concat())),
                Description::Text(text) if text == *id => (Some(Shape::RectWithTitle), Description::Many(added)),
                Description::Text(text) if !text.is_empty() => {
                    (Some(Shape::RectWithTitle), Description::Many([vec![text], added].concat()))
                }
                Description::Text(_) | Description::None => (Some(Shape::Rect), state.description.clone()),
            };
        }
        if matches!(&known.description, Description::Many(lines) if lines.len() == 1) && known.shape == Some(Shape::RectWithTitle) {
            known.shape = Some(if known.group { Shape::RoundedWithTitle } else { Shape::Rect });
        }
        if !known.group && state.doc.is_some() {
            known.group = true;
            known.direction = Some(state.doc.as_deref().and_then(direction_of).unwrap_or(Direction::TB));
            known.shape = Some(if state.kind == Some(Kind::Divider) { Shape::Divider } else { Shape::RoundedWithTitle });
        }
        let label = if known.shape == Some(Shape::Divider) { Description::None } else { known.description.clone() };
        Node {
            id: id.clone(),
            shape: known.shape,
            label: String::new(),
            descriptions: label.lines(),
            parent: parent.filter(|&parent| parent != "root").map(str::to_owned),
            group: known.group,
            direction: known.direction,
            side: None,
        }
    }

    /// A note: a group of its own holding it, then the state, then a link between them, from the note when it stands
    /// on the left.
    fn note(&mut self, node: Node, note: &Note) -> Result<(), Error> {
        let group = format!("{}----parent", node.id);
        let id = format!("{}----note-{}", node.id, self.items);
        self.items += 1;
        let text = vec![note.text.clone()];
        let side = Some(note.side);
        let shared = Node { label: String::new(), descriptions: text, parent: None, direction: None, side, ..node.clone() };
        self.insert(Node { id: group.clone(), shape: Some(Shape::NoteGroup), group: true, ..shared.clone() })?;
        self.insert(Node { id: id.clone(), shape: Some(Shape::Note), group: false, parent: Some(group), ..shared })?;
        let state = node.id.clone();
        self.insert(node)?;
        let (start, end) = if note.side == Side::Left { (id, state) } else { (state, id) };
        self.edge(Edge { start, end, label: String::new(), note: true })
    }

    /// `insertOrUpdateNode`: a node already there takes the new values, its parent only when one is given.
    fn insert(&mut self, node: Node) -> Result<(), Error> {
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return Ok(());
        }
        if let Some(&place) = self.places.get(&node.id) {
            let existing = &mut self.nodes[place];
            let parent = node.parent.or_else(|| existing.parent.take());
            let side = node.side.or(existing.side);
            *existing = Node { parent, side, ..node };
            return Ok(());
        }
        if self.nodes.len() == self.limits.nodes {
            return Err(Error::TooManyNodes { nodes: self.nodes.len() + 1 });
        }
        self.places.insert(node.id.clone(), self.nodes.len());
        self.nodes.push(node);
        Ok(())
    }

    fn edge(&mut self, edge: Edge) -> Result<(), Error> {
        if self.edges.len() == self.limits.edges {
            return Err(Error::TooManyEdges { edges: self.edges.len() + 1 });
        }
        self.edges.push(edge);
        Ok(())
    }
}

/// The first description is the label, the others lines under it; a group may only have its label.
fn described(node: Node) -> Result<Node, Error> {
    let mut lines = node.descriptions.into_iter();
    let label = lines.next().unwrap_or_default();
    let descriptions: Vec<String> = lines.collect();
    if node.group && !descriptions.is_empty() {
        return Err(Invalid::GroupDescription { id: node.id }.into());
    }
    Ok(Node { label, descriptions, ..node })
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn read(source: &str) -> Diagram {
        parse(source, Limits::DRAWN).unwrap()
    }

    fn ids(diagram: &Diagram) -> Vec<(&str, Option<Shape>, Option<&str>)> {
        diagram.nodes.iter().map(|node| (node.id.as_str(), node.shape, node.parent.as_deref())).collect()
    }

    #[test]
    fn start_and_end_dots_are_named_after_their_document() {
        let diagram = read("stateDiagram-v2\n[*] --> A\nstate A {\n  [*] --> B\n  B --> [*]\n}\n");

        assert_eq!(
            ids(&diagram),
            [
                ("root_start", Some(Shape::Start), None),
                ("A", Some(Shape::RoundedWithTitle), None),
                ("A_start", Some(Shape::Start), Some("A")),
                ("B", Some(Shape::Rect), Some("A")),
                ("A_end", Some(Shape::End), Some("A")),
            ]
        );
    }

    #[test]
    fn a_body_with_dividers_becomes_regions_the_last_named_by_count() {
        let diagram = read("stateDiagram-v2\nstate A {\n  B\n  --\n  C\n}\n");

        assert_eq!(
            ids(&diagram),
            [
                ("A", Some(Shape::RoundedWithTitle), None),
                ("divider-id-1", Some(Shape::Divider), Some("A")),
                ("B", Some(Shape::Rect), Some("divider-id-1")),
                ("id-region-1", Some(Shape::Divider), Some("A")),
                ("C", Some(Shape::Rect), Some("id-region-1")),
            ]
        );
    }

    #[test]
    fn a_body_ending_with_a_divider_keeps_it_as_a_state() {
        let diagram = read("stateDiagram-v2\nstate A {\n  B\n  --\n}\n");

        assert_eq!(ids(&diagram)[1..], [("B", Some(Shape::Rect), Some("A")), ("divider-id-1", Some(Shape::Divider), Some("A"))]);
    }

    #[test]
    fn a_note_stands_in_a_group_of_its_own_linked_to_its_state() {
        let diagram = read("stateDiagram-v2\nA --> B\nnote left of B : hi\n");

        assert_eq!(
            ids(&diagram)[2..],
            [("B----parent", Some(Shape::NoteGroup), None), ("B----note-1", Some(Shape::Note), Some("B----parent"))]
        );
        assert_eq!(diagram.nodes[3].label, "hi");
        assert_eq!(diagram.edges[1], Edge { start: "B----note-1".to_owned(), end: "B".to_owned(), label: String::new(), note: true });
    }

    #[test]
    fn descriptions_after_the_first_are_lines_under_the_label() {
        let diagram = read("stateDiagram-v2\nA : one\nA : two\n");

        assert_eq!((diagram.nodes[0].label.as_str(), diagram.nodes[0].descriptions.as_slice()), ("one", ["two".to_owned()].as_slice()));
        assert_eq!(diagram.nodes[0].shape, Some(Shape::RectWithTitle));
    }

    #[test]
    fn a_composite_state_with_two_descriptions_is_refused() {
        let refused = parse("stateDiagram-v2\nA : one\nA : two\nstate A {\n  B\n}\n", Limits::DRAWN);

        assert!(matches!(refused, Err(Error::Invalid { .. })));
    }

    #[test]
    fn a_composite_state_takes_the_last_direction_of_its_body() {
        let diagram = read("stateDiagram-v2\ndirection LR\nstate A {\n  direction BT\n  B\n}\n");

        assert_eq!((diagram.direction, diagram.nodes[0].direction), (Direction::LR, Some(Direction::BT)));
    }
}
