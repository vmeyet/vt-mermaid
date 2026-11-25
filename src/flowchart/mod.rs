//! Flowcharts: read as mermaid reads them, then drawn as a graph.

mod diagram;
mod parse;
mod shape;

use crate::error::Error;
use crate::graph::{self, Drawing};
use crate::settings::Settings;

pub(crate) use diagram::read;

/// Draws `source` as a graph, in the first form that fits.
pub(crate) fn render_with(source: &str, settings: Settings) -> Result<Drawing, Error> {
    graph::render(&read(source)?, settings)
}

/// Draws `source` as [`render_with`] does, long words broken.
#[cfg(any(test, feature = "measure"))]
pub(crate) fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
    render_with(source, Settings::new(max_width))
}

/// How much a flowchart holds once read.
#[cfg(feature = "fuzz")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Nodes and subgraphs.
    pub nodes: usize,
    /// Edges, invisible ones included.
    pub edges: usize,
}

/// Reads `source` within the limits `render` draws.
#[cfg(feature = "fuzz")]
pub fn counts(source: &str) -> Result<Counts, Error> {
    let flowchart = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: flowchart.nodes.len() + flowchart.subgraphs.len(), edges: flowchart.edges.len() })
}

/// Draws `source` as [`render`](crate::render) does, then measures the drawing.
#[cfg(any(test, feature = "measure"))]
pub fn measure(source: &str, max_width: u16) -> Result<graph::measure::Measures, Error> {
    let drawing = render(source, max_width)?;
    Ok(graph::measure::measures(&read(source)?.wrapped(drawing.wrap), &drawing))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::error::MAX_SOURCE_BYTES;
    use crate::graph::measure::{rows, violations};

    /// Wide enough for every spike flowchart: the comparison with mermaid-text judges the width.
    const WIDTH: u16 = 400;

    fn spike() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let list = fs::read_to_string(root.join("tests/spike/flowcharts.txt")).unwrap();
        list.lines()
            .map(|path| {
                let name = Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
                (name, fs::read_to_string(root.join(path)).unwrap())
            })
            .collect()
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
                violations(&read(&source).unwrap().wrapped(drawing.wrap), &drawing).into_iter().map(move |found| format!("{name}: {found}"))
            })
            .collect();

        assert_eq!(found, [""; 0]);
    }

    #[test]
    fn a_source_at_the_length_limit_is_read() {
        let diagram = "flowchart LR\n  A --> B\n";
        let source = format!("{diagram}%%{}", " ".repeat(MAX_SOURCE_BYTES - diagram.len() - 2));

        assert_eq!(source.len(), MAX_SOURCE_BYTES);
        assert!(render(&source, WIDTH).is_ok());
    }

    #[test]
    fn a_source_over_the_length_limit_is_refused() {
        let source = format!("flowchart LR\n  A --> B\n%%{}", " ".repeat(MAX_SOURCE_BYTES));

        assert!(matches!(render(&source, WIDTH), Err(Error::TooLong { bytes }) if bytes == source.len()));
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
