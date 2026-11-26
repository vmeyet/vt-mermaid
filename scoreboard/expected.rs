//! What each diagram means, from the oracle's level B results: the labels a drawing must show and its arrowheads.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::lines::ARROWS;

/// The diagram kinds on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Flowchart,
    Sequence,
    State,
    Class,
    Er,
}

impl Kind {
    /// Its folder under `corpus/` and its oracle file's name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::State => "state",
            Self::Class => "class",
            Self::Er => "er",
        }
    }

    /// What one input is called on the board.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence diagram",
            Self::State => "state diagram",
            Self::Class => "class diagram",
            Self::Er => "ER diagram",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        [Self::Flowchart, Self::Sequence, Self::State, Self::Class, Self::Er].into_iter().find(|kind| kind.name() == name)
    }

    /// Its folder under `scoreboard/results/`: flowcharts were there first, at the top.
    pub(crate) fn results(self) -> &'static str {
        match self {
            Self::Flowchart => "",
            Self::Sequence => "sequence",
            Self::State => "state",
            Self::Class => "class",
            Self::Er => "er",
        }
    }

    /// The glyphs counted as heads: arrowheads, and for a class diagram its triangles and diamonds too; none for an ER
    /// diagram, as every tool draws a crow's foot its own way.
    pub(crate) fn heads(self) -> &'static str {
        match self {
            Self::Flowchart | Self::Sequence | Self::State => ARROWS,
            Self::Class => CLASS_HEADS,
            Self::Er => "",
        }
    }

    /// Whether vt-mermaid measures its drawings of this kind exactly, from their geometry.
    pub(crate) fn measured(self) -> bool {
        matches!(self, Self::Flowchart | Self::State | Self::Class | Self::Er)
    }

    /// Whether a crossing glyph counts only when lines join it on its four sides: a crow's foot draws its bars as `┼`
    /// across a line.
    pub(crate) fn crossings_joined_all_round(self) -> bool {
        self == Self::Er
    }
}

/// Arrowheads, then the triangles and diamonds class relations end on, in any tool's glyphs.
const CLASS_HEADS: &str = "►▶▸▷→◄◀◂◁←▲▴△↑▼▾▽↓▵▹▿◃⬥⬦◆◇♦♢◊";

/// The labels a drawing of one input must show, each as the words it is made of, and how many arrowheads it has.
///
/// A sequence diagram's participants count as nodes, its messages and notes as edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Expected {
    pub(crate) node_labels: Vec<Vec<String>>,
    pub(crate) edge_labels: Vec<Vec<String>>,
    pub(crate) arrows: usize,
}

#[derive(Debug, Deserialize)]
struct Results<C> {
    cases: BTreeMap<String, C>,
}

#[derive(Debug, Deserialize)]
struct Case {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
struct Node {
    text: String,
}

#[derive(Debug, Deserialize)]
struct Edge {
    text: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Sequence {
    actors: Vec<Actor>,
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct Actor {
    description: String,
}

/// An entry of mermaid's `messages`: a message, a note, a block's start or end, an activation…
#[derive(Debug, Deserialize)]
struct Message {
    message: serde_json::Value,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct State {
    nodes: Vec<StateNode>,
    edges: Vec<StateEdge>,
}

#[derive(Debug, Deserialize)]
struct StateNode {
    shape: Option<String>,
    label: Option<String>,
    description: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StateEdge {
    label: String,
    arrowhead: String,
}

/// The shapes of a state diagram that show their label: a dot, a bar, a choice, a region and a note's group do not.
const LABELLED_SHAPES: [&str; 5] = ["rect", "rectWithTitle", "roundedWithTitle", "note", "-"];

#[derive(Debug, Deserialize)]
struct Class {
    classes: Vec<ClassBox>,
    relations: Vec<Relation>,
    notes: Vec<Note>,
}

#[derive(Debug, Deserialize)]
struct ClassBox {
    label: String,
    members: Vec<Member>,
    methods: Vec<Member>,
}

#[derive(Debug, Deserialize)]
struct Member {
    id: String,
    parameters: Option<String>,
    #[serde(rename = "returnType")]
    returns: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Relation {
    type1: serde_json::Value,
    type2: serde_json::Value,
    title1: String,
    title2: String,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Note {
    text: String,
}

#[derive(Debug, Deserialize)]
struct Er {
    entities: Vec<ErEntity>,
    relationships: Vec<ErRelationship>,
}

#[derive(Debug, Deserialize)]
struct ErEntity {
    label: String,
    alias: String,
    attributes: Vec<ErAttribute>,
}

#[derive(Debug, Deserialize)]
struct ErAttribute {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    keys: Vec<String>,
    comment: String,
}

#[derive(Debug, Deserialize)]
struct ErRelationship {
    #[serde(rename = "roleA")]
    role: String,
}

/// Every diagram of `kind` the oracle accepted, by its path under `corpus/`.
pub(crate) fn read(results: &Path, kind: Kind) -> Result<BTreeMap<String, Expected>, Box<dyn Error>> {
    let json = fs::read_to_string(results)?;
    Ok(match kind {
        Kind::Flowchart => {
            serde_json::from_str::<Results<Case>>(&json)?.cases.into_iter().map(|(path, case)| (path, Expected::of(&case))).collect()
        }
        Kind::Sequence => serde_json::from_str::<Results<Sequence>>(&json)?
            .cases
            .into_iter()
            .map(|(path, case)| (path, Expected::of_sequence(&case)))
            .collect(),
        Kind::State => {
            serde_json::from_str::<Results<State>>(&json)?.cases.into_iter().map(|(path, case)| (path, Expected::of_state(&case))).collect()
        }
        Kind::Class => {
            serde_json::from_str::<Results<Class>>(&json)?.cases.into_iter().map(|(path, case)| (path, Expected::of_class(&case))).collect()
        }
        Kind::Er => {
            serde_json::from_str::<Results<Er>>(&json)?.cases.into_iter().map(|(path, case)| (path, Expected::of_er(&case))).collect()
        }
    })
}

impl Expected {
    /// Messages and notes are what happens: a block's text is left out, as tools draw blocks their own way.
    fn of_sequence(case: &Sequence) -> Self {
        let said = case.messages.iter().filter(|message| message.kind == "NOTE" || sequence_arrowheads(&message.kind).is_some());
        Self {
            node_labels: case.actors.iter().map(|actor| words(&actor.description)).filter(|words| !words.is_empty()).collect(),
            edge_labels: said.filter_map(|message| message.message.as_str()).map(words).filter(|words| !words.is_empty()).collect(),
            arrows: case.messages.iter().filter_map(|message| sequence_arrowheads(&message.kind)).sum(),
        }
    }

    /// A state's label and descriptions, a composite state's title and a note's text are node labels; each transition
    /// has an arrowhead, a note's link none.
    fn of_state(case: &State) -> Self {
        let labelled = case.nodes.iter().filter(|node| LABELLED_SHAPES.contains(&node.shape.as_deref().unwrap_or("-")));
        let texts = labelled.flat_map(|node| node.label.iter().chain(&node.description));
        Self {
            node_labels: texts.map(|text| words(text)).filter(|words| !words.is_empty()).collect(),
            edge_labels: case.edges.iter().map(|edge| words(&edge.label)).filter(|words| !words.is_empty()).collect(),
            arrows: case.edges.iter().filter(|edge| edge.arrowhead == "normal").count(),
        }
    }

    /// A class's name and each member's name, parameters and return type are its labels, a generic type's `~` a
    /// space, as tools draw it `<T>`; a relation's label and the texts at its ends are edge labels, and each typed end
    /// a head.
    fn of_class(case: &Class) -> Self {
        let generic = |text: &str| words(&text.replace('~', " "));
        let member = |member: &Member| {
            let call = [member.parameters.as_deref(), member.returns.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ");
            generic(&format!("{} {call}", member.id))
        };
        let members = case.classes.iter().flat_map(|class| class.members.iter().chain(&class.methods).map(member));
        let names = case.classes.iter().map(|class| generic(&class.label));
        let notes = case.notes.iter().map(|note| words(&note.text));
        let texts = case.relations.iter().flat_map(|relation| {
            let ends = [&relation.title1, &relation.title2].into_iter().filter(|text| *text != "none").map(|text| words(text));
            ends.chain(relation.title.as_deref().map(words))
        });
        let typed = |end: &serde_json::Value| usize::from(end.is_number());
        Self {
            node_labels: names.chain(members).chain(notes).filter(|words| !words.is_empty()).collect(),
            edge_labels: texts.filter(|words| !words.is_empty()).collect(),
            arrows: case.relations.iter().map(|relation| typed(&relation.type1) + typed(&relation.type2)).sum(),
        }
    }

    /// An entity's alias, else its name, and each attribute's type, name, keys and comment are its labels; a
    /// relationship's role is an edge label; no head is counted.
    fn of_er(case: &Er) -> Self {
        let names = case.entities.iter().map(|entity| words(if entity.alias.is_empty() { &entity.label } else { &entity.alias }));
        let attributes = case.entities.iter().flat_map(|entity| {
            entity.attributes.iter().map(|attribute| {
                words(&format!("{} {} {} {}", attribute.kind, attribute.name, attribute.keys.join(" "), attribute.comment))
            })
        });
        Self {
            node_labels: names.chain(attributes).filter(|words| !words.is_empty()).collect(),
            edge_labels: case
                .relationships
                .iter()
                .map(|relationship| words(&relationship.role))
                .filter(|words| !words.is_empty())
                .collect(),
            arrows: 0,
        }
    }

    fn of(case: &Case) -> Self {
        Self {
            node_labels: case.nodes.iter().map(|node| words(&node.text)).filter(|words| !words.is_empty()).collect(),
            edge_labels: case.edges.iter().map(|edge| words(&edge.text)).filter(|words| !words.is_empty()).collect(),
            arrows: case.edges.iter().map(|edge| arrowheads(&edge.kind)).sum(),
        }
    }
}

fn arrowheads(kind: &str) -> usize {
    match kind {
        "arrow_point" => 1,
        "double_arrow_point" => 2,
        _ => 0,
    }
}

/// The arrowheads a message draws, none for an entry that is not a message: an open (`->`) or a crossed (`-x`) end
/// has none, a two-way message two.
fn sequence_arrowheads(kind: &str) -> Option<usize> {
    match kind {
        "SOLID_OPEN" | "DOTTED_OPEN" | "SOLID_CROSS" | "DOTTED_CROSS" => Some(0),
        "BIDIRECTIONAL_SOLID" | "BIDIRECTIONAL_DOTTED" => Some(2),
        _ if kind.starts_with("SOLID") || kind.starts_with("DOTTED") || kind.starts_with("STICK") => Some(1),
        _ => None,
    }
}

const BREAKS: [&str; 3] = ["<br>", "<br/>", "<br />"];

/// The words a label shows once its markup is gone, the oracle's escaped characters read back; a Font Awesome icon
/// (`fa:fa-car`) is no word, as mermaid draws an icon there.
fn words(label: &str) -> Vec<String> {
    let text = without_markup(label);
    let text = text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    text.split_whitespace().filter(|word| !is_icon(word)).map(str::to_owned).collect()
}

fn is_icon(word: &str) -> bool {
    let prefix = word.strip_prefix("fa").map(|rest| rest.strip_prefix(['b', 'k', 'l', 'r', 's']).unwrap_or(rest));
    prefix.and_then(|rest| rest.strip_prefix(":fa-")).is_some_and(|name| !name.is_empty())
}

/// Text with its markup gone, the same for the oracle's labels and every tool's drawing: `<br>` in any spelling
/// becomes a space, tags go, and so do markdown emphasis and backticks.
pub(crate) fn without_markup(text: &str) -> String {
    let text = BREAKS.iter().fold(text.to_owned(), |text, line_break| text.replace(line_break, " "));
    without_tags(&text).replace(['`', '*'], "")
}

/// A `<` that opens no tag, as in `2 < 4`, is text.
fn without_tags(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let (before, from) = rest.split_at(start);
        kept.push_str(before);
        let skipped = tag_length(from).unwrap_or_else(|| {
            kept.push('<');
            1
        });
        rest = &from[skipped..];
    }
    kept.push_str(rest);
    kept
}

/// The length of the tag `text` starts with: `<`, a letter or `/`, then up to the next `>` on the line.
fn tag_length(text: &str) -> Option<usize> {
    let opens = text[1..].starts_with(|char: char| char.is_ascii_alphabetic() || char == '/');
    let end = 1 + text[1..].find(['>', '<', '\n'])?;
    (opens && text[end..].starts_with('>')).then_some(end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_loses_its_markup_and_keeps_its_words() {
        assert_eq!(words("`The **cat**`<br>in &amp; <strong>out</strong>"), ["The", "cat", "in", "&", "out"]);
    }

    #[test]
    fn a_font_awesome_icon_is_no_word() {
        assert_eq!(words("fa:fa-car Car fab:fa-github"), ["Car"]);
    }

    #[test]
    fn every_spelling_of_a_line_break_is_a_space_and_a_tag_goes() {
        assert_eq!(without_markup("a<br>b<br/>c<br />d <b>bold</b>er"), "a b c d bolder");
    }

    #[test]
    fn an_angle_bracket_opening_no_tag_is_text() {
        assert_eq!(without_markup("5 > 3 && 2 < 4"), "5 > 3 && 2 < 4");
    }

    #[test]
    fn a_two_way_message_has_two_heads_an_open_or_crossed_one_none_and_a_block_is_no_message() {
        let heads: Vec<Option<usize>> =
            ["BIDIRECTIONAL_DOTTED", "SOLID", "STICK_ARROW_TOP_REVERSE", "DOTTED_OPEN", "SOLID_CROSS", "LOOP_START", "NOTE"]
                .iter()
                .map(|kind| sequence_arrowheads(kind))
                .collect();

        assert_eq!(heads, [Some(2), Some(1), Some(1), Some(0), Some(0), None, None]);
    }

    #[test]
    fn a_state_shows_its_label_and_descriptions_and_a_start_dot_shows_nothing() {
        let json = r#"{"nodes": [
            {"shape": "stateStart", "label": "root_start", "description": []},
            {"shape": "rectWithTitle", "label": "Idle", "description": ["waits"]},
            {"shape": "note", "label": "a note", "description": []}
        ], "edges": [{"label": "go", "arrowhead": "normal"}, {"label": "", "arrowhead": "none"}]}"#;

        let expected = Expected::of_state(&serde_json::from_str(json).unwrap());

        assert_eq!(expected.node_labels, [vec!["Idle"], vec!["waits"], vec!["a", "note"]]);
        assert_eq!((expected.edge_labels.len(), expected.arrows), (1, 1));
    }

    #[test]
    fn a_double_arrow_has_two_heads_and_an_open_line_none() {
        assert_eq!([arrowheads("double_arrow_point"), arrowheads("arrow_point"), arrowheads("arrow_open")], [2, 1, 0]);
    }
}
