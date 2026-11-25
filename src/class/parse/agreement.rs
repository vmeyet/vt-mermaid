//! Agreement with mermaid itself on every corpus class diagram, from the oracle's committed results: level A (accepts
//! and refuses the same sources) and level B (the same classes, members, namespaces, relations and notes).
//!
//! `cargo test --lib class::parse::agreement -- --nocapture` prints the scores and every disagreement.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::db::ClassDiagram;
use super::lexer::{Direction, End};
use super::member::Member;
use super::{Limits, parse};
use crate::source::sanitized;

/// Mermaid has no limit of its own on a class diagram.
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
    let meanings = json(&root.join("oracle/results/class.json"));
    let mut scores = Scores { cases: 0, level_a: 0, compared: 0, level_b: 0, disagreements: Vec::new() };
    for (path, verdict) in verdicts["cases"].as_object().unwrap().iter().filter(|(path, _)| path.starts_with("class/")) {
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

fn list(value: &Value) -> Vec<String> {
    value.as_array().unwrap().iter().map(text).collect()
}

/// The oracle's meaning, each part one line, texts made comparable.
fn comparable(meaning: &Value) -> Vec<String> {
    let member = |member: &Value| {
        format!(
            "{} {:?} {} {:?} {:?}",
            text(&member["visibility"]),
            text(&member["id"]),
            text(&member["classifier"]),
            text(&member["parameters"]),
            text(&member["returnType"])
        )
    };
    let mut lines = vec![format!("direction {}", text(&meaning["direction"]))];
    lines.extend(meaning["classes"].as_array().unwrap().iter().map(|class| {
        let members: Vec<String> = class["members"].as_array().unwrap().iter().map(member).collect();
        let methods: Vec<String> = class["methods"].as_array().unwrap().iter().map(member).collect();
        format!(
            "class {:?} {:?} {:?} {:?} {members:?} {methods:?} {}",
            text(&class["id"]),
            text(&class["type"]),
            text(&class["label"]),
            list(&class["annotations"]),
            text(&class["parent"])
        )
    }));
    lines.extend(meaning["namespaces"].as_array().unwrap().iter().map(|namespace| {
        format!(
            "namespace {:?} {:?} {} {:?} {:?}",
            text(&namespace["id"]),
            text(&namespace["label"]),
            text(&namespace["parent"]),
            list(&namespace["classes"]),
            list(&namespace["notes"])
        )
    }));
    lines.extend(meaning["relations"].as_array().unwrap().iter().map(|relation| {
        format!(
            "relation {:?} {:?} {} {} {} {:?} {:?} {:?}",
            text(&relation["id1"]),
            text(&relation["id2"]),
            relation["type1"],
            relation["type2"],
            relation["lineType"],
            text(&relation["title1"]),
            text(&relation["title2"]),
            text(&relation["title"])
        )
    }));
    lines.extend(
        meaning["notes"].as_array().unwrap().iter().map(|note| {
            format!("note {} {:?} {:?} {}", text(&note["id"]), text(&note["class"]), text(&note["text"]), text(&note["parent"]))
        }),
    );
    lines
}

fn meaning_of(diagram: &ClassDiagram) -> Vec<String> {
    let optional = |value: Option<&str>| value.map_or_else(|| "-".to_owned(), sanitized);
    let member = |member: &Member| {
        let call = member.call.as_ref();
        format!(
            "{} {:?} {} {:?} {:?}",
            sanitized(&member.visibility),
            sanitized(&member.name),
            sanitized(&member.classifier),
            optional(call.map(|call| call.parameters.as_str())),
            optional(call.map(|call| call.returns.as_str()))
        )
    };
    let direction = match diagram.direction {
        Direction::TB => "TB",
        Direction::BT => "BT",
        Direction::RL => "RL",
        Direction::LR => "LR",
    };
    let mut lines = vec![format!("direction {direction}")];
    lines.extend(diagram.classes.iter().map(|class| {
        let members: Vec<String> = class.attributes.iter().map(member).collect();
        let methods: Vec<String> = class.methods.iter().map(member).collect();
        let annotations: Vec<String> = class.annotations.iter().map(|annotation| sanitized(annotation)).collect();
        format!(
            "class {:?} {:?} {:?} {annotations:?} {members:?} {methods:?} {}",
            sanitized(&class.id),
            sanitized(&class.generic),
            sanitized(&class.label),
            optional(class.namespace.as_deref())
        )
    }));
    lines.extend(diagram.namespaces.iter().map(|namespace| {
        let classes: Vec<String> = namespace.classes.iter().map(|class| sanitized(class)).collect();
        format!(
            "namespace {:?} {:?} {} {classes:?} {:?}",
            sanitized(&namespace.id),
            sanitized(&namespace.label),
            optional(namespace.parent.as_deref()),
            namespace.notes
        )
    }));
    lines.extend(diagram.relations.iter().map(|relation| {
        format!(
            "relation {:?} {:?} {} {} {} {:?} {:?} {:?}",
            sanitized(&relation.from),
            sanitized(&relation.to),
            end(relation.from_end),
            end(relation.to_end),
            u8::from(relation.dotted),
            sanitized(&relation.from_text),
            sanitized(&relation.to_text),
            optional(relation.label.as_deref())
        )
    }));
    lines.extend(diagram.notes.iter().map(|note| {
        format!(
            "note {} {:?} {:?} {}",
            note.id,
            optional(note.class.as_deref()),
            sanitized(&note.text),
            optional(note.namespace.as_deref())
        )
    }));
    lines
}

/// Mermaid's `relationType` number, or `"none"` as JSON writes it.
fn end(end: Option<End>) -> String {
    match end {
        None => "\"none\"".to_owned(),
        Some(End::Aggregation) => "0".to_owned(),
        Some(End::Extension) => "1".to_owned(),
        Some(End::Composition) => "2".to_owned(),
        Some(End::Dependency) => "3".to_owned(),
        Some(End::Lollipop) => "4".to_owned(),
    }
}

/// The lines of `one` missing from `other`.
fn difference(one: &[String], other: &[String]) -> String {
    let missing: Vec<&str> = one.iter().filter(|line| !other.contains(line)).map(String::as_str).collect();
    if missing.is_empty() { "(same lines, another order)".to_owned() } else { missing.join(" | ") }
}
