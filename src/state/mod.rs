//! State diagrams.

mod parse;

use vt_canvas::Canvas;

use crate::error::Error;

/// Reads `source` as a state diagram; one mermaid reads is not drawn yet.
pub(crate) fn render(source: &str) -> Result<Canvas, Error> {
    parse::parse(source, parse::Limits::DRAWN)?;
    Err(Error::Unsupported)
}
