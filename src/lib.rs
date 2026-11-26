//! Mermaid diagrams drawn as text on a vt-canvas grid.
//!
//! ```
//! let canvas = vt_mermaid::render("flowchart LR\n  A --> B", 80)?;
//! let lines: Vec<String> = canvas.lines(vt_mermaid::GlyphSet::Unicode).collect();
//! assert_eq!(lines.len(), 3);
//! # Ok::<(), vt_mermaid::Error>(())
//! ```

mod class;
mod er;
mod error;
mod flowchart;
mod graph;
mod kind;
mod label;
mod role;
mod sequence;
mod settings;
mod source;
mod state;

pub use error::{
    Error, MAX_AREA, MAX_CELLS, MAX_EDGES, MAX_MESSAGES, MAX_NODES, MAX_PARTICIPANTS, MAX_ROUTING_STEPS, MAX_SOURCE_BYTES, MAX_WIDTH,
};
use kind::Kind;
pub use role::Role;
pub use settings::{LongWords, Settings};
pub use vt_canvas::{Canvas, GlyphSet, Run};

/// Exact measures of a drawing, for the scoreboard: not part of the API, behind the `measure` feature.
#[cfg(feature = "measure")]
#[doc(hidden)]
pub use graph::measure::Measures;

/// Reads a diagram as `render` does, without laying it out, for the fuzzer: not part of the API, behind the `fuzz`
/// feature.
#[cfg(feature = "fuzz")]
#[doc(hidden)]
pub use {
    class::counts as read_class,
    er::counts as read_er,
    flowchart::counts as read_flowchart,
    graph::Counts as GraphCounts,
    sequence::{Counts as SequenceCounts, counts as read_sequence},
    state::counts as read_state,
};

/// Draws a flowchart, a state, a class or an ER diagram as [`render`] does, then measures the drawing: not part of the
/// API, behind the `measure` feature.
#[cfg(feature = "measure")]
#[doc(hidden)]
pub fn measure(source: &str, max_width: u16) -> Result<Measures, Error> {
    let prepared = source::prepared(source)?;
    match Kind::of(&prepared)? {
        Kind::Graph(read) => graph::measured(&read(&prepared)?, max_width),
        Kind::Own(_) => Err(Error::Unsupported { kind: source::header(&prepared.text) }),
    }
}

/// Draws the diagram in `source` no wider than `max_width` columns, and never more than [`MAX_WIDTH`].
pub fn render(source: &str, max_width: u16) -> Result<Canvas<Role>, Error> {
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
pub fn render_with(source: &str, settings: Settings) -> Result<Canvas<Role>, Error> {
    let prepared = source::prepared(source)?;
    Kind::of(&prepared)?.render(&prepared, settings.clamped())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(drawn: Result<Canvas<Role>, Error>) -> Result<Vec<String>, Error> {
        drawn.map(|canvas| canvas.lines(GlyphSet::Unicode).collect())
    }

    /// A chain of 60 nodes flowing sideways needs more than [`MAX_WIDTH`] columns as written.
    fn long_chain() -> String {
        format!("flowchart LR\n{}", (1..60).map(|index| format!("  n{} --> n{index}\n", index - 1)).collect::<String>())
    }

    #[test]
    fn a_width_over_the_canvas_limit_draws_as_the_limit_does() {
        let at_the_limit = lines(render(&long_chain(), MAX_WIDTH));

        assert!(at_the_limit.is_ok(), "{at_the_limit:?}");
        assert_eq!(lines(render(&long_chain(), MAX_WIDTH + 1)), at_the_limit);
        assert_eq!(lines(render(&long_chain(), u16::MAX)), at_the_limit);
    }

    #[test]
    fn a_width_over_the_canvas_limit_set_on_the_settings_draws_as_the_limit_does() {
        let mut settings = Settings::new(MAX_WIDTH);
        settings.max_width = u16::MAX;

        assert_eq!(lines(render_with(&long_chain(), settings)), lines(render(&long_chain(), MAX_WIDTH)));
    }

    #[test]
    fn composite_states_nested_to_the_node_limit_are_too_large_to_route() {
        let opened: String = (0..MAX_NODES - 1).map(|index| format!("  state c{index} {{\n")).collect();
        let source = format!("stateDiagram-v2\n{opened}  deep\n{}", "  }\n".repeat(MAX_NODES - 1));

        assert!(matches!(render(&source, MAX_WIDTH), Err(Error::TooLarge { max_cells: MAX_AREA, .. })));
    }

    /// Found by fuzzing: subgraphs named alike around a relationship repeated between two entities leave it no path
    /// whatever the placement, which the ladder stops trying once the same edge is stuck twice.
    #[test]
    fn an_edge_walled_in_whatever_the_placement_is_unroutable() {
        let source = include_str!("../benches/inputs/fuzzed-er.mmd");

        assert_eq!(render(source, 200), Err(Error::Unroutable { from: "I1".to_owned(), to: "F1".to_owned() }));
    }

    #[test]
    fn a_kind_mermaid_knows_but_not_drawn_is_unsupported_and_names_its_header() {
        let refused = render("xychart-beta\n  bar [1, 2]", 80);

        assert_eq!(refused, Err(Error::Unsupported { kind: "xychart-beta".to_owned() }));
        assert_eq!(refused.unwrap_err().to_string(), "xychart-beta diagrams are not drawn as text yet");
    }
}
