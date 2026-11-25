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

/// The words a label shows once its markup is gone: `<br>` and tags, entities, markdown emphasis and backticks.
fn words(label: &str) -> Vec<String> {
    let text = without_tags(&label.replace("<br>", " ").replace("<br/>", " ").replace("<br />", " "));
    let text = text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    text.replace(['`', '*'], "").split_whitespace().map(str::to_owned).collect()
}

fn without_tags(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut in_tag = false;
    for char in text.chars() {
        match char {
            '<' if !in_tag => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => kept.push(char),
            _ => {}
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_loses_its_markup_and_keeps_its_words() {
        assert_eq!(words("`The **cat**`<br>in &amp; <strong>out</strong>"), ["The", "cat", "in", "&", "out"]);
    }

    #[test]
    fn a_double_arrow_has_two_heads_and_an_open_line_none() {
        assert_eq!([arrowheads("double_arrow_point"), arrowheads("arrow_point"), arrowheads("arrow_open")], [2, 1, 0]);
    }
}
