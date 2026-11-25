//! Agreement with mermaid itself on every corpus state diagram, from the oracle's committed results: level A (accepts
//! and refuses the same sources) and level B (the same nodes and edges mermaid's renderer is handed).
//!
//! `cargo test --lib state::parse::agreement -- --nocapture` prints the scores and every disagreement.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::db::{Shape, Side};
use super::lexer::Direction;
use super::{Diagram, Limits, parse};
use crate::source::sanitized;

/// Mermaid has no limit of its own on a state diagram.
const MERMAID: Limits = Limits { bytes: usize::MAX, nodes: usize::MAX, edges: usize::MAX };

#[derive(Debug)]
struct Scores {
    cases: usize,
    level_a: usize,
    compared: usize,
    level_b: usize,
    disagreements: Vec<String>,
}

/// Every case agrees today; a change that breaks one lists it.
#[test]
fn the_parser_agrees_with_mermaid_on_the_corpus() {
    let scores = scores();

    println!("level A: {}/{} cases", scores.level_a, scores.cases);
    println!("level B: {}/{} cases both accept", scores.level_b, scores.compared);
    assert!(scores.disagreements.is_empty(), "{}", scores.disagreements.join("\n"));
}

fn scores() -> Scores {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let verdicts = json(&root.join("oracle/results/parse.json"));
    let meanings = json(&root.join("oracle/results/state.json"));
    let mut scores = Scores { cases: 0, level_a: 0, compared: 0, level_b: 0, disagreements: Vec::new() };
    for (path, verdict) in verdicts["cases"].as_object().unwrap().iter().filter(|(path, _)| path.starts_with("state/")) {
        let source = fs::read_to_string(root.join("corpus").join(path)).unwrap();
        let parsed = parse(&source, MERMAID);
        let mermaid_accepts = verdict["accepted"].as_bool().unwrap();
        scores.cases += 1;
        if mermaid_accepts == parsed.is_ok() {
            scores.level_a += 1;
        } else {
            scores.disagreements.push(format!(
                "A {path}: mermaid {}, here {:?}",
                if mermaid_accepts { "accepts" } else { "refuses" },
                parsed.as_ref().err()
            ));
        }
        if let (Some(meaning), Ok(diagram)) = (meanings["cases"].get(path), &parsed) {
            scores.compared += 1;
            let ours = meaning_of(diagram);
            let theirs = comparable(meaning);
            if ours == theirs {
                scores.level_b += 1;
            } else {
                scores.disagreements.push(format!(
                    "B {path}:\n  mermaid {}\n  here    {}",
                    difference(&theirs, &ours),
                    difference(&ours, &theirs)
                ));
            }
        }
    }
    scores
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn comparable(meaning: &Value) -> Vec<String> {
    let text = |value: &Value| value.as_str().map_or_else(|| "-".to_owned(), sanitized);
    let name = |value: &Value| value.as_str().unwrap_or("-").to_owned();
    let mut lines = vec![format!("direction {}", meaning["direction"].as_str().unwrap())];
    lines.extend(meaning["nodes"].as_array().unwrap().iter().map(|node| {
        let descriptions: Vec<String> = node["description"].as_array().unwrap().iter().map(text).collect();
        format!(
            "node {} {:?} {} {descriptions:?} {} {} {} {}",
            name(&node["id"]),
            text(&node["label"]),
            name(&node["shape"]),
            name(&node["parent"]),
            node["group"].as_bool().unwrap(),
            name(&node["dir"]),
            name(&node["position"])
        )
    }));
    lines.extend(meaning["edges"].as_array().unwrap().iter().map(|edge| {
        format!("edge {} {} {:?} {}", name(&edge["start"]), name(&edge["end"]), text(&edge["label"]), name(&edge["arrowhead"]))
    }));
    lines
}

fn meaning_of(diagram: &Diagram) -> Vec<String> {
    let mut regions = Regions::default();
    let mut lines = vec![format!("direction {:?}", diagram.direction)];
    lines.extend(diagram.nodes.iter().map(|node| {
        let descriptions: Vec<String> = node.descriptions.iter().map(|line| sanitized(line)).collect();
        let id = regions.named(&node.id);
        let label = regions.named(&node.label);
        let parent = node.parent.as_deref().map_or_else(|| "-".to_owned(), |parent| regions.named(parent));
        format!(
            "node {id} {:?} {} {descriptions:?} {parent} {} {} {}",
            sanitized(&label),
            node.shape.map_or("-", shape_name),
            node.group,
            node.direction.map_or_else(|| "-".to_owned(), |direction: Direction| format!("{direction:?}")),
            node.side.map_or("-", |side| if side == Side::Left { "left of" } else { "right of" })
        )
    }));
    lines.extend(diagram.edges.iter().map(|edge| {
        let (start, end) = (regions.named(&edge.start), regions.named(&edge.end));
        format!("edge {start} {end} {:?} {}", sanitized(&edge.label), if edge.note { "none" } else { "normal" })
    }));
    lines
}

/// The oracle's names for the regions mermaid names at random, `id-<n>` in the order they appear.
#[derive(Debug, Default)]
struct Regions(HashMap<String, usize>);

impl Regions {
    fn named(&mut self, text: &str) -> String {
        let Some(start) = text.find("id-region-") else { return text.to_owned() };
        let digits = text[start + 10..].find(|char: char| !char.is_ascii_digit()).map_or(text.len(), |end| start + 10 + end);
        let count = self.0.len();
        let number = *self.0.entry(text[start..digits].to_owned()).or_insert(count + 1);
        format!("{}id-{number}{}", &text[..start], &text[digits..])
    }
}

fn shape_name(shape: Shape) -> &'static str {
    match shape {
        Shape::Rect => "rect",
        Shape::RectWithTitle => "rectWithTitle",
        Shape::Start => "stateStart",
        Shape::End => "stateEnd",
        Shape::Fork => "fork",
        Shape::Join => "join",
        Shape::Choice => "choice",
        Shape::Divider => "divider",
        Shape::RoundedWithTitle => "roundedWithTitle",
        Shape::Note => "note",
        Shape::NoteGroup => "noteGroup",
    }
}

/// The lines of `one` missing from `other`.
fn difference(one: &[String], other: &[String]) -> String {
    let missing: Vec<&str> = one.iter().filter(|line| !other.contains(line)).map(String::as_str).collect();
    if missing.is_empty() { "(same lines, another order)".to_owned() } else { missing.join(" | ") }
}
