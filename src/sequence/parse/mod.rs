//! Sequence diagrams read as mermaid 12.3.0 reads them: its lexer, its grammar and its diagram db, rewritten by hand,
//! so a sequence diagram means here what it means to mermaid.

#[cfg(test)]
mod agreement;
mod arrow;
mod colour;
mod db;
mod grammar;
mod lexer;

use std::fmt;

pub(super) use arrow::{Arrow, Tip};
pub(super) use db::{Item, Placement, Sequence};
pub(super) use lexer::Block;

use crate::error::{Error, MAX_MESSAGES, MAX_PARTICIPANTS, MAX_SOURCE_BYTES};
use crate::source;

/// Longest source, most participants and most items read before the diagram is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Limits {
    pub(super) bytes: usize,
    pub(super) participants: usize,
    /// Entries of mermaid's `messages`: messages, notes, activations, block starts, sections and ends.
    pub(super) items: usize,
}

impl Limits {
    /// What `render` draws.
    pub(super) const DRAWN: Self = Self { bytes: MAX_SOURCE_BYTES, participants: MAX_PARTICIPANTS, items: MAX_MESSAGES };
}

/// Why mermaid would refuse a sequence diagram; lines are counted as mermaid counts them, once the front matter,
/// directives and comment lines are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Invalid {
    /// The source does not open with `sequenceDiagram`.
    NoDiagram,
    /// No rule of the lexer reads the text at that line.
    Unrecognized { line: usize },
    /// The grammar allows no such token there.
    Unexpected { line: usize, found: String },
    /// A participant's `@{ … }` block is not the YAML mermaid reads.
    Config { id: String },
    /// A participant is put in a second box.
    TwoBoxes { id: String },
    /// `create` names a participant that is already there.
    Recreated { id: String },
    /// The message after `create` is not sent to the participant created.
    NotCreated { id: String },
    /// The message after `destroy` neither comes from nor goes to the participant destroyed.
    NotDestroyed { id: String },
    /// `deactivate` or `-` on a participant that is not active.
    Inactive { id: String },
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiagram => write!(f, "no diagram type matches the text"),
            Self::Unrecognized { line } => write!(f, "unrecognized text on line {line}"),
            Self::Unexpected { line, found } => write!(f, "unexpected {found} on line {line}"),
            Self::Config { id } => write!(f, "the @{{ }} block of {id} is not valid YAML"),
            Self::TwoBoxes { id } => write!(f, "{id} is in two boxes"),
            Self::Recreated { id } => write!(f, "{id} is created but already there"),
            Self::NotCreated { id } => write!(f, "no message is sent to {id} right after it is created"),
            Self::NotDestroyed { id } => write!(f, "no message comes from or goes to {id} right after it is destroyed"),
            Self::Inactive { id } => write!(f, "{id} is deactivated but not active"),
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid { reason: invalid.to_string() }
    }
}

/// The sequence diagram in `source`, refused as mermaid refuses it or once it holds more than `limits` allow.
pub(super) fn parse(source: &str, limits: Limits) -> Result<Sequence, Error> {
    if source.len() > limits.bytes {
        return Err(Error::TooLong { bytes: source.len() });
    }
    read(&source::prepare(source), limits)
}

/// The sequence diagram in `prepared`, refused as mermaid refuses it or once it holds more than `limits` allow.
pub(super) fn read(prepared: &source::Prepared, limits: Limits) -> Result<Sequence, Error> {
    let text = &prepared.text;
    if !source::opens_with(text, "sequenceDiagram") {
        return Err(Invalid::NoDiagram.into());
    }
    let tokens = lexer::tokens(text)?;
    let statements = grammar::parse(text, &tokens)?;
    db::apply(statements, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn participants(count: usize) -> String {
        format!("sequenceDiagram\n{}", (0..count).map(|index| format!("  participant p{index}\n")).collect::<String>())
    }

    fn messages(count: usize) -> String {
        format!("sequenceDiagram\n{}", "  A->>B: hi\n".repeat(count))
    }

    #[test]
    fn a_diagram_at_the_participant_limit_is_read() {
        assert_eq!(parse(&participants(MAX_PARTICIPANTS), Limits::DRAWN).map(|sequence| sequence.participants.len()), Ok(MAX_PARTICIPANTS));
    }

    #[test]
    fn a_diagram_over_the_participant_limit_is_refused() {
        let refused = parse(&participants(MAX_PARTICIPANTS + 1), Limits::DRAWN);

        assert_eq!(refused, Err(Error::TooManyParticipants { participants: MAX_PARTICIPANTS + 1 }));
    }

    #[test]
    fn a_diagram_at_the_message_limit_is_read() {
        assert_eq!(parse(&messages(MAX_MESSAGES), Limits::DRAWN).map(|sequence| sequence.items.len()), Ok(MAX_MESSAGES));
    }

    #[test]
    fn notes_and_block_ends_count_towards_the_message_limit() {
        let source = format!("{}loop\nend\n", messages(MAX_MESSAGES - 1));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooManyMessages { messages: MAX_MESSAGES + 1 }));
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused_before_it_is_read() {
        let source = format!("sequenceDiagram\n{}", " ".repeat(MAX_SOURCE_BYTES));

        assert_eq!(parse(&source, Limits::DRAWN), Err(Error::TooLong { bytes: source.len() }));
    }

    #[test]
    fn a_source_without_a_sequence_header_is_invalid() {
        assert!(matches!(parse("flowchart LR\n  A --> B", Limits::DRAWN), Err(Error::Invalid { .. })));
    }
}
