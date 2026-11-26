//! ER diagrams: read as mermaid reads them, then drawn as a graph of tables joined by crow's feet.

mod diagram;
mod parse;

use crate::error::Error;
#[cfg(feature = "fuzz")]
use crate::graph::Counts;

pub(crate) use diagram::read;

/// Reads `source` as [`read`] does, for the tests that start from a source.
#[cfg(test)]
pub(crate) fn read_source(source: &str) -> Result<crate::graph::diagram::Diagram, Error> {
    read(&crate::source::prepare(source))
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let diagram = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: diagram.entities.len() + diagram.subgraphs.len(), edges: diagram.relationships.len() })
}

#[cfg(test)]
mod tests {
    use vt_canvas::GlyphSet;

    use super::*;
    use crate::graph::measure::{rows, violations};
    use crate::graph::{self, Drawing};
    use crate::settings::Settings;
    use crate::source::listed as sources;

    fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
        graph::render(&read_source(source)?, Settings::new(max_width))
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
                violations(&read_source(&source).unwrap().wrapped(drawing.wrap), &drawing)
                    .into_iter()
                    .map(move |found| format!("{name}: {found}"))
            })
            .collect();

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_diagram_draws_in_ascii_too() {
        let drawing = render("erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER }|..|| INVOICE : \"billed in\"", 60).unwrap();

        let ascii: Vec<String> = drawing.canvas.lines(GlyphSet::Ascii).collect();
        insta::assert_snapshot!(ascii.join("\n"));
    }

    /// Every corpus ER diagram mermaid accepts is drawn within 80 columns; only those with wide or zero-width characters,
    /// which the measures miscount, and one relationship of an entity to itself, its label left out, break the level C
    /// invariants.
    #[test]
    fn corpus_diagrams_are_drawn_within_80_columns() {
        let mut found = Vec::new();
        for (name, source) in sources("corpus/er") {
            let Ok(read) = read_source(&source) else { continue };
            let drawing = render(&source, 80).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(rows(&drawing.canvas).iter().all(|row| vt_canvas::text_width(row) <= 80), "{name}");
            found.extend(violations(&read.wrapped(drawing.wrap), &drawing).into_iter().map(|found| format!("{name}: {found}")));
        }

        assert!(found.iter().all(|found| found.contains("unicode") || found.starts_with("spec--er-parser-erDiagram--L530")), "{found:#?}");
    }
}
