//! Flowcharts: read as mermaid reads them, then drawn as a graph.

mod diagram;
mod parse;
mod shape;

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

/// Draws `source` as [`render`](crate::render) does, long words broken.
#[cfg(test)]
pub(crate) fn render(source: &str, max_width: u16) -> Result<crate::graph::Drawing, Error> {
    crate::graph::render(&read_source(source)?, crate::settings::Settings::new(max_width))
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
#[cfg(test)]
pub(crate) fn measure(source: &str, max_width: u16) -> Result<crate::graph::measure::Measures, Error> {
    crate::graph::measured(&read_source(source)?, max_width)
}

/// Reads `source` within the limits `render` draws.
#[cfg(fuzzing)]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let flowchart = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: flowchart.nodes.len() + flowchart.subgraphs.len(), edges: flowchart.edges.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MAX_SOURCE_BYTES;
    use crate::graph::measure::{rows, violations};
    use crate::source::listed;

    /// Wide enough for every spike flowchart: the comparison with mermaid-text judges the width.
    const WIDTH: u16 = 400;

    fn spike() -> Vec<(String, String)> {
        listed("tests/spike/flowcharts.txt")
    }

    #[test]
    fn spike_flowcharts_match_their_snapshots() {
        for (name, source) in spike() {
            let drawing = render(&source, WIDTH).unwrap();
            insta::assert_snapshot!(name, rows(&drawing.canvas).join("\n"));
        }
    }

    #[test]
    fn spike_flowcharts_keep_the_level_c_invariants() {
        let found: Vec<String> = spike()
            .into_iter()
            .flat_map(|(name, source)| {
                let drawing = render(&source, WIDTH).unwrap();
                violations(&read_source(&source).unwrap().wrapped(drawing.wrap), &drawing)
                    .into_iter()
                    .map(move |found| format!("{name}: {found}"))
            })
            .collect();

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_source_at_the_length_limit_is_read() {
        let diagram = "flowchart LR\n  A --> B\n";
        let source = format!("{diagram}%%{}", " ".repeat(MAX_SOURCE_BYTES - diagram.len() - 2));

        assert_eq!(source.len(), MAX_SOURCE_BYTES);
        assert!(crate::render(&source, WIDTH).is_ok());
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused() {
        let source = format!("flowchart LR\n  A --> B\n%%{}", " ".repeat(MAX_SOURCE_BYTES));

        assert!(matches!(crate::render(&source, WIDTH), Err(Error::TooLong { bytes }) if bytes == source.len()));
    }

    /// Found by fuzzing, written as mermaid reads them: each one failed an assertion mmdr's layout only checked in debug
    /// builds.
    #[test]
    fn sources_that_broke_the_layout_before_are_still_drawn() {
        let sources =
            ["graph TD\nsubgraph \"1.fir\"\na\n end\nsubgraph 2. second\nd\nend", "graph TD\nA;B;subgraph ol styl a brokken;cc\nend"];

        for source in sources {
            assert!(render(source, WIDTH).is_ok(), "{source:?}");
        }
    }

    /// Found by fuzzing: a subgraph opened again inside itself held itself, so it was never placed.
    #[test]
    fn a_subgraph_repeated_inside_itself_is_one_group() {
        let drawing = render("flowchart\nsubgraph Si\nsubgraph Si\nSi\nend\nend", WIDTH).unwrap();

        assert_eq!(drawing.groups.len(), 1);
        assert!(drawing.groups.iter().all(|group| group.width > 0 && group.height > 0), "{:?}", drawing.groups);
    }
}
