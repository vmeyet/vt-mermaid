//! How a diagram is drawn, beyond its source.

/// What becomes of a word longer than a line of its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LongWords {
    /// The word goes on as many lines as it takes, so all of it is drawn.
    #[default]
    Break,
    /// The word keeps one line and is cut where the line ends, so the node stays small.
    Cut,
}

/// How [`render_with`](crate::render_with) draws a diagram.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The most columns the drawing may take.
    pub max_width: u16,
    /// What becomes of a word longer than a line of a node's label.
    pub long_words: LongWords,
}

impl Settings {
    /// A drawing at most `max_width` columns wide, long words broken.
    pub const fn new(max_width: u16) -> Self {
        Self { max_width, long_words: LongWords::Break }
    }

    /// The same with long words drawn as `long_words` says.
    pub const fn long_words(self, long_words: LongWords) -> Self {
        Self { long_words, ..self }
    }
}
