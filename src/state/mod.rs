//! State diagrams: read as mermaid reads them, then drawn as a graph.

mod diagram;
mod parse;

#[cfg(any(test, fuzzing))]
use crate::error::Error;
#[cfg(fuzzing)]
use crate::graph::Counts;

pub(crate) use diagram::read;

/// Reads `source` as [`read`] does, for the tests that start from a source.
#[cfg(test)]
pub(crate) fn read_source(source: &str) -> Result<crate::graph::diagram::Diagram, Error> {
    read(&crate::source::prepare(source))
}

/// Reads `source` within the limits `render` draws.
#[cfg(fuzzing)]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let state = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: state.nodes.len(), edges: state.edges.len() })
}

#[cfg(test)]
mod tests {
    use vt_canvas::{Canvas, GlyphSet};

    use super::*;
    use crate::Role;
    use crate::graph::{self, measure::rows};
    use crate::settings::Settings;
    use crate::source::listed;

    fn render_with(source: &str, settings: Settings) -> Result<Canvas<Role>, Error> {
        graph::render(&read_source(source)?, settings).map(|drawing| drawing.canvas)
    }

    #[test]
    fn listed_diagrams_match_their_snapshots_at_100_columns() {
        for (name, source) in listed("tests/state/diagrams.txt") {
            insta::assert_snapshot!(name, rows(&render_with(&source, Settings::new(100)).unwrap()).join("\n"));
        }
    }

    #[test]
    fn a_diagram_draws_in_ascii_too() {
        let canvas = render_with("stateDiagram-v2\n  state c <<choice>>\n  [*] --> c\n  c --> A\n  A --> [*]", Settings::new(40)).unwrap();

        let ascii: Vec<String> = canvas.lines(GlyphSet::Ascii).collect();
        insta::assert_snapshot!(ascii.join("\n"));
    }

    /// Every corpus diagram mermaid accepts is drawn within 80 columns, each node and label whole, each edge joining
    /// both its ends and nothing overlapping; only a label left without room, as the layout gives two labels of a
    /// cycle between the same states, is let through.
    #[test]
    fn corpus_diagrams_are_drawn_within_80_columns_keeping_the_level_c_invariants() {
        let mut found = Vec::new();
        for (name, source) in listed("corpus/state") {
            let Ok(measures) = read_source(&source).and_then(|written| graph::measured(&written, 80)) else { continue };
            found.extend(
                measures
                    .violations
                    .into_iter()
                    .filter(|found| !found.ends_with("label found no room"))
                    .map(|found| format!("{name}: {found}")),
            );
        }

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_sideways_note_too_wide_goes_under_its_state() {
        let source = "stateDiagram-v2\n  A --> B\n  note right of A : a note far too long to stand beside its state";

        let beside = rows(&render_with(source, Settings::new(100)).unwrap());
        let under = rows(&render_with(source, Settings::new(40)).unwrap());

        assert!(beside[1].contains("├╌"), "{beside:?}");
        assert!(under.len() > beside.len() + 3, "{under:?}");
    }
}
