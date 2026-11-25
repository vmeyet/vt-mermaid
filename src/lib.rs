//! Mermaid diagrams drawn as text on a vt-canvas grid.
//!
//! ```
//! let canvas = vt_mermaid::render("flowchart LR\n  A --> B", 80)?;
//! let rows: Vec<String> =
//!     canvas.rows(vt_mermaid::GlyphSet::Unicode).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect();
//! assert_eq!(rows.len(), 3);
//! # Ok::<(), vt_mermaid::Error>(())
//! ```

mod error;
mod flowchart;

pub use error::{Error, MAX_AREA, MAX_EDGES, MAX_NODES, MAX_SOURCE_BYTES};
pub use vt_canvas::{Canvas, GlyphSet, Role, Run};

/// Exact measures of a flowchart drawing, for the scoreboard: not part of the API, behind the `measure` feature.
#[cfg(feature = "measure")]
#[doc(hidden)]
pub use flowchart::measure::{Measures, measure};

/// Draws the flowchart in `source` no wider than `max_width` columns.
pub fn render(source: &str, max_width: u16) -> Result<Canvas, Error> {
    flowchart::render(source, max_width).map(|drawing| drawing.canvas)
}
