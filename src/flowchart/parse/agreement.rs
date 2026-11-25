//! Agreement with mermaid itself on every corpus flowchart, from the oracle's committed results: level A (accepts and
//! refuses the same sources) and level B (the same nodes, edges, labels and subgraphs).
//!
//! `cargo test --lib agreement -- --nocapture` prints the scores and every disagreement.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::db::TextKind;
use super::entity;
use super::{Flowchart, Limits, parse};
use crate::error::Error;

/// Mermaid's own limit, `maxEdges`, and no other, so the corpus is read as mermaid reads it.
const MERMAID: Limits = Limits { bytes: usize::MAX, nodes: usize::MAX, edges: 500 };

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
    let meanings = json(&root.join("oracle/results/flowchart.json"));
    let mut scores = Scores { cases: 0, level_a: 0, compared: 0, level_b: 0, disagreements: Vec::new() };
    for (path, verdict) in verdicts["cases"].as_object().unwrap().iter().filter(|(path, _)| path.starts_with("flowchart/")) {
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
        if let (Some(meaning), Ok(flowchart)) = (meanings["cases"].get(path), &parsed) {
            scores.compared += 1;
            let ours = meaning_of(flowchart);
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

/// The parts of the oracle's meaning the parser keeps, each part one line, texts made comparable.
fn comparable(meaning: &Value) -> Vec<String> {
    let text = |value: &Value| value.as_str().map_or_else(|| value.to_string(), sanitized);
    let mut lines = vec![format!("direction {}", meaning["direction"].as_str().unwrap_or_default())];
    lines.extend(meaning["nodes"].as_array().unwrap().iter().map(|node| {
        format!(
            "node {} {:?} {} {}",
            node["id"].as_str().unwrap(),
            text(&node["text"]),
            node["shape"].as_str().unwrap_or("-"),
            node["labelType"].as_str().unwrap()
        )
    }));
    lines.extend(meaning["edges"].as_array().unwrap().iter().map(|edge| {
        format!(
            "edge {} {} {} {} {} {:?} {}",
            edge["start"].as_str().unwrap(),
            edge["end"].as_str().unwrap(),
            edge["type"].as_str().unwrap(),
            edge["stroke"].as_str().unwrap(),
            edge["length"].as_u64().map_or_else(|| "-".to_owned(), |length| length.to_string()),
            text(&edge["text"]),
            edge["labelType"].as_str().unwrap()
        )
    }));
    lines.extend(meaning["subgraphs"].as_array().unwrap().iter().map(|subgraph| {
        let nodes: Vec<&str> = subgraph["nodes"].as_array().unwrap().iter().map(|node| node.as_str().unwrap()).collect();
        format!(
            "subgraph {} {:?} {nodes:?} {}",
            subgraph["id"].as_str().unwrap(),
            text(&subgraph["title"]),
            subgraph["dir"].as_str().unwrap_or("-")
        )
    }));
    lines
}

fn meaning_of(flowchart: &Flowchart) -> Vec<String> {
    let mut lines = vec![format!("direction {:?}", flowchart.direction)];
    lines.extend(flowchart.nodes.iter().map(|node| {
        format!("node {} {:?} {} {}", node.id, sanitized(&node.text.value), node.shape.as_deref().unwrap_or("-"), kind_name(node.text.kind))
    }));
    lines.extend(flowchart.edges.iter().map(|edge| {
        format!(
            "edge {} {} {} {} {} {:?} {}",
            edge.start,
            edge.end,
            edge.link.arrow.name(),
            edge.link.stroke.name(),
            edge.link.length.map_or_else(|| "-".to_owned(), |length| length.to_string()),
            sanitized(&edge.text.value),
            kind_name(edge.text.kind)
        )
    }));
    lines.extend(flowchart.subgraphs.iter().map(|subgraph| {
        let direction = subgraph.direction.map_or_else(|| "-".to_owned(), |direction| format!("{direction:?}"));
        format!("subgraph {} {:?} {:?} {direction}", subgraph.id, sanitized(&subgraph.title.value), subgraph.nodes)
    }));
    lines
}

fn kind_name(kind: TextKind) -> &'static str {
    match kind {
        TextKind::Text => "text",
        TextKind::String => "string",
        TextKind::Markdown => "markdown",
    }
}

/// Mermaid's labels go through DOMPurify, which drops unknown tags and escapes text holding a tag: compared without
/// tags and with entities decoded, as the scoreboard reads labels.
fn sanitized(text: &str) -> String {
    let mut untagged = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let is_tag = rest[open + 1..].starts_with(|char: char| char.is_ascii_alphabetic() || char == '/');
        if let Some(close) = rest[open..].find('>').filter(|_| is_tag) {
            untagged.push_str(&rest[..open]);
            rest = &rest[open + close + 1..];
        } else {
            untagged.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
        }
    }
    untagged.push_str(rest);
    entity::decoded(&untagged)
}

/// The lines of `one` missing from `other`.
fn difference(one: &[String], other: &[String]) -> String {
    let missing: Vec<&str> = one.iter().filter(|line| !other.contains(line)).map(String::as_str).collect();
    if missing.is_empty() { "(same lines, another order)".to_owned() } else { missing.join(" | ") }
}

#[test]
fn a_source_over_the_edge_limit_is_refused_as_mermaid_refuses_it() {
    let source = format!("graph TD\n{}", "A --> B\n".repeat(501));

    assert!(matches!(parse(&source, MERMAID), Err(Error::TooManyEdges { edges: 501 })));
}
