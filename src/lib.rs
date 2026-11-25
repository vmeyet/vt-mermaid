//! Mermaid diagrams drawn as text on a vt-canvas grid.
//!
//! ```
//! let canvas = vt_mermaid::render("flowchart LR\n  A --> B", 80)?;
//! let rows: Vec<String> =
//!     canvas.rows(vt_mermaid::GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect();
//! assert_eq!(rows.len(), 3);
//! # Ok::<(), vt_mermaid::Error>(())
//! ```

mod class;
mod error;
mod flowchart;
mod graph;
mod label;
mod sequence;
mod settings;
mod source;
mod state;

pub use error::{Error, MAX_AREA, MAX_EDGES, MAX_MESSAGES, MAX_NODES, MAX_PARTICIPANTS, MAX_SOURCE_BYTES};
pub use settings::{LongWords, Settings};
pub use vt_canvas::{Canvas, GlyphSet, Role, Run};

/// Exact measures of a drawing, for the scoreboard: not part of the API, behind the `measure` feature.
#[cfg(feature = "measure")]
#[doc(hidden)]
pub use graph::measure::Measures;

/// Reads a flowchart as `render` does, without laying it out, for the fuzzer: not part of the API, behind the `fuzz`
/// feature.
#[cfg(feature = "fuzz")]
#[doc(hidden)]
pub use flowchart::{Counts, counts as read_flowchart};

/// Reads a sequence diagram as `render` does, without laying it out, for the fuzzer: not part of the API, behind the
/// `fuzz` feature.
#[cfg(feature = "fuzz")]
#[doc(hidden)]
pub use sequence::{Counts as SequenceCounts, read as read_sequence};

/// Reads a class diagram as `render` does, without laying it out, for the fuzzer: not part of the API, behind the
/// `fuzz` feature.
#[cfg(feature = "fuzz")]
#[doc(hidden)]
pub use class::{Counts as ClassCounts, read as read_class};

/// Reads a state diagram as `render` does, without laying it out, for the fuzzer: not part of the API, behind the
/// `fuzz` feature.
#[cfg(feature = "fuzz")]
#[doc(hidden)]
pub use state::{Counts as StateCounts, read as read_state};

/// Draws a flowchart or a state diagram as [`render`] does, then measures the drawing: not part of the API, behind
/// the `measure` feature.
#[cfg(feature = "measure")]
#[doc(hidden)]
pub fn measure(source: &str, max_width: u16) -> Result<Measures, Error> {
    match source::Kind::of(source)? {
        source::Kind::Flowchart => flowchart::measure(source, max_width),
        source::Kind::State => state::measure(source, max_width),
        source::Kind::Sequence => Err(Error::Unsupported),
    }
}

/// Draws the diagram in `source` no wider than `max_width` columns.
pub fn render(source: &str, max_width: u16) -> Result<Canvas, Error> {
    render_with(source, Settings::new(max_width))
}

/// Draws the diagram in `source` as `settings` say.
///
/// ```
/// use vt_mermaid::{LongWords, Settings};
///
/// let settings = Settings::new(80).long_words(LongWords::Cut);
/// let canvas = vt_mermaid::render_with("flowchart LR\n  A[a-word-far-too-long-to-stand-on-one-line-of-a-node]", settings)?;
/// assert_eq!(canvas.rows(vt_mermaid::GlyphSet::Unicode).count(), 3);
/// # Ok::<(), vt_mermaid::Error>(())
/// ```
pub fn render_with(source: &str, settings: Settings) -> Result<Canvas, Error> {
    match source::Kind::of(source)? {
        source::Kind::Flowchart => flowchart::render_with(source, settings).map(|drawing| drawing.canvas),
        source::Kind::Sequence => sequence::render_with(source, settings),
        source::Kind::State => state::render_with(source, settings),
    }
}
