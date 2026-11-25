//! Attribute blocks, `A@{ shape: hex, label: "Text" }`: mmdr 0.3.1 reads them as a node named `A@`.

use std::collections::HashMap;

use vt_canvas::Shape;

use super::shape;
use super::source;

/// Keys only a node's block has; a block without any belongs to an edge or a group, which mmdr cannot use.
const NODE_KEYS: [&str; 4] = ["shape", "label", "icon", "img"];

/// The source with every attribute block in the syntax mmdr reads, and the shape each node's block names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rewritten {
    pub(super) source: String,
    pub(super) shapes: HashMap<String, Shape>,
}

/// A block and the node before it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block<'a> {
    /// Where the node's id starts.
    start: usize,
    /// Where the block's `@` is.
    at: usize,
    /// After the block's `}`.
    end: usize,
    id: &'a str,
    /// The node has a label in brackets before the block.
    labelled: bool,
    attributes: HashMap<&'a str, String>,
}

/// A node's block becomes its label in brackets; any other block is left out with the id before it.
pub(super) fn rewrite(source: &str) -> Rewritten {
    let (front_matter, body) = source::front_matter(source);
    let mut text = front_matter.to_owned();
    let mut shapes = HashMap::new();
    let mut copied = 0;
    for block in blocks(body) {
        if block.start < copied {
            continue;
        }
        if NODE_KEYS.iter().any(|key| block.attributes.contains_key(key)) {
            text.push_str(&body[copied..block.at]);
            text.push_str(&new_label(&block));
        } else {
            text.push_str(&body[copied..block.start]);
        }
        if let Some(name) = block.attributes.get("shape") {
            shapes.insert(block.id.to_owned(), shape::named(Some(name)));
        }
        copied = block.end;
    }
    text.push_str(&body[copied..]);
    Rewritten { source: text, shapes }
}

/// The block's label in brackets, on one line as mmdr reads a statement, unless the node has one already.
fn new_label(block: &Block<'_>) -> String {
    match block.attributes.get("label") {
        Some(label) if !block.labelled => {
            let lines: Vec<&str> = label.lines().map(str::trim).collect();
            format!("[\"{}\"]", lines.join("<br>").replace('"', "#quot;"))
        }
        _ => String::new(),
    }
}

/// Every `@{ … }` block after a node id, in the order they appear.
fn blocks(body: &str) -> Vec<Block<'_>> {
    source::syntax(body)
        .filter(|&(at, char)| char == '@' && body[at..].starts_with("@{"))
        .filter_map(|(at, _)| {
            let end = closing(body, at + 1)?;
            let before = &body[..at];
            let label_start = if before.ends_with([']', ')', '}']) { opening(before) } else { None };
            let id_end = label_start.unwrap_or(at);
            let start = body[..id_end].rfind(|char: char| !(char.is_alphanumeric() || char == '_')).map_or(0, |index| index + 1);
            let id = &body[start..id_end];
            let attributes = attributes(&body[at + 2..end - 1]);
            (!id.is_empty()).then_some(Block { start, at, end, id, labelled: label_start.is_some(), attributes })
        })
        .collect()
}

/// Where the bracketed label `text` ends with starts.
fn opening(text: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, char) in text.char_indices().rev() {
        match char {
            ']' | ')' | '}' => depth += 1,
            '[' | '(' | '{' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The index after the `}` closing the `{` at `open`, skipping quoted text and the escaped quotes inside it.
fn closing(body: &str, open: usize) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, char) in body[open + 1..].char_indices() {
        match char {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '}' if !quoted => return Some(open + 1 + index + 1),
            _ => {}
        }
    }
    None
}

/// `key: value` pairs split at commas and line ends, quotes around a value left out.
fn attributes(inside: &str) -> HashMap<&str, String> {
    entries(inside)
        .into_iter()
        .filter_map(|entry| entry.split_once(':'))
        .map(|(key, value)| (key.trim(), unquoted(value.trim())))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

fn entries(inside: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let (mut start, mut quote, mut escaped) = (0, None, false);
    for (index, char) in inside.char_indices() {
        match char {
            _ if escaped => escaped = false,
            '\\' if quote.is_some() => escaped = true,
            '"' | '\'' if quote.is_none() => quote = Some(char),
            _ if quote == Some(char) => quote = None,
            ',' | '\n' if quote.is_none() => {
                entries.push(&inside[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    entries.push(&inside[start..]);
    entries
}

fn unquoted(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|rest| rest.strip_suffix(quote)) {
            return inner.replace(&format!("\\{quote}"), &quote.to_string());
        }
    }
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewritten(body: &str) -> String {
        rewrite(body).source
    }

    #[test]
    fn a_node_block_becomes_its_label_and_names_its_shape() {
        let rewritten = rewrite("flowchart LR\n  A@{ shape: hex, label: \"Six, sides\" } --> B");

        assert_eq!(rewritten.source, "flowchart LR\n  A[\"Six, sides\"] --> B");
        assert_eq!(rewritten.shapes, HashMap::from([("A".to_owned(), Shape::Hexagon)]));
    }

    #[test]
    fn a_block_without_a_label_leaves_the_bare_id() {
        assert_eq!(rewritten("flowchart LR\n  A@{shape: diam}\n  A --> B"), "flowchart LR\n  A\n  A --> B");
    }

    #[test]
    fn a_block_after_a_bracketed_label_keeps_that_label() {
        let rewritten = rewrite("flowchart LR\n  C[Hello]@{\n    shape: circle\n    other: \"clock\"\n  }");

        assert_eq!(rewritten.source, "flowchart LR\n  C[Hello]");
        assert_eq!(rewritten.shapes["C"], Shape::Circle);
    }

    #[test]
    fn an_edge_or_group_block_is_left_out_with_its_id() {
        assert_eq!(
            rewritten("flowchart LR\n  A e1@--> B\n  e1@{ curve: linear }\n  one@{ view: collapsed }"),
            "flowchart LR\n  A e1@--> B\n  \n  "
        );
    }

    #[test]
    fn a_block_right_after_another_is_left_as_written() {
        assert_eq!(rewritten("flowchart LR\n  A@{ label: x }@{ curve: y }"), "flowchart LR\n  A[\"x\"]@{ curve: y }");
    }

    #[test]
    fn quotes_escaped_inside_a_label_are_kept() {
        let source = r#"flowchart LR
  A@{ label: "<b style=\"color:red\">bold</b>" }"#;

        assert_eq!(rewritten(source), "flowchart LR\n  A[\"<b style=#quot;color:red#quot;>bold</b>\"]");
    }

    #[test]
    fn a_label_over_several_lines_is_written_on_one() {
        let source = "flowchart LR\n  A@{\n    label: \"one\n      two\"\n  }";

        assert_eq!(rewritten(source), "flowchart LR\n  A[\"one<br>two\"]");
    }

    #[test]
    fn an_unknown_shape_is_a_rectangle_and_an_alias_maps_like_its_name() {
        let rewritten = rewrite("flowchart LR\n  A@{ shape: cloud }\n  B@{ shape: trap-t }\n  C@{ shape: database }");

        assert_eq!(
            [&rewritten.shapes["A"], &rewritten.shapes["B"], &rewritten.shapes["C"]],
            [&Shape::Rectangle, &Shape::InvertedTrapezoid, &Shape::Cylinder]
        );
    }

    #[test]
    fn a_block_inside_a_label_or_the_front_matter_is_text() {
        let source = "---\ntitle: A@{ shape: hex }\n---\nflowchart LR\n  A[\"B@{ shape: hex }\"]";

        assert_eq!(rewrite(source), Rewritten { source: source.to_owned(), shapes: HashMap::new() });
    }
}
