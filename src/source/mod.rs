//! What every diagram kind shares before its own parser: mermaid's preparation of the text, its entities, the YAML of
//! `@{ … }` blocks, and the word a source opens with.

pub(crate) mod entity;
mod prepare;
pub(crate) mod unicode;
pub(crate) mod yaml;

pub(crate) use prepare::{Prepared, is_word, last_line_break, prepare};

use prepare::without_any_comments;

use crate::error::{Error, MAX_SOURCE_BYTES};

/// `source` prepared as mermaid prepares it for every kind; refused when it is too long to read.
pub(crate) fn prepared(source: &str) -> Result<Prepared, Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::TooLong { bytes: source.len() });
    }
    Ok(prepare(source))
}

/// Whether prepared `text` opens with `header` as mermaid's detectors see it: comments left out, then spaces.
pub(crate) fn opens_with(text: &str, header: &str) -> bool {
    without_any_comments(text).trim_start_matches(is_space).starts_with(header)
}

/// Most characters of the header an [`Error::Unsupported`] quotes.
const HEADER_CHARS: usize = 32;

/// The word prepared `text` opens with, cut to [`HEADER_CHARS`] ASCII letters, digits and `-`, so an error may quote
/// it.
pub(crate) fn header(text: &str) -> String {
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

/// Each file `list` names, or each file in the folder `list`, by its name, for the tests reading the repository's
/// diagrams.
#[cfg(test)]
pub(crate) fn listed(list: &str) -> Vec<(String, String)> {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let paths: Vec<String> = if root.join(list).is_dir() {
        fs::read_dir(root.join(list)).unwrap().map(|entry| format!("{list}/{}", entry.unwrap().file_name().to_string_lossy())).collect()
    } else {
        fs::read_to_string(root.join(list)).unwrap().lines().map(str::to_owned).collect()
    };
    let named = |path: String| {
        (Path::new(&path).file_stem().unwrap().to_string_lossy().into_owned(), fs::read_to_string(root.join(&path)).unwrap())
    };
    paths.into_iter().map(named).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_is_read_after_the_front_matter_and_comments_cut_to_letters_digits_and_dashes() {
        let long = format!("xychart-beta{}\x1b[31m", "-".repeat(40));

        assert_eq!(header(&prepare("---\ntitle: a\n---\n%% note\n  pie title Pets").text), "pie");
        assert_eq!(header(&long), long[..HEADER_CHARS]);
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_prepared() {
        let source = format!("sequenceDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(prepared(&source), Err(Error::TooLong { bytes: source.len() }));
    }
}
