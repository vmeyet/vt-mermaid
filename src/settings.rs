//! How a diagram is drawn, beyond its source.

use crate::error::MAX_WIDTH;

/// What becomes of a word longer than a line of its label.
#[non_exhaustive]
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
    /// The most columns the drawing may take; over [`MAX_WIDTH`], [`MAX_WIDTH`].
    pub max_width: u16,
    /// What becomes of a word longer than a line of a node's label.
    pub long_words: LongWords,
}

impl Settings {
    /// A drawing at most `max_width` columns wide, and never more than [`MAX_WIDTH`], long words broken.
    pub const fn new(max_width: u16) -> Self {
        Self { max_width, long_words: LongWords::Break }.clamped()
    }

    /// The same with long words drawn as `long_words` says.
    pub const fn long_words(self, long_words: LongWords) -> Self {
        Self { long_words, ..self }
    }

    /// The same with `max_width` brought down to [`MAX_WIDTH`], as `max_width` is a public field.
    pub(crate) const fn clamped(self) -> Self {
        Self { max_width: if self.max_width > MAX_WIDTH { MAX_WIDTH } else { self.max_width }, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_width_up_to_the_canvas_limit_is_kept_and_one_over_it_brought_down_to_it() {
        assert_eq!(Settings::new(MAX_WIDTH).max_width, MAX_WIDTH);
        assert_eq!(Settings::new(MAX_WIDTH + 1).max_width, MAX_WIDTH);
        assert_eq!(Settings::new(u16::MAX).max_width, MAX_WIDTH);
    }
}
