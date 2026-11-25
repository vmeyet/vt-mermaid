//! A node's or an edge's `@{ … }` block: the YAML mermaid reads it as, flat `key: value` pairs, inline or one per line.

use std::mem;

use super::{Invalid, is_space};

/// Every shape name mermaid 12.3.0 accepts in a block: lowercase, without `_`.
pub(super) const SHAPES: [&str; 156] = [
    "doublecircle",
    "state",
    "choice",
    "note",
    "composite",
    "anchor",
    "icon",
    "bpmn-start",
    "bpmn-intermediate",
    "bpmn-boundary",
    "bpmn-end",
    "bpmn-gateway",
    "bpmn-activity",
    "bpmn-data",
    "bpmn-data-store",
    "bpmn-annotation",
    "rect",
    "proc",
    "process",
    "rectangle",
    "rounded",
    "event",
    "stadium",
    "terminal",
    "pill",
    "fr-rect",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "subroutine",
    "cyl",
    "db",
    "database",
    "cylinder",
    "datastore",
    "data-store",
    "folder",
    "directory",
    "bucket",
    "console",
    "browser",
    "person",
    "circle",
    "circ",
    "bang",
    "cloud",
    "diam",
    "decision",
    "diamond",
    "question",
    "hex",
    "hexagon",
    "prepare",
    "lean-r",
    "lean-right",
    "in-out",
    "lean-l",
    "lean-left",
    "out-in",
    "trap-b",
    "priority",
    "trapezoid-bottom",
    "trapezoid",
    "trap-t",
    "manual",
    "trapezoid-top",
    "inv-trapezoid",
    "dbl-circ",
    "double-circle",
    "text",
    "notch-rect",
    "card",
    "notched-rectangle",
    "lin-rect",
    "lined-rectangle",
    "lined-process",
    "lin-proc",
    "shaded-process",
    "sm-circ",
    "start",
    "small-circle",
    "fr-circ",
    "stop",
    "framed-circle",
    "fork",
    "join",
    "hourglass",
    "collate",
    "brace",
    "comment",
    "brace-l",
    "brace-r",
    "braces",
    "bolt",
    "com-link",
    "lightning-bolt",
    "doc",
    "document",
    "delay",
    "half-rounded-rectangle",
    "h-cyl",
    "das",
    "horizontal-cylinder",
    "lin-cyl",
    "disk",
    "lined-cylinder",
    "curv-trap",
    "curved-trapezoid",
    "display",
    "div-rect",
    "div-proc",
    "divided-rectangle",
    "divided-process",
    "tri",
    "extract",
    "triangle",
    "win-pane",
    "internal-storage",
    "window-pane",
    "f-circ",
    "junction",
    "filled-circle",
    "notch-pent",
    "loop-limit",
    "notched-pentagon",
    "flip-tri",
    "manual-file",
    "flipped-triangle",
    "sl-rect",
    "manual-input",
    "sloped-rectangle",
    "docs",
    "documents",
    "st-doc",
    "stacked-document",
    "st-rect",
    "procs",
    "processes",
    "stacked-rectangle",
    "bow-rect",
    "stored-data",
    "bow-tie-rectangle",
    "cross-circ",
    "summary",
    "crossed-circle",
    "tag-doc",
    "tagged-document",
    "tag-rect",
    "tagged-rectangle",
    "tag-proc",
    "tagged-process",
    "flag",
    "paper-tape",
    "odd",
    "lin-doc",
    "lined-document",
];

/// A value as YAML's JSON schema resolves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    Text(String),
    Number(String),
    Bool(bool),
    Null,
    /// A nested mapping or sequence, which no key mermaid reads for a flowchart holds.
    Nested,
}

impl Value {
    /// JavaScript's truthiness, which decides whether mermaid uses a key.
    pub(super) fn is_truthy(&self) -> bool {
        match self {
            Self::Text(text) => !text.is_empty(),
            Self::Number(number) => number.parse::<f64>().is_ok_and(|number| number != 0.0),
            Self::Bool(value) => *value,
            Self::Null => false,
            Self::Nested => true,
        }
    }

    /// The value as a label shows it.
    pub(super) fn text(&self) -> Option<String> {
        match self {
            Self::Text(text) | Self::Number(text) => Some(text.clone()),
            Self::Bool(value) => Some(value.to_string()),
            Self::Null | Self::Nested => None,
        }
    }
}

/// The pairs of a block, as mermaid hands it to YAML: one line is a flow mapping, several a block mapping.
pub(super) fn read(block: &str) -> Result<Vec<(String, Value)>, Invalid> {
    let pairs = if block.contains('\n') { block_pairs(block)? } else { flow_pairs(block)? };
    let duplicate = pairs.iter().enumerate().any(|(index, (key, _))| pairs[..index].iter().any(|(earlier, _)| earlier == key));
    if duplicate { Err(Invalid::ShapeData) } else { Ok(pairs) }
}

/// `{ key: value, key: "value" }`.
fn flow_pairs(block: &str) -> Result<Vec<(String, Value)>, Invalid> {
    let mut pairs = Vec::new();
    let mut rest = block.trim_matches(is_space);
    while !rest.is_empty() {
        let (key, after_key) = scalar(rest, true)?;
        let after_colon = after_key.trim_start_matches(is_space).strip_prefix(':');
        let (value, after_value) = match after_colon {
            Some(after) => value(after.trim_start_matches(is_space), true)?,
            None => (Value::Null, after_key),
        };
        pairs.push((key.text().unwrap_or_default(), value));
        let after_value = after_value.trim_start_matches(is_space);
        rest = match after_value.strip_prefix(',') {
            Some(after) => after.trim_start_matches(is_space),
            None if after_value.is_empty() => after_value,
            None => return Err(Invalid::ShapeData),
        };
    }
    Ok(pairs)
}

/// One `key: value` per line, a more indented line without a key going on with the value before.
fn block_pairs(block: &str) -> Result<Vec<(String, Value)>, Invalid> {
    let mut pairs: Vec<(String, Value)> = Vec::new();
    let mut indent = None;
    let mut lines = block.lines().peekable();
    while let Some(line) = lines.next() {
        let content = without_comment(line).trim_end_matches(is_space);
        let depth = content.len() - content.trim_start_matches(is_space).len();
        let content = &content[depth..];
        if content.is_empty() {
            continue;
        }
        let deeper = indent.is_some_and(|indent| depth > indent);
        if let Some((_, Value::Text(scalar))) =
            pairs.last_mut().filter(|(_, value)| deeper && matches!(value, Value::Text(text) if is_block_scalar(text)))
        {
            let indicator = mem::take(scalar);
            let mut body = vec![line];
            while let Some(next) = lines.next_if(|next| next.trim_matches(is_space).is_empty() || depth_of(next) > indent.unwrap_or(0)) {
                body.push(next);
            }
            *scalar = block_scalar(&indicator, &body);
            continue;
        }
        if let (true, Some((_, Value::Text(text)))) = (deeper, pairs.last_mut()) {
            text.push(' ');
            text.push_str(content);
            continue;
        }
        if indent.is_some_and(|indent| depth != indent) {
            return Err(Invalid::ShapeData);
        }
        indent = Some(depth);
        let (key, after_key) = scalar(content, false)?;
        let after_colon = after_key.trim_start_matches(is_space).strip_prefix(':').ok_or(Invalid::ShapeData)?;
        let (value, rest) = value(after_colon.trim_start_matches(is_space), false)?;
        if !rest.trim_matches(is_space).is_empty() {
            return Err(Invalid::ShapeData);
        }
        pairs.push((key.text().unwrap_or_default(), value));
    }
    Ok(pairs)
}

/// `|` or `>`, with a chomping indicator: the lines after the key are the value.
fn is_block_scalar(text: &str) -> bool {
    matches!(text, "|" | "|-" | "|+" | ">" | ">-" | ">+")
}

fn depth_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(is_space).len()
}

/// A literal (`|`) block keeps its line ends, a folded one (`>`) joins its lines with spaces; both end with one line
/// end, none with `-`, all of them with `+`.
fn block_scalar(indicator: &str, lines: &[&str]) -> String {
    let indent = lines.iter().filter(|line| !line.trim_matches(is_space).is_empty()).map(|line| depth_of(line)).min().unwrap_or(0);
    let lines: Vec<&str> = lines.iter().map(|line| line.get(indent..).unwrap_or("")).collect();
    let content_end = lines.iter().rposition(|line| !line.is_empty()).map_or(0, |last| last + 1);
    let separator = if indicator.starts_with('|') { "\n" } else { " " };
    let joined = lines[..content_end].join(separator);
    match indicator.chars().nth(1) {
        Some('-') => joined,
        Some('+') => format!("{joined}{}", "\n".repeat(lines.len() - content_end + 1)),
        _ => format!("{joined}\n"),
    }
}

/// The line before a `#` that starts a comment: at its start or after a space, outside quotes.
fn without_comment(line: &str) -> &str {
    let mut quote = None;
    let mut previous = ' ';
    for (at, char) in line.char_indices() {
        match (quote, char) {
            (None, '"' | '\'') => quote = Some(char),
            (Some(open), _) if char == open => quote = None,
            (None, '#') if is_space(previous) => return &line[..at],
            _ => {}
        }
        previous = char;
    }
    line
}

fn value(text: &str, flow: bool) -> Result<(Value, &str), Invalid> {
    if let Some(open) = text.chars().next().filter(|char| matches!(char, '{' | '[')) {
        return nested(text, open).map(|rest| (Value::Nested, rest));
    }
    scalar(text, flow)
}

/// A quoted or plain scalar and the text after it; a plain one ends at `: `, and in a flow mapping at `,` or `}`.
fn scalar(text: &str, flow: bool) -> Result<(Value, &str), Invalid> {
    match text.chars().next() {
        Some('"') => double_quoted(&text[1..]).map(|(inside, rest)| (Value::Text(inside), rest)),
        Some('\'') => single_quoted(&text[1..]).map(|(inside, rest)| (Value::Text(inside), rest)),
        _ => {
            let end = plain_end(text, flow);
            Ok((resolved(text[..end].trim_end_matches(is_space)), &text[end..]))
        }
    }
}

fn plain_end(text: &str, flow: bool) -> usize {
    let mut chars = text.char_indices().peekable();
    while let Some((at, char)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        let ends_key = char == ':' && next.is_none_or(|next| is_space(next) || (flow && matches!(next, ',' | '}' | ']')));
        if ends_key || (flow && matches!(char, ',' | '{' | '}' | '[' | ']')) {
            return at;
        }
    }
    text.len()
}

fn resolved(plain: &str) -> Value {
    match plain {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ if is_number(plain) => Value::Number(plain.to_owned()),
        _ => Value::Text(plain.to_owned()),
    }
}

/// A JSON number: `-?(0|[1-9][0-9]*)(\.[0-9]*)?([eE][-+]?[0-9]+)?`.
fn is_number(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let integer = unsigned.find(|char: char| !char.is_ascii_digit()).unwrap_or(unsigned.len());
    let (digits, rest) = unsigned.split_at(integer);
    let fraction = rest.strip_prefix('.').map_or(rest, |after| after.trim_start_matches(|char: char| char.is_ascii_digit()));
    let exponent = fraction.strip_prefix(['e', 'E']).map(|after| after.strip_prefix(['+', '-']).unwrap_or(after));
    let exponent_ok = exponent.is_none_or(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()));
    let ends = exponent.is_some() || fraction.is_empty();
    !digits.is_empty() && (digits == "0" || !digits.starts_with('0')) && ends && exponent_ok
}

fn double_quoted(text: &str) -> Result<(String, &str), Invalid> {
    let mut inside = String::new();
    let mut chars = text.char_indices();
    while let Some((at, char)) = chars.next() {
        match char {
            '"' => return Ok((inside, &text[at + 1..])),
            '\\' => {
                let (_, escaped) = chars.next().ok_or(Invalid::ShapeData)?;
                inside.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => '\0',
                    '"' | '\\' | '/' | ' ' => escaped,
                    _ => return Err(Invalid::ShapeData),
                });
            }
            _ => inside.push(char),
        }
    }
    Err(Invalid::ShapeData)
}

fn single_quoted(text: &str) -> Result<(String, &str), Invalid> {
    let mut inside = String::new();
    let mut rest = text;
    loop {
        let quote = rest.find('\'').ok_or(Invalid::ShapeData)?;
        inside.push_str(&rest[..quote]);
        rest = &rest[quote + 1..];
        match rest.strip_prefix('\'') {
            Some(after) => {
                inside.push('\'');
                rest = after;
            }
            None => return Ok((inside, rest)),
        }
    }
}

/// The text after the mapping or sequence `text` opens with, skipping quoted text.
fn nested(text: &str, open: char) -> Result<&str, Invalid> {
    let close = if open == '{' { '}' } else { ']' };
    let mut depth = 0_usize;
    let mut quote = None;
    for (at, char) in text.char_indices() {
        match (quote, char) {
            (None, '"' | '\'') => quote = Some(char),
            (Some(open), _) if char == open => quote = None,
            (None, _) if char == open => depth += 1,
            (None, _) if char == close => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&text[at + 1..]);
                }
            }
            _ => {}
        }
    }
    Err(Invalid::ShapeData)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Text(value.to_owned())
    }

    #[test]
    fn an_inline_block_is_a_flow_mapping() {
        let pairs = read(r#" shape: rect, label: "Hello, \"you\"", w: 40 "#).unwrap();

        assert_eq!(
            pairs,
            [
                ("shape".to_owned(), text("rect")),
                ("label".to_owned(), text("Hello, \"you\"")),
                ("w".to_owned(), Value::Number("40".to_owned()))
            ]
        );
    }

    #[test]
    fn a_block_over_several_lines_is_a_block_mapping() {
        let pairs = read("\n    shape: circle\n    other: 'a ''b'''  # note\n    label: a\n      long one\n  ").unwrap();

        assert_eq!(
            pairs,
            [("shape".to_owned(), text("circle")), ("other".to_owned(), text("a 'b'")), ("label".to_owned(), text("a long one"))]
        );
    }

    #[test]
    fn a_block_scalar_takes_the_lines_after_its_key() {
        let pairs = read("\n  label: |\n    This is a\n    multiline string\n  shape: rect\n").unwrap();

        assert_eq!(pairs, [("label".to_owned(), text("This is a\nmultiline string\n")), ("shape".to_owned(), text("rect"))]);
        assert_eq!(read("\n  label: >-\n    one\n    two\n").unwrap(), [("label".to_owned(), text("one two"))]);
    }

    #[test]
    fn json_values_are_resolved() {
        let values: Vec<Value> = ["true", "null", "-1.5e3", "01", "x"].iter().map(|plain| resolved(plain)).collect();

        assert_eq!(values, [Value::Bool(true), Value::Null, Value::Number("-1.5e3".to_owned()), text("01"), text("x")]);
    }

    #[test]
    fn a_nested_value_is_skipped_whole() {
        assert_eq!(read("a: {b: '}'}, c: d").unwrap(), [("a".to_owned(), Value::Nested), ("c".to_owned(), text("d"))]);
    }

    #[test]
    fn a_value_counts_as_given_as_javascript_judges_it() {
        let truthy: Vec<bool> = [text(""), text("0"), Value::Number("0".to_owned()), Value::Bool(false), Value::Null, Value::Nested]
            .iter()
            .map(Value::is_truthy)
            .collect();

        assert_eq!(truthy, [false, true, false, false, false, true]);
        assert_eq!([Value::Number("42".to_owned()).text(), Value::Null.text()], [Some("42".to_owned()), None]);
    }

    #[test]
    fn every_shape_name_is_lowercase_without_underscores_as_mermaid_demands() {
        assert!(SHAPES.iter().all(|name| name.to_lowercase() == *name && !name.contains('_')));
    }

    #[test]
    fn broken_yaml_is_refused() {
        assert_eq!(read(" label: \"open "), Err(Invalid::ShapeData));
        assert_eq!(read(" a: 1, a: 2"), Err(Invalid::ShapeData));
        assert_eq!(read("\n  a: 1\n b: 2"), Err(Invalid::ShapeData));
    }
}
