//! What mermaid's flowchart parser adds to the preparation every diagram kind shares.

use crate::source::{self, is_space, last_line_break};

/// The text the flowchart lexer reads and what the configuration changes in its meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Prepared {
    pub(super) text: String,
    pub(super) config: Config,
}

/// The settings of a front matter or a directive that change what a flowchart means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Config {
    /// `inheritDir`: a subgraph without its own direction takes the flowchart's.
    pub(super) inherits_direction: bool,
}

/// The shared preparation, then the spaces after a closing brace cut to one line end.
pub(super) fn prepare(source: &str) -> Prepared {
    let prepared = source::prepare(source);
    Prepared { text: with_braces_ending_lines(&prepared.text), config: Config { inherits_direction: inherits_direction(&prepared.config) } }
}

/// `inheritDir: true` in a front matter's YAML or a directive's JSON, the one setting that changes a flowchart's meaning.
fn inherits_direction(config: &str) -> bool {
    config.match_indices("inheritDir").any(|(at, word)| {
        let after = config[at + word.len()..].strip_prefix(['"', '\'']).unwrap_or(&config[at + word.len()..]);
        let value = after.trim_start_matches(is_space).strip_prefix(':').map(|value| value.trim_start_matches(is_space));
        value.is_some_and(|value| value.starts_with("true"))
    })
}

/// Mermaid's flowchart parser cuts the spaces after a `}` up to the last line end among them to that line end.
fn with_braces_ending_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(brace) = rest.find('}') {
        out.push_str(&rest[..=brace]);
        rest = &rest[brace + 1..];
        if let Some(line_end) = last_line_break(rest, 0) {
            out.push('\n');
            rest = &rest[line_end + 1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherit_dir_is_read_from_the_front_matter_or_a_directive() {
        let inherits = |source: &str| prepare(source).config.inherits_direction;

        assert!(inherits("---\nconfig:\n  flowchart:\n    inheritDir: true\n---\ngraph TD"));
        assert!(inherits("%%{init: {\"flowchart\": { \"inheritDir\": true }}}%%\ngraph TD"));
        assert!(!inherits("graph TD\n  A[inheritDir: true]"));
    }

    #[test]
    fn spaces_after_a_brace_end_at_their_last_line_end() {
        assert_eq!(with_braces_ending_lines("}  \n \n  A} B\n"), "}\n  A} B\n");
    }
}
