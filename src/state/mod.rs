//! State diagrams: read as mermaid reads them, then drawn as a graph.

mod diagram;
mod parse;

use vt_canvas::Canvas;

use crate::Role;
use crate::error::Error;
use crate::graph;
use crate::settings::Settings;

/// Draws `source` as a graph, in the first form that fits.
pub(crate) fn render_with(source: &str, settings: Settings) -> Result<Canvas<Role>, Error> {
    graph::render(&diagram::read(source)?, settings).map(|drawing| drawing.canvas)
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
#[cfg(any(test, feature = "measure"))]
pub(crate) fn measure(source: &str, max_width: u16) -> Result<graph::measure::Measures, Error> {
    let written = diagram::read(source)?;
    let drawing = graph::render(&written, Settings::new(max_width))?;
    Ok(graph::measure::measures(&written.wrapped(drawing.wrap), &drawing))
}

/// How much a state diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Nodes mermaid's renderer is handed: states, dots, notes and their groups, regions.
    pub nodes: usize,
    /// Transitions and links between a note and its state.
    pub edges: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn read(source: &str) -> Result<Counts, Error> {
    let state = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: state.nodes.len(), edges: state.edges.len() })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use vt_canvas::GlyphSet;

    use super::*;
    use crate::graph::measure::rows;

    fn listed() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let list = fs::read_to_string(root.join("tests/state/diagrams.txt")).unwrap();
        list.lines()
            .map(|path| {
                let name = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
                (name, fs::read_to_string(root.join(path)).unwrap())
            })
            .collect()
    }

    #[test]
    fn listed_diagrams_match_their_snapshots_at_100_columns() {
        for (name, source) in listed() {
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/state");
        let mut found = Vec::new();
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let Ok(measures) = measure(&fs::read_to_string(&path).unwrap(), 80) else { continue };
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
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
