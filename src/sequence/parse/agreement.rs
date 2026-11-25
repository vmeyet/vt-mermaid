//! Agreement with mermaid itself on every corpus sequence diagram, from the oracle's committed results: level A
//! (accepts and refuses the same sources) and level B (the same participants, messages and boxes).
//!
//! `cargo test --lib sequence::parse::agreement -- --nocapture` prints the scores and every disagreement.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::db::{Item, Placement, Sequence, Text};
use super::lexer::Block;
use super::{Limits, parse};
use crate::source::sanitized;

/// Mermaid has no limit of its own on a sequence diagram.
const MERMAID: Limits = Limits { bytes: usize::MAX, participants: usize::MAX, items: usize::MAX };

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
    let meanings = json(&root.join("oracle/results/sequence.json"));
    let mut scores = Scores { cases: 0, level_a: 0, compared: 0, level_b: 0, disagreements: Vec::new() };
    for (path, verdict) in verdicts["cases"].as_object().unwrap().iter().filter(|(path, _)| path.starts_with("sequence/")) {
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
        if let (Some(meaning), Ok(sequence)) = (meanings["cases"].get(path), &parsed) {
            scores.compared += 1;
            let ours = meaning_of(sequence);
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

/// The oracle's meaning, each part one line, texts made comparable.
fn comparable(meaning: &Value) -> Vec<String> {
    let text = |value: &Value| value.as_str().map_or_else(|| value.to_string(), sanitized);
    let name = |value: &Value| value.as_str().unwrap_or("-").to_owned();
    let mut lines: Vec<String> = meaning["actors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|actor| {
            format!(
                "actor {} {} {:?} {} {}",
                name(&actor["id"]),
                name(&actor["name"]),
                text(&actor["description"]),
                name(&actor["type"]),
                actor["box"].as_str().map_or_else(|| "-".to_owned(), sanitized)
            )
        })
        .collect();
    lines.extend(meaning["messages"].as_array().unwrap().iter().map(|message| {
        let content = match &message["message"] {
            Value::Object(number) => {
                let field = |key: &str| number.get(key).and_then(Value::as_f64);
                autonumber(field("start"), field("step"), number.get("visible").and_then(Value::as_bool) == Some(true))
            }
            other => format!("{:?}", text(other)),
        };
        format!(
            "message {} {} {content} {} {} {}",
            name(&message["from"]),
            name(&message["to"]),
            name(&message["type"]),
            name(&message["placement"]),
            message["activate"].as_bool().unwrap_or(false)
        )
    }));
    lines.extend(meaning["boxes"].as_array().unwrap().iter().map(|group| {
        let participants: Vec<&str> = group["actors"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
        format!("box {:?} {participants:?}", group["name"].as_str().map(sanitized))
    }));
    lines
}

fn meaning_of(sequence: &Sequence) -> Vec<String> {
    let mut lines: Vec<String> = sequence
        .participants
        .iter()
        .map(|participant| {
            let group =
                participant.group.and_then(|group| sequence.groups[group].name.as_deref()).map_or_else(|| "-".to_owned(), sanitized);
            format!("actor {} {} {:?} {} {group}", participant.id, participant.id, sanitized(&participant.description), participant.figure)
        })
        .collect();
    lines.extend(sequence.items.iter().map(item_line));
    lines.extend(sequence.groups.iter().map(|group| format!("box {:?} {:?}", group.name.as_deref().map(sanitized), group.participants)));
    lines
}

fn item_line(item: &Item) -> String {
    let text = |text: &Text| format!("{:?}", sanitized(&text.value));
    let none = || format!("{:?}", "");
    let (from, to, content, kind, placement, activate) = match item {
        Item::Message { from, to, text: message, arrow, activate } => {
            (from.as_str(), to.as_str(), text(message), arrow.name(), "-", *activate)
        }
        Item::Note { from, to, placement, text: note } => {
            (from.as_str(), to.as_str(), text(note), "NOTE", placement_name(*placement), false)
        }
        Item::Activation { id, start } => (id.as_str(), "-", none(), if *start { "ACTIVE_START" } else { "ACTIVE_END" }, "-", false),
        Item::Central { id, reverse } => {
            (id.as_str(), "-", none(), if *reverse { "CENTRAL_CONNECTION_REVERSE" } else { "CENTRAL_CONNECTION" }, "-", false)
        }
        Item::Autonumber { start, step, visible } => ("-", "-", autonumber(*start, *step, *visible), "AUTONUMBER", "-", false),
        Item::Open { block, text: title } => ("-", "-", text(title), block_name(*block, "START"), "-", false),
        Item::Section { block, text: title } => ("-", "-", text(title), section_name(*block), "-", false),
        Item::Close { block } => ("-", "-", none(), block_name(*block, "END"), "-", false),
    };
    format!("message {from} {to} {content} {kind} {placement} {activate}")
}

fn autonumber(start: Option<f64>, step: Option<f64>, visible: bool) -> String {
    let number = |value: Option<f64>| value.map_or_else(|| "-".to_owned(), |value| value.to_string());
    format!("autonumber {} {} {visible}", number(start), number(step))
}

fn placement_name(placement: Placement) -> &'static str {
    match placement {
        Placement::LeftOf => "LEFTOF",
        Placement::RightOf => "RIGHTOF",
        Placement::Over => "OVER",
    }
}

/// `LOOP_START`, `PAR_END`…: a `par_over` starts as itself and ends as a `par`.
fn block_name(block: Block, end: &str) -> &'static str {
    match (block, end) {
        (Block::Loop, "START") => "LOOP_START",
        (Block::Loop, _) => "LOOP_END",
        (Block::Rect, "START") => "RECT_START",
        (Block::Rect, _) => "RECT_END",
        (Block::Opt, "START") => "OPT_START",
        (Block::Opt, _) => "OPT_END",
        (Block::Alt, "START") => "ALT_START",
        (Block::Alt, _) => "ALT_END",
        (Block::Par, "START") => "PAR_START",
        (Block::ParOver, "START") => "PAR_OVER_START",
        (Block::Par | Block::ParOver, _) => "PAR_END",
        (Block::Critical, "START") => "CRITICAL_START",
        (Block::Critical, _) => "CRITICAL_END",
        (Block::Break, "START") => "BREAK_START",
        (Block::Break, _) => "BREAK_END",
    }
}

fn section_name(block: Block) -> &'static str {
    match block {
        Block::Critical => "CRITICAL_OPTION",
        Block::Par | Block::ParOver => "PAR_AND",
        _ => "ALT_ELSE",
    }
}

/// The lines of `one` missing from `other`.
fn difference(one: &[String], other: &[String]) -> String {
    let missing: Vec<&str> = one.iter().filter(|line| !other.contains(line)).map(String::as_str).collect();
    if missing.is_empty() { "(same lines, another order)".to_owned() } else { missing.join(" | ") }
}
