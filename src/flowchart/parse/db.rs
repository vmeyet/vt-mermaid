//! Mermaid's flowchart db (`flowDb.ts`): what each statement adds to the flowchart, merges into it or refuses.

use std::collections::{HashMap, HashSet};

use super::entity;
use super::link::Link;
use super::prepare::Config;
use super::shape_data::{self, SHAPES, Value};
use super::{Direction, Invalid, Limits, is_space};
use crate::error::Error;

/// A flowchart as mermaid's db holds it once the source is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Flowchart {
    pub(in crate::flowchart) direction: Direction,
    /// In the order the source first names them.
    pub(in crate::flowchart) nodes: Vec<Node>,
    pub(in crate::flowchart) edges: Vec<Edge>,
    /// In the order they close: a nested subgraph comes before the one holding it.
    pub(in crate::flowchart) subgraphs: Vec<Subgraph>,
    pub(in crate::flowchart) classes: Vec<Class>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Node {
    pub(in crate::flowchart) id: String,
    pub(in crate::flowchart) text: Text,
    /// Mermaid's name for it: `square`, `round`, `diamond`… from brackets, any of [`SHAPES`] from a block.
    pub(in crate::flowchart) shape: Option<String>,
    pub(in crate::flowchart) styles: Vec<String>,
    pub(in crate::flowchart) classes: Vec<String>,
}

/// A label as written, entities still marked, and how mermaid reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Text {
    pub(in crate::flowchart) value: String,
    pub(in crate::flowchart) kind: TextKind,
}

impl Text {
    /// The label as mermaid shows it, entities decoded.
    pub(in crate::flowchart) fn shown(&self) -> String {
        entity::decoded(&self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flowchart) enum TextKind {
    Text,
    String,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Edge {
    pub(in crate::flowchart) start: String,
    pub(in crate::flowchart) end: String,
    pub(in crate::flowchart) link: Link,
    pub(in crate::flowchart) text: Text,
    pub(in crate::flowchart) styles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Subgraph {
    pub(in crate::flowchart) id: String,
    pub(in crate::flowchart) title: Text,
    /// Its own nodes and the ids of the subgraphs right inside it, each in one subgraph only.
    pub(in crate::flowchart) nodes: Vec<String>,
    pub(in crate::flowchart) direction: Option<Direction>,
    /// How many subgraphs the source opens before it.
    pub(in crate::flowchart) opened: usize,
}

/// A `classDef`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flowchart) struct Class {
    pub(in crate::flowchart) id: String,
    pub(in crate::flowchart) styles: Vec<String>,
}

/// What a statement in a subgraph's body adds to its list of nodes, as the grammar returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    Nodes(Vec<String>),
    /// A subgraph's id, or a token the grammar hands on as it is, a lone `;` or an accessibility title.
    Name(String),
    Direction(Direction),
}

/// A link statement's link, text and id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LinkData {
    pub(super) link: Link,
    pub(super) text: Option<Text>,
    pub(super) id: Option<String>,
}

/// A subgraph's header: its id and title are one text when it has no `[title]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Header {
    pub(super) id: Text,
    pub(super) title: Option<Text>,
    pub(super) opened: usize,
}

/// A `linkStyle` target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Position {
    Default,
    /// The index as written: mermaid looks it up as a property name, so `01` names no edge.
    Index(String),
}

#[derive(Debug)]
pub(super) struct Db {
    limits: Limits,
    direction: Direction,
    nodes: Vec<Node>,
    node_index: HashMap<String, usize>,
    edges: Vec<Edge>,
    edge_ids: HashSet<String>,
    subgraphs: Vec<Subgraph>,
    classes: Vec<Class>,
    untitled: usize,
    config: Config,
}

impl Db {
    pub(super) fn new(limits: Limits, config: Config) -> Self {
        Self {
            limits,
            direction: Direction::TB,
            nodes: Vec::new(),
            node_index: HashMap::new(),
            edges: Vec::new(),
            edge_ids: HashSet::new(),
            subgraphs: Vec::new(),
            classes: Vec::new(),
            untitled: 0,
            config,
        }
    }

    pub(super) fn into_flowchart(self) -> Flowchart {
        Flowchart { direction: self.direction, nodes: self.nodes, edges: self.edges, subgraphs: self.subgraphs, classes: self.classes }
    }

    pub(super) fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// `addVertex`: a node is created the first time it is named, then given what each later mention adds; a block
    /// naming a subgraph or an edge goes to it instead.
    pub(super) fn add_vertex(&mut self, id: &str, text: Option<Text>, shape: Option<&str>, block: Option<&str>) -> Result<(), Error> {
        if id.trim_matches(is_space).is_empty() {
            return Ok(());
        }
        let pairs = block.map(shape_data::read).transpose()?;
        if (pairs.is_some() && self.subgraphs.iter().any(|subgraph| subgraph.id == id)) || self.edge_ids.contains(id) {
            return Ok(());
        }
        let index = self.node(id)?;
        let node = &mut self.nodes[index];
        if let Some(text) = text {
            node.text = Text { value: unquoted(text.value.trim_matches(is_space)), kind: text.kind };
        }
        if let Some(shape) = shape {
            node.shape = Some(shape.to_owned());
        }
        if let Some(pairs) = pairs {
            with_block(node, &pairs)?;
        }
        Ok(())
    }

    /// `style A …`: the node is created if it is new.
    pub(super) fn add_styles(&mut self, id: &str, styles: Vec<String>) -> Result<(), Error> {
        if id.trim_matches(is_space).is_empty() || self.edge_ids.contains(id) {
            return Ok(());
        }
        let index = self.node(id)?;
        self.nodes[index].styles.extend(styles);
        Ok(())
    }

    fn node(&mut self, id: &str) -> Result<usize, Error> {
        if let Some(&index) = self.node_index.get(id) {
            return Ok(index);
        }
        self.check_nodes(1)?;
        let text = Text { value: id.to_owned(), kind: TextKind::Text };
        self.nodes.push(Node { id: id.to_owned(), text, shape: None, styles: Vec::new(), classes: Vec::new() });
        self.node_index.insert(id.to_owned(), self.nodes.len() - 1);
        Ok(self.nodes.len() - 1)
    }

    fn check_nodes(&self, more: usize) -> Result<(), Error> {
        let nodes = self.nodes.len() + self.subgraphs.len() + more;
        if nodes > self.limits.nodes { Err(Error::TooManyNodes { nodes }) } else { Ok(()) }
    }

    /// `addLink`: an edge from every start to every end; only the last start's edge to the first end takes the id.
    pub(super) fn add_link(&mut self, starts: &[String], ends: &[String], data: &LinkData) -> Result<(), Error> {
        let edges = self.edges.len().saturating_add(starts.len().saturating_mul(ends.len()));
        if edges > self.limits.edges {
            return Err(Error::TooManyEdges { edges });
        }
        let (last_start, first_end) = (starts.last(), ends.first());
        for start in starts {
            for end in ends {
                let id = data.id.as_deref().filter(|_| Some(start) == last_start && Some(end) == first_end);
                self.add_edge(start, end, data, id);
            }
        }
        Ok(())
    }

    fn add_edge(&mut self, start: &str, end: &str, data: &LinkData, id: Option<&str>) {
        let text = data.text.as_ref().map_or(Text { value: String::new(), kind: TextKind::Text }, |text| Text {
            value: unquoted(text.value.trim_matches(is_space)),
            kind: text.kind,
        });
        let id = if let Some(id) = id.filter(|id| !self.edge_ids.contains(*id)) {
            id.to_owned()
        } else {
            let same_ends = self.edges.iter().filter(|edge| edge.start == start && edge.end == end).count();
            format!("L_{start}_{end}_{}", if same_ends == 0 { 0 } else { same_ends + 1 })
        };
        self.edge_ids.insert(id);
        self.edges.push(Edge { start: start.to_owned(), end: end.to_owned(), link: data.link, text, styles: Vec::new() });
    }

    /// `updateLink` and `updateLinkInterpolate`: an index must name an edge already read.
    pub(super) fn style_links(&mut self, positions: &[Position], styles: &[String]) -> Result<(), Error> {
        for position in positions {
            let Position::Index(written) = position else { continue };
            let index = written.parse::<usize>().ok().filter(|index| index.to_string() == *written && *index < self.edges.len());
            let edge = index.and_then(|index| self.edges.get_mut(index)).ok_or_else(|| Invalid::LinkStyle { index: written.clone() })?;
            edge.styles = styles.to_vec();
        }
        Ok(())
    }

    /// `addClass`: each comma-separated id gets the styles, split at commas not escaped.
    pub(super) fn add_class(&mut self, ids: &str, styles: &[String]) {
        let joined = styles.join(",").replace("\\,", "\u{0}").replace(',', ";").replace('\u{0}', ",");
        let styles: Vec<String> = joined.split(';').map(str::to_owned).collect();
        for id in ids.split(',') {
            match self.classes.iter_mut().find(|class| class.id == id) {
                Some(class) => class.styles.extend(styles.iter().cloned()),
                None => self.classes.push(Class { id: id.to_owned(), styles: styles.clone() }),
            }
        }
    }

    /// `setClass`: each comma-separated node gets the class.
    pub(super) fn set_class(&mut self, ids: &str, class: &str) {
        for id in ids.split(',') {
            if let Some(&index) = self.node_index.get(id) {
                self.nodes[index].classes.push(class.to_owned());
            }
        }
    }

    pub(super) fn check_subgraph(&self) -> Result<(), Error> {
        self.check_nodes(1)
    }

    /// `addSubGraph`: an id with a space in a header without `[title]` is no id; a node another subgraph already holds
    /// stays there; a repeated id adds its nodes to the first.
    pub(super) fn add_subgraph(&mut self, header: Header, items: Vec<Item>) -> Result<String, Error> {
        self.check_subgraph()?;
        let (nodes, direction) = listed(items);
        let direction = direction.or(self.config.inherits_direction.then_some(self.direction));
        let id = if header.title.is_none() && header.id.value.contains(is_space) {
            format!("subGraph{}", self.untitled)
        } else {
            header.id.value.trim_matches(is_space).to_owned()
        };
        let title = header.title.unwrap_or(header.id);
        self.untitled += 1;
        let kept: Vec<String> = nodes
            .into_iter()
            .filter(|node| !self.subgraphs.iter().any(|subgraph| subgraph.nodes.contains(node)))
            .filter(|node| *node != id)
            .collect();
        if let Some(existing) = self.subgraphs.iter_mut().find(|subgraph| subgraph.id == id) {
            existing.nodes.extend(kept);
        } else {
            let title = Text { value: title.value.trim_matches(is_space).to_owned(), kind: title.kind };
            self.subgraphs.push(Subgraph { id: id.clone(), title, nodes: kept, direction, opened: header.opened });
        }
        Ok(id)
    }
}

/// A node's block: its shape, checked; its label, and an icon or image without a label leaves the node unnamed.
fn with_block(node: &mut Node, pairs: &[(String, Value)]) -> Result<(), Error> {
    let get = |key: &str| pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value).filter(|value| value.is_truthy());
    if let Some(shape) = get("shape") {
        let Value::Text(name) = shape else { return Err(Invalid::Shape { name: format!("{shape:?}") }.into()) };
        if !SHAPES.contains(&name.as_str()) {
            return Err(Invalid::Shape { name: name.clone() }.into());
        }
        node.shape = Some(name.clone());
    }
    let label = get("label").and_then(Value::text);
    if let Some(label) = &label {
        let kind = match get("labelType").and_then(Value::text).as_deref() {
            Some("string") => TextKind::String,
            Some("text") => TextKind::Text,
            _ => TextKind::Markdown,
        };
        node.text = Text { value: label.clone(), kind };
    }
    let unlabelled = label.as_deref().is_none_or(|label| label.trim_matches(is_space).is_empty());
    if (get("icon").is_some() || get("img").is_some()) && unlabelled && node.text.value == node.id {
        node.text.value = String::new();
    }
    Ok(())
}

/// A text wholly inside quotes loses them.
fn unquoted(text: &str) -> String {
    match text.strip_prefix('"') {
        Some(inner) if text.ends_with('"') => inner.strip_suffix('"').unwrap_or_default().to_owned(),
        _ => text.to_owned(),
    }
}

/// The nodes a subgraph's body lists, each once and none blank, and the last direction it names.
fn listed(items: Vec<Item>) -> (Vec<String>, Option<Direction>) {
    let mut nodes: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut direction = None;
    for item in items {
        let names = match item {
            Item::Nodes(names) => names,
            Item::Name(name) => vec![name],
            Item::Direction(named) => {
                direction = Some(named);
                continue;
            }
        };
        nodes.extend(names.into_iter().filter(|name| !name.trim_matches(is_space).is_empty() && seen.insert(name.clone())));
    }
    (nodes, direction)
}

#[cfg(test)]
mod tests {
    use super::super::link;
    use super::*;

    const LIMITS: Limits = Limits { bytes: usize::MAX, nodes: 10, edges: 10 };

    fn text(value: &str) -> Text {
        Text { value: value.to_owned(), kind: TextKind::String }
    }

    fn arrow() -> LinkData {
        LinkData { link: link::read("-->", None), text: None, id: None }
    }

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn a_node_is_created_once_and_keeps_what_each_mention_adds() {
        let mut db = Db::new(LIMITS, Config::default());

        db.add_vertex("A", None, None, None).unwrap();
        db.add_vertex("A", Some(text(" \"Hello\" ")), Some("round"), None).unwrap();
        db.add_vertex("A", None, None, None).unwrap();

        let flowchart = db.into_flowchart();
        assert_eq!(flowchart.nodes.len(), 1);
        assert_eq!((flowchart.nodes[0].text.value.as_str(), flowchart.nodes[0].shape.as_deref()), ("Hello", Some("round")));
    }

    #[test]
    fn a_link_makes_an_edge_from_every_start_to_every_end_and_one_takes_its_id() {
        let mut db = Db::new(LIMITS, Config::default());
        let data = LinkData { id: Some("e1".to_owned()), ..arrow() };

        db.add_link(&names(&["A", "B"]), &names(&["C", "D"]), &data).unwrap();
        db.add_vertex("e1", None, None, None).unwrap();

        let flowchart = db.into_flowchart();
        let ends: Vec<(&str, &str)> = flowchart.edges.iter().map(|edge| (edge.start.as_str(), edge.end.as_str())).collect();
        assert_eq!(ends, [("A", "C"), ("A", "D"), ("B", "C"), ("B", "D")]);
        assert!(flowchart.nodes.is_empty(), "an edge's id names no node");
    }

    #[test]
    fn edges_over_the_limit_are_refused_before_they_are_made() {
        let mut db = Db::new(LIMITS, Config::default());

        assert_eq!(db.add_link(&names(&["A", "B", "C"]), &names(&["D", "E", "F", "G"]), &arrow()), Err(Error::TooManyEdges { edges: 12 }));
        assert_eq!(db.into_flowchart().edges, []);
    }

    #[test]
    fn a_subgraph_keeps_only_the_nodes_no_earlier_subgraph_holds() {
        let mut db = Db::new(LIMITS, Config::default());
        let header = |id: &str| Header { id: Text { value: id.to_owned(), kind: TextKind::Text }, title: None, opened: 0 };

        db.add_subgraph(header("one"), vec![Item::Nodes(names(&["B", "A"]))]).unwrap();
        db.add_subgraph(header("two"), vec![Item::Nodes(names(&["A", "C"])), Item::Name(";".to_owned()), Item::Name("\n".to_owned())])
            .unwrap();
        db.add_subgraph(header("one"), vec![Item::Nodes(names(&["D"]))]).unwrap();
        db.add_subgraph(header("has space"), Vec::new()).unwrap();

        let subgraphs: Vec<(String, Vec<String>)> =
            db.into_flowchart().subgraphs.into_iter().map(|subgraph| (subgraph.id, subgraph.nodes)).collect();
        assert_eq!(
            subgraphs,
            [("one".to_owned(), names(&["B", "A", "D"])), ("two".to_owned(), names(&["C", ";"])), ("subGraph3".to_owned(), Vec::new())]
        );
    }

    #[test]
    fn a_link_style_must_name_an_edge_as_written() {
        let mut db = Db::new(LIMITS, Config::default());
        db.add_link(&names(&["A"]), &names(&["B"]), &arrow()).unwrap();

        assert_eq!(db.style_links(&[Position::Index("0".to_owned()), Position::Default], &[]), Ok(()));
        assert!(db.style_links(&[Position::Index("1".to_owned())], &[]).is_err());
        assert!(db.style_links(&[Position::Index("00".to_owned())], &[]).is_err());
    }

    #[test]
    fn a_subgraph_takes_its_last_direction_or_with_inherit_dir_the_flowcharts() {
        let header = Header { id: Text { value: "one".to_owned(), kind: TextKind::Text }, title: None, opened: 0 };
        let directions = |inherits: bool, items: Vec<Item>| {
            let mut db = Db::new(LIMITS, Config { inherits_direction: inherits });
            db.set_direction(Direction::LR);
            db.add_subgraph(header.clone(), items).unwrap();
            db.into_flowchart().subgraphs[0].direction
        };

        assert_eq!(directions(false, vec![Item::Direction(Direction::BT), Item::Direction(Direction::RL)]), Some(Direction::RL));
        assert_eq!(directions(false, Vec::new()), None);
        assert_eq!(directions(true, Vec::new()), Some(Direction::LR));
    }

    #[test]
    fn styles_and_classes_are_kept() {
        let mut db = Db::new(LIMITS, Config::default());

        db.add_styles("A", names(&["fill:#f00"])).unwrap();
        db.add_class("hot,cold", &names(&["fill:#f00", "stroke:a\\,b"]));
        db.set_class("A,B", "hot");

        let flowchart = db.into_flowchart();
        assert_eq!(
            (flowchart.nodes[0].styles.as_slice(), flowchart.nodes[0].classes.as_slice()),
            (names(&["fill:#f00"]).as_slice(), names(&["hot"]).as_slice())
        );
        assert_eq!(flowchart.classes[1], Class { id: "cold".to_owned(), styles: names(&["fill:#f00", "stroke:a,b"]) });
    }

    #[test]
    fn a_block_sets_the_shape_and_label_and_refuses_an_unknown_shape() {
        let mut db = Db::new(LIMITS, Config::default());

        db.add_vertex("A", None, None, Some(" shape: hex, label: Six ")).unwrap();
        db.add_vertex("B", None, None, Some(" icon: 'fa:user' ")).unwrap();

        assert!(db.add_vertex("C", None, None, Some(" shape: Hex ")).is_err());
        let nodes = db.into_flowchart().nodes;
        assert_eq!((nodes[0].shape.as_deref(), nodes[0].text.value.as_str(), nodes[0].text.kind), (Some("hex"), "Six", TextKind::Markdown));
        assert_eq!(nodes[1].text.value, "");
    }
}
