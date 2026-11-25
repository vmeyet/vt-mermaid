//! Sequence diagrams: participants side by side, what happens between them from top to bottom.

#[cfg_attr(not(test), expect(dead_code, reason = "laid out by a later change"))]
mod diagram;
#[expect(dead_code, reason = "laid out and drawn by later changes")]
mod layout;
#[cfg_attr(not(test), expect(dead_code, reason = "read by the drawing, which lands next"))]
mod parse;

/// How much a sequence diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Participants.
    pub participants: usize,
    /// Messages, notes, activations and the starts, sections and ends of blocks.
    pub messages: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn read(source: &str) -> Result<Counts, crate::Error> {
    let sequence = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { participants: sequence.participants.len(), messages: sequence.items.len() })
}
