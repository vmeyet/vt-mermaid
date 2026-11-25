//! ER diagrams: read as mermaid reads them, then drawn as a graph of tables joined by crow's feet.

mod diagram;
mod parse;

use crate::error::Error;
use crate::graph::{self, Drawing};
use crate::settings::Settings;

/// Draws `source` as a graph, in the first form that fits.
pub(crate) fn render_with(source: &str, settings: Settings) -> Result<Drawing, Error> {
    graph::render(&diagram::read(source)?, settings)
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
#[cfg(any(test, feature = "measure"))]
pub(crate) fn measure(source: &str, max_width: u16) -> Result<graph::measure::Measures, Error> {
    let written = diagram::read(source)?;
    let drawing = graph::render(&written, Settings::new(max_width))?;
    Ok(graph::measure::measures(&written.wrapped(drawing.wrap), &drawing))
}

/// How much an ER diagram holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Entities and subgraphs.
    pub nodes: usize,
    /// Relationships.
    pub edges: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let diagram = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: diagram.entities.len() + diagram.subgraphs.len(), edges: diagram.relationships.len() })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use vt_canvas::GlyphSet;

    use super::*;
    use crate::graph::measure::{rows, violations};

    fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
        render_with(source, Settings::new(max_width))
    }

    /// Each file `list` names, or each file in the folder `list`, by its name.
    fn sources(list: &str) -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let paths: Vec<String> = if root.join(list).is_dir() {
            fs::read_dir(root.join(list)).unwrap().map(|entry| format!("{list}/{}", entry.unwrap().file_name().to_string_lossy())).collect()
        } else {
            fs::read_to_string(root.join(list)).unwrap().lines().map(str::to_owned).collect()
        };
        let named = |path: String| {
            (Path::new(&path).file_stem().unwrap().to_string_lossy().into_owned(), fs::read_to_string(root.join(&path)).unwrap())
        };
        paths.into_iter().map(named).collect()
    }

    #[test]
    fn listed_diagrams_match_their_snapshots_at_100_columns() {
        for (name, source) in sources("tests/er/diagrams.txt") {
            insta::assert_snapshot!(name, rows(&render(&source, 100).unwrap().canvas).join("\n"));
        }
    }

    #[test]
    fn listed_diagrams_keep_the_level_c_invariants() {
        let found: Vec<String> = sources("tests/er/diagrams.txt")
            .into_iter()
            .flat_map(|(name, source)| {
                let drawing = render(&source, 100).unwrap();
                violations(&diagram::read(&source).unwrap().wrapped(drawing.wrap), &drawing)
                    .into_iter()
                    .map(move |found| format!("{name}: {found}"))
            })
            .collect();

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_diagram_draws_in_ascii_too() {
        let drawing = render("erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER }|..|| INVOICE : \"billed in\"", 60).unwrap();

        let ascii: Vec<String> = drawing.canvas.rows(GlyphSet::Ascii).map(|runs| runs.into_iter().map(|run| run.text).collect()).collect();
        insta::assert_snapshot!(ascii.join("\n"));
    }

    /// Every corpus ER diagram mermaid accepts is drawn within 80 columns; only those with wide or zero-width characters,
    /// which the measures miscount, and one relationship of an entity to itself, its label left out, break the level C
    /// invariants.
    #[test]
    fn corpus_diagrams_are_drawn_within_80_columns() {
        let mut found = Vec::new();
        for (name, source) in sources("corpus/er") {
            let Ok(read) = diagram::read(&source) else { continue };
            let drawing = render(&source, 80).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(rows(&drawing.canvas).iter().all(|row| vt_canvas::width(row) <= 80), "{name}");
            found.extend(violations(&read.wrapped(drawing.wrap), &drawing).into_iter().map(|found| format!("{name}: {found}")));
        }

        assert!(found.iter().all(|found| found.contains("unicode") || found.starts_with("spec--er-parser-erDiagram--L530")), "{found:#?}");
    }
}
