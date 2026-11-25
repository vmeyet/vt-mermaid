//! What each flowchart means, from the oracle's level B results: the labels a drawing must show and its arrowheads.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use serde::Deserialize;

/// The labels a drawing of one input must show, each as the words it is made of, and how many arrowheads it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Expected {
    pub(crate) node_labels: Vec<Vec<String>>,
    pub(crate) edge_labels: Vec<Vec<String>>,
    pub(crate) arrows: usize,
}

#[derive(Debug, Deserialize)]
struct Results {
    cases: BTreeMap<String, Case>,
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

/// Every flowchart the oracle accepted, by its path under `corpus/`.
pub(crate) fn read(results: &Path) -> Result<BTreeMap<String, Expected>, Box<dyn Error>> {
    let results: Results = serde_json::from_str(&fs::read_to_string(results)?)?;
    Ok(results.cases.into_iter().map(|(path, case)| (path, Expected::of(&case))).collect())
}

impl Expected {
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

const BREAKS: [&str; 3] = ["<br>", "<br/>", "<br />"];

/// The words a label shows once its markup is gone, the oracle's escaped characters read back.
fn words(label: &str) -> Vec<String> {
    let text = without_markup(label);
    let text = text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    text.split_whitespace().map(str::to_owned).collect()
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
    fn every_spelling_of_a_line_break_is_a_space_and_a_tag_goes() {
        assert_eq!(without_markup("a<br>b<br/>c<br />d <b>bold</b>er"), "a b c d bolder");
    }

    #[test]
    fn an_angle_bracket_opening_no_tag_is_text() {
        assert_eq!(without_markup("5 > 3 && 2 < 4"), "5 > 3 && 2 < 4");
    }

    #[test]
    fn a_double_arrow_has_two_heads_and_an_open_line_none() {
        assert_eq!([arrowheads("double_arrow_point"), arrowheads("arrow_point"), arrowheads("arrow_open")], [2, 1, 0]);
    }
}
