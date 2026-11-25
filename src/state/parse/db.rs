//! The document mermaid's state grammar builds: statements, each composite state's body a document of its own.

use super::lexer::Direction;

/// A statement of a document, as the grammar's actions build it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Statement {
    State(State),
    Relation {
        from: State,
        to: State,
        description: String,
    },
    Direction(Direction),
    /// A statement the drawing never reads (a class, a style, a click, a floating note…), kept because it still
    /// makes a concurrent region of its own.
    Other,
}

/// A state statement: a state named, described, given a note, or opening a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct State {
    pub(super) id: String,
    /// Mermaid's `type`; a note statement has none.
    pub(super) kind: Option<Kind>,
    pub(super) description: Description,
    pub(super) doc: Option<Vec<Statement>>,
    pub(super) note: Option<Note>,
    /// Whether a `[*]` is the start (`true`) or the end of its document.
    pub(super) start: Option<bool>,
}

impl State {
    pub(super) fn named(id: String) -> Self {
        Self { id, kind: Some(Kind::Default), description: Description::None, doc: None, note: None, start: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Default,
    Fork,
    Join,
    Choice,
    Divider,
}

/// A state's description: none, one text, or several, as `state "a" as b:c` gives two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Description {
    None,
    Text(String),
    Many(Vec<String>),
}

impl Description {
    /// An empty text describes nothing, as JavaScript reads `''` as false.
    pub(super) fn text(text: String) -> Self {
        if text.is_empty() { Self::None } else { Self::Text(text) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Note {
    pub(super) side: Side,
    pub(super) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::state) enum Side {
    Left,
    Right,
}
