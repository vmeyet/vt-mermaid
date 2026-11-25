//! Flowcharts: mmdr's layout snapped to the grid, edges routed on it, then drawn.

mod attributes;
mod diagram;
mod draw;
mod edge_label;
mod invisible;
mod label;
#[cfg(any(test, feature = "measure"))]
pub(crate) mod measure;
mod route;
mod shape;
mod snap;
mod source;

use vt_canvas::{Canvas, Rect};

use crate::error::{Error, MAX_AREA, MAX_SOURCE_BYTES};
use diagram::Diagram;
use route::Route;

/// A drawn flowchart and where each part went, in canvas cells.
#[derive(Debug, Clone)]
#[cfg_attr(not(any(test, feature = "measure")), expect(dead_code, reason = "the parts are read by the measures"))]
pub(crate) struct Drawing {
    pub(crate) canvas: Canvas,
    nodes: Vec<Rect>,
    groups: Vec<Rect>,
    routes: Vec<Route>,
    labels: Vec<Option<Rect>>,
}

/// Draws `source` as laid out, or turned top-down when it flows sideways and that is too wide.
pub(crate) fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let diagram = Diagram::read(source)?;
    match drawn(&diagram, max_width) {
        Err(Error::TooWide { .. }) if diagram.is_horizontal() => drawn(&Diagram::read_top_down(source)?, max_width),
        drawing => drawing,
    }
}

fn drawn(diagram: &Diagram, max_width: u16) -> Result<Drawing, Error> {
    let placement = snap::place(diagram);
    check_area(usize::from(placement.size.width), usize::from(placement.size.height))?;
    let (routes, grid) = route::route(diagram, &placement)?;
    let labels = edge_label::place(diagram, &routes, &grid);
    draw::draw(diagram, draw::Parts { nodes: placement.nodes, groups: placement.groups, routes, labels }, max_width)
}

fn check_area(width: usize, height: usize) -> Result<(), Error> {
    if width * height > MAX_AREA { Err(Error::TooLarge { width, height }) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::measure::{rows, violations};
    use super::*;

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

    /// Level C is not reachable everywhere: a long label between close parallel edges has nowhere to go, and
    /// mmdr misreads L1138's markdown strings into an edge squeezed between two others.
    const KNOWN_VIOLATIONS: [&str; 2] = [
        "docs--community-e2e-tests--L81: edge 2 start → push_develop label found no room",
        "docs--syntax-flowchart--L1138: edge 1 in → d label found no room",
    ];

    #[test]
    fn spike_flowcharts_keep_the_level_c_invariants_but_the_known_ones() {
        let found: Vec<String> = spike()
            .into_iter()
            .flat_map(|(name, source)| {
                let drawing = render(&source, WIDTH).unwrap();
                violations(&Diagram::read(&source).unwrap(), &drawing).into_iter().map(move |found| format!("{name}: {found}"))
            })
            .collect();

        assert_eq!(found, KNOWN_VIOLATIONS);
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

    #[test]
    fn a_drawing_at_the_area_limit_is_accepted() {
        assert_eq!(check_area(MAX_AREA / 100, 100), Ok(()));
    }

    #[test]
    fn a_drawing_over_the_area_limit_is_refused() {
        assert_eq!(check_area(MAX_AREA / 100 + 1, 100), Err(Error::TooLarge { width: MAX_AREA / 100 + 1, height: 100 }));
    }

    /// Found by fuzzing: each one failed an assertion mmdr only checks in debug builds.
    #[test]
    fn layouts_mmdr_grades_as_flawed_are_still_drawn() {
        let sources = ["graph T\nsubgraph \"1.fir\na\n end\nsubgraph 2. second\nd", "h-/\nA B;subgraph ol styl a brokken;cc"];

        for source in sources {
            assert!(render(source, WIDTH).is_ok(), "{source:?}");
        }
    }

    /// Found by fuzzing: each subgraph holds the other, so neither was placed.
    #[test]
    fn subgraphs_sharing_a_name_are_both_drawn() {
        let drawing = render("flowchart\nsubgraph Si\nsubgraph Si\nSi", WIDTH).unwrap();

        assert_eq!(drawing.groups.len(), 2);
        assert!(drawing.groups.iter().all(|group| group.width > 0 && group.height > 0), "{:?}", drawing.groups);
    }

    #[test]
    fn a_sideways_flowchart_too_wide_is_turned_top_down() {
        let source = "flowchart LR\n  A[First step] --> B[Second step] --> C[Third step]";

        let sideways = render(source, WIDTH).unwrap();
        let turned = render(source, 20).unwrap();

        assert_eq!(rows(&sideways.canvas).len(), 3);
        assert!(rows(&turned.canvas).len() > 9);
    }

    #[test]
    fn a_drawing_wider_than_allowed_is_refused() {
        let source = "flowchart LR\n  A[A long first label] --> B[A long second label]";

        assert!(matches!(render(source, 20), Err(Error::TooWide { max_width: 20, .. })));
    }
}
