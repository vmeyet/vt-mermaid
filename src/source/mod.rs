//! What every diagram kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of
//! `@{ … }` blocks, and which kind a source is.

pub(crate) mod entity;
mod prepare;
pub(crate) mod yaml;

pub(crate) use prepare::{last_line_break, prepare};

use prepare::without_any_comments;

use crate::error::{Error, MAX_SOURCE_BYTES};

/// The diagram kinds drawn as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Flowchart,
    Sequence,
}

/// Headers of the diagram kinds mermaid knows but none drawn here, each a prefix of the word a source starts with.
const OTHER_KINDS: [&str; 29] = [
    "classDiagram",
    "stateDiagram",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "quadrantChart",
    "requirementDiagram",
    "gitGraph",
    "C4Context",
    "C4Container",
    "C4Component",
    "C4Dynamic",
    "C4Deployment",
    "mindmap",
    "timeline",
    "zenuml",
    "sankey",
    "xychart",
    "block",
    "packet",
    "kanban",
    "architecture",
    "radar",
    "treemap",
    "info",
    "swimlane",
    "venn",
    "ishikawa",
];

impl Kind {
    /// The kind of diagram `source` holds, as mermaid detects it once the text is prepared; refused when the source is
    /// too long to read or holds no diagram drawn here.
    pub(crate) fn of(source: &str) -> Result<Self, Error> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(Error::TooLong { bytes: source.len() });
        }
        let text = prepare(source).text;
        if opens_with(&text, "graph") || opens_with(&text, "flowchart") {
            Ok(Self::Flowchart)
        } else if opens_with(&text, "sequenceDiagram") {
            Ok(Self::Sequence)
        } else if OTHER_KINDS.iter().any(|kind| opens_with(&text, kind)) {
            Err(Error::Unsupported)
        } else {
            Err(Error::Invalid { reason: "no diagram type matches the text".to_owned() })
        }
    }
}

/// Whether prepared `text` opens with `header` as mermaid's detectors see it: comments left out, then spaces.
pub(crate) fn opens_with(text: &str, header: &str) -> bool {
    without_any_comments(text).trim_start_matches(is_space).starts_with(header)
}

/// A character of a JavaScript regex `\s`, the space mermaid's patterns mean.
pub(crate) fn is_space(char: char) -> bool {
    matches!(
        char,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// A label as mermaid's DOMPurify leaves it, for comparing with the oracle: unknown tags dropped, entities decoded.
#[cfg(test)]
pub(crate) fn sanitized(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_is_read_after_the_front_matter_and_comments() {
        assert_eq!(Kind::of("---\ntitle: a\n---\n%% note\nsequenceDiagram\n  A->>B: hi"), Ok(Kind::Sequence));
        assert_eq!(Kind::of("  graph TD\n  A --> B"), Ok(Kind::Flowchart));
        assert_eq!(Kind::of("%%%%{init: {}}%%\nsequenceDiagram\n  A->>B: hi"), Ok(Kind::Sequence));
    }

    #[test]
    fn a_diagram_of_another_kind_is_unsupported_and_text_without_a_header_invalid() {
        assert_eq!(Kind::of("classDiagram\n  A <|-- B"), Err(Error::Unsupported));
        assert!(matches!(Kind::of("not a diagram"), Err(Error::Invalid { .. })));
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("sequenceDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(Kind::of(&source), Err(Error::TooLong { bytes: source.len() }));
    }
}
