//! What every diagram kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of
//! `@{ … }` blocks, and which kind a source is.

pub(crate) mod entity;
mod prepare;
pub(crate) mod unicode;
pub(crate) mod yaml;

pub(crate) use prepare::{is_word, last_line_break, prepare};

use prepare::without_any_comments;

use crate::error::{Error, MAX_SOURCE_BYTES};

/// The diagram kinds drawn as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Flowchart,
    Sequence,
    State,
    Class,
    Er,
}

/// Most characters of the header an [`Error::Unsupported`] quotes.
const HEADER_CHARS: usize = 32;

/// Headers of the diagram kinds mermaid knows but none drawn here, each a prefix of the word a source starts with.
const OTHER_KINDS: [&str; 33] = [
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
    "cynefin",
    "wardley",
    "agentflow",
    "eventmodeling",
    "treeView",
    "usecase",
    "railroad",
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
        } else if opens_with(&text, "stateDiagram") {
            Ok(Self::State)
        } else if opens_with(&text, "classDiagram") {
            Ok(Self::Class)
        } else if opens_with(&text, "erDiagram") {
            Ok(Self::Er)
        } else if OTHER_KINDS.iter().any(|kind| opens_with(&text, kind)) {
            Err(Error::Unsupported { kind: header(&text) })
        } else {
            Err(Error::Invalid { reason: "no diagram type matches the text".to_owned() })
        }
    }
}

/// Whether prepared `text` opens with `header` as mermaid's detectors see it: comments left out, then spaces.
pub(crate) fn opens_with(text: &str, header: &str) -> bool {
    without_any_comments(text).trim_start_matches(is_space).starts_with(header)
}

/// The word prepared `text` opens with, cut to [`HEADER_CHARS`] ASCII letters, digits and `-`, so an error may quote
/// it.
fn header(text: &str) -> String {
    let uncommented = without_any_comments(text);
    uncommented
        .trim_start_matches(is_space)
        .chars()
        .take_while(|char| char.is_ascii_alphanumeric() || *char == '-')
        .take(HEADER_CHARS)
        .collect()
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
    use std::fs;
    use std::path::Path;

    use serde_json::Value;

    use super::*;

    #[test]
    fn the_kind_is_read_after_the_front_matter_and_comments() {
        assert_eq!(Kind::of("---\ntitle: a\n---\n%% note\nsequenceDiagram\n  A->>B: hi"), Ok(Kind::Sequence));
        assert_eq!(Kind::of("  graph TD\n  A --> B"), Ok(Kind::Flowchart));
        assert_eq!(Kind::of("%%%%{init: {}}%%\nsequenceDiagram\n  A->>B: hi"), Ok(Kind::Sequence));
    }

    #[test]
    fn a_diagram_of_another_kind_is_unsupported_and_text_without_a_header_invalid() {
        assert_eq!(Kind::of("journey\n  title A day"), Err(Error::Unsupported { kind: "journey".to_owned() }));
        assert!(matches!(Kind::of("not a diagram"), Err(Error::Invalid { .. })));
    }

    #[test]
    fn an_unsupported_kind_quotes_its_header_cut_to_letters_digits_and_dashes() {
        let long = format!("xychart-beta{}\x1b[31m", "-".repeat(40));

        assert_eq!(Kind::of("%% c\n  pie title Pets"), Err(Error::Unsupported { kind: "pie".to_owned() }));
        assert_eq!(Kind::of(&long), Err(Error::Unsupported { kind: long[..HEADER_CHARS].to_owned() }));
    }

    /// Every kind mermaid accepts in the corpus but those drawn here is told apart from text that is no diagram; only a
    /// radar chart whose front matter closes less indented than it opens, which mermaid's radar parser reads past and
    /// the shared preparation does not, is left out.
    #[test]
    fn every_corpus_diagram_mermaid_accepts_of_a_kind_not_drawn_is_unsupported() {
        let drawn = ["flowchart-v2", "flowchart-elk", "sequence", "stateDiagram", "classDiagram", "er"];
        let unfenced = "radar/e2e--platform-dev-diagrams-knsv-knsv2-03-radar.mmd";
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let oracle: Value = serde_json::from_str(&fs::read_to_string(root.join("oracle/results/parse.json")).unwrap()).unwrap();

        let mut found = Vec::new();
        for (path, verdict) in oracle["cases"].as_object().unwrap() {
            let kind = verdict["kind"].as_str().unwrap_or_default();
            if verdict["accepted"] != true || drawn.contains(&kind) || path == unfenced {
                continue;
            }
            let source = fs::read_to_string(root.join("corpus").join(path)).unwrap();
            if !matches!(Kind::of(&source), Err(Error::Unsupported { .. })) {
                found.push(path.clone());
            }
        }

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("sequenceDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(Kind::of(&source), Err(Error::TooLong { bytes: source.len() }));
    }
}
