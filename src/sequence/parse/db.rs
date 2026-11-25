//! Mermaid's sequence db (`sequenceDb.ts`): what each statement adds to the diagram, merges into it or refuses.

use super::arrow::Arrow;
use super::lexer::Block;
use crate::source::is_space;

/// A text as written after `:` or a keyword, and whether it asks to wrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::sequence) struct Text {
    pub(in crate::sequence) value: String,
    pub(in crate::sequence) wrap: Option<bool>,
}

impl Text {
    /// Mermaid's `parseMessage`: trimmed, a `wrap:` or `nowrap:` prefix taken off.
    pub(in crate::sequence) fn parsed(written: &str) -> Self {
        let trimmed = written.trim_matches(is_space);
        let unprefixed = trimmed.strip_prefix(':').unwrap_or(trimmed);
        let (wrap, value) = match (unprefixed.strip_prefix("wrap:"), unprefixed.strip_prefix("nowrap:")) {
            (Some(rest), _) => (Some(true), rest),
            (_, Some(rest)) => (Some(false), rest),
            _ => (None, trimmed),
        };
        Self { value: value.trim_matches(is_space).to_owned(), wrap }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::sequence) enum Placement {
    LeftOf,
    RightOf,
    Over,
}

/// What the grammar hands the db, as jison's actions build it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Statement {
    /// `participant`, `actor` or `create`, or a name a statement mentions, with no figure then.
    Participant {
        id: String,
        figure: Option<&'static str>,
        description: Option<Text>,
        config: Option<String>,
        created: bool,
    },
    Destroy {
        id: String,
    },
    GroupStart {
        text: String,
    },
    GroupEnd,
    Message {
        from: String,
        to: String,
        text: Text,
        arrow: Arrow,
        activate: bool,
    },
    Note {
        from: String,
        to: String,
        placement: Placement,
        text: Text,
    },
    Activation {
        id: String,
        start: bool,
    },
    Central {
        id: String,
        reverse: bool,
    },
    Autonumber {
        start: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
    Open {
        block: Block,
        text: Text,
    },
    Section {
        block: Block,
        text: Text,
    },
    Close {
        block: Block,
    },
}

impl Statement {
    /// A participant a message, a note or a link names.
    pub(super) fn named(id: String) -> Self {
        Self::Participant { id, figure: None, description: None, config: None, created: false }
    }
}
