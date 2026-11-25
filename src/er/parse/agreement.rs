//! Agreement with mermaid itself on every corpus ER diagram, from the oracle's committed results: level A (accepts and
//! refuses the same sources) and level B (the same entities, attributes, relationships and subgraphs).
//!
//! `cargo test --lib er::parse::agreement -- --nocapture` prints the scores and every disagreement.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::db::ErDiagram;
use super::lexer::{Cardinality, Direction};
use super::{Limits, parse};
use crate::source::sanitized;

/// Mermaid has no limit of its own on an ER diagram.
const MERMAID: Limits = Limits { bytes: usize::MAX, nodes: usize::MAX, edges: usize::MAX };

#[derive(Debug)]
struct Scores {
    cases: usize,
    level_a: usize,
    compared: usize,
    level_b: usize,
    disagreements: Vec<String>,
}

#[test]
fn the_parser_agrees_with_mermaid_on_the_corpus() {
    let scores = scores();

    println!("level A: {}/{} cases", scores.level_a, scores.cases);
    println!("level B: {}/{} cases both accept", scores.level_b, scores.compared);
    println!("{}", scores.disagreements.join("\n"));
    assert_eq!(scores.level_a, scores.cases);
    assert!(scores.level_b * 100 >= scores.compared * 98, "level B {}/{}", scores.level_b, scores.compared);
}

fn scores() -> Scores {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let verdicts = json(&root.join("oracle/results/parse.json"));
    let meanings = json(&root.join("oracle/results/er.json"));
    let mut scores = Scores { cases: 0, level_a: 0, compared: 0, level_b: 0, disagreements: Vec::new() };
    for (path, verdict) in verdicts["cases"].as_object().unwrap().iter().filter(|(path, _)| path.starts_with("er/")) {
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
            let (ours, theirs) = (meaning_of(diagram), comparable(meaning));
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

fn text(value: &Value) -> String {
    value.as_str().map_or_else(|| "-".to_owned(), sanitized)
}

/// The oracle's meaning, each part one line, texts made comparable.
fn comparable(meaning: &Value) -> Vec<String> {
    let mut lines = vec![format!("direction {}", text(&meaning["direction"]))];
    lines.extend(meaning["entities"].as_array().unwrap().iter().map(|entity| {
        let attributes: Vec<String> = entity["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|attribute| {
                let keys: Vec<String> = attribute["keys"].as_array().unwrap().iter().map(text).collect();
                format!("{:?} {:?} {keys:?} {:?}", text(&attribute["type"]), text(&attribute["name"]), text(&attribute["comment"]))
            })
            .collect();
        format!("entity {:?} {:?} {:?} {attributes:?}", text(&entity["name"]), text(&entity["id"]), text(&entity["alias"]))
    }));
    lines.extend(meaning["relationships"].as_array().unwrap().iter().map(|relationship| {
        format!(
            "relationship {:?} {:?} {:?} {} {} {}",
            text(&relationship["entityA"]),
            text(&relationship["entityB"]),
            text(&relationship["roleA"]),
            text(&relationship["cardA"]),
            text(&relationship["cardB"]),
            text(&relationship["relType"])
        )
    }));
    lines.extend(meaning["subgraphs"].as_array().unwrap().iter().map(|subgraph| {
        let nodes: Vec<String> = subgraph["nodes"].as_array().unwrap().iter().map(text).collect();
        format!("subgraph {:?} {:?} {nodes:?} {}", text(&subgraph["id"]), text(&subgraph["title"]), text(&subgraph["dir"]))
    }));
    lines
}

fn meaning_of(diagram: &ErDiagram) -> Vec<String> {
    let mut lines = vec![format!("direction {}", direction(Some(diagram.direction)))];
    lines.extend(diagram.entities.iter().map(|entity| {
        let attributes: Vec<String> = entity
            .attributes
            .iter()
            .map(|attribute| {
                let keys: Vec<String> = attribute.keys.iter().map(|key| sanitized(key)).collect();
                format!("{:?} {:?} {keys:?} {:?}", sanitized(&attribute.kind), sanitized(&attribute.name), sanitized(&attribute.comment))
            })
            .collect();
        format!("entity {:?} {:?} {:?} {attributes:?}", sanitized(&entity.name), sanitized(&entity.id), sanitized(&entity.alias))
    }));
    lines.extend(diagram.relationships.iter().map(|relationship| {
        format!(
            "relationship {:?} {:?} {:?} {} {} {}",
            sanitized(&relationship.from),
            sanitized(&relationship.to),
            sanitized(&relationship.role),
            cardinality(relationship.to_cardinality),
            cardinality(relationship.from_cardinality),
            if relationship.identifying { "IDENTIFYING" } else { "NON_IDENTIFYING" }
        )
    }));
    lines.extend(diagram.subgraphs.iter().map(|subgraph| {
        let nodes: Vec<String> = subgraph.nodes.iter().map(|node| sanitized(node)).collect();
        format!("subgraph {:?} {:?} {nodes:?} {}", sanitized(&subgraph.id), sanitized(&subgraph.title), direction(subgraph.direction))
    }));
    lines
}

fn direction(direction: Option<Direction>) -> &'static str {
    match direction {
        None => "-",
        Some(Direction::TB) => "TB",
        Some(Direction::BT) => "BT",
        Some(Direction::RL) => "RL",
        Some(Direction::LR) => "LR",
    }
}

fn cardinality(cardinality: Cardinality) -> &'static str {
    match cardinality {
        Cardinality::ZeroOrOne => "ZERO_OR_ONE",
        Cardinality::ZeroOrMore => "ZERO_OR_MORE",
        Cardinality::OneOrMore => "ONE_OR_MORE",
        Cardinality::OnlyOne => "ONLY_ONE",
        Cardinality::MdParent => "MD_PARENT",
    }
}

/// The lines of `one` missing from `other`.
fn difference(one: &[String], other: &[String]) -> String {
    let missing: Vec<&str> = one.iter().filter(|line| !other.contains(line)).map(String::as_str).collect();
    if missing.is_empty() { "(same lines, another order)".to_owned() } else { missing.join(" | ") }
}
