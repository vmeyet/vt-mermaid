//! Flowcharts: laid out in layers on the grid, edges routed on it, then drawn.

mod diagram;
mod draw;
mod edge_label;
mod layout;
#[cfg(any(test, feature = "measure"))]
pub(crate) mod measure;
mod parse;
mod port;
mod route;
mod shape;

use vt_canvas::{Canvas, Rect};

use crate::error::{Error, MAX_AREA};
use crate::label::Wrap;
use crate::settings::{LongWords, Settings};
use diagram::Diagram;
use layout::{Fit, MARGIN, Placement};
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

/// Columns a folded layout gets, in eighths of the width allowed, then a packed one.
const FOLDED: [i32; 5] = [8, 7, 6, 5, 4];
const PACKED: [i32; 3] = [8, 6, 4];

/// Draws `source` in the first form that fits `max_width` and whose edges find a path: laid out loose, then tight,
/// then with its wide ranks folded narrower and narrower, then packed without room for the links passing a rank;
/// each time as written, then turned top-down when it flows sideways.
pub(crate) fn render_with(source: &str, settings: Settings) -> Result<Drawing, Error> {
    let max_width = settings.max_width;
    let read = Diagram::read(source)?;
    let written = if settings.long_words == LongWords::Break { read } else { read.wrapped(Wrap::wide(settings.long_words)) };
    let turned = written.flows_sideways().then(|| written.turned_top_down());
    let columns = |eighths: i32| i32::from(max_width) * eighths / 8;
    let folded = FOLDED.map(|eighths| Fit::Folded { columns: columns(eighths) });
    let packed = PACKED.map(|eighths| Fit::Packed { columns: columns(eighths) });
    let fits = [Fit::Loose, Fit::Tight].into_iter().chain(folded).chain(packed);
    let forms = fits.flat_map(|fit| std::iter::once(&written).chain(&turned).map(move |diagram| (diagram, fit)));
    let mut drawing = Err(Error::TooWide { width: usize::MAX, max_width });
    for (diagram, fit) in forms {
        if !may_fit_tighter(&drawing) {
            break;
        }
        drawing = drawn(diagram, layout::place(diagram, fit), max_width);
    }
    drawing
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
pub fn read(source: &str) -> Result<Counts, Error> {
    let flowchart = parse::parse(source, parse::Limits::DRAWN)?;
    Ok(Counts { nodes: flowchart.nodes.len() + flowchart.subgraphs.len(), edges: flowchart.edges.len() })
}

/// A drawing too wide, or one whose blocks leave an edge no path, may come out with the blocks placed another way.
fn may_fit_tighter(drawing: &Result<Drawing, Error>) -> bool {
    matches!(drawing, Err(Error::TooWide { .. } | Error::Unroutable { .. }))
}

fn drawn(diagram: &Diagram, placement: Placement, max_width: u16) -> Result<Drawing, Error> {
    check_width(&placement, max_width)?;
    check_area(usize::from(placement.size.width), usize::from(placement.size.height))?;
    let routed = route::route(diagram, &placement)?;
    let labels = edge_label::place(diagram, &routed.routes, &routed.grid, &placement.labels);
    let parts = draw::Parts { nodes: placement.nodes, groups: placement.groups, titles: routed.titles, routes: routed.routes, labels };
    draw::draw(diagram, parts, max_width)
}

/// Blocks wider than allowed are refused before their edges are routed: routes only add to the width.
fn check_width(placement: &Placement, max_width: u16) -> Result<(), Error> {
    let width = placement.size.width.saturating_sub(2 * MARGIN);
    if width > max_width { Err(Error::TooWide { width: usize::from(width), max_width }) } else { Ok(()) }
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
    use crate::error::MAX_SOURCE_BYTES;

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
                violations(&Diagram::read(&source).unwrap(), &drawing).into_iter().map(move |found| format!("{name}: {found}"))
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

    #[test]
    fn a_drawing_at_the_area_limit_is_accepted() {
        assert_eq!(check_area(MAX_AREA / 100, 100), Ok(()));
    }

    #[test]
    fn a_drawing_over_the_area_limit_is_refused() {
        assert_eq!(check_area(MAX_AREA / 100 + 1, 100), Err(Error::TooLarge { width: MAX_AREA / 100 + 1, height: 100 }));
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

    #[test]
    fn a_sideways_flowchart_too_wide_is_turned_top_down() {
        let source = "flowchart LR\n  A[First step] --> B[Second step] --> C[Third step]";

        let sideways = render(source, WIDTH).unwrap();
        let turned = render(source, 20).unwrap();

        assert_eq!(rows(&sideways.canvas).len(), 3);
        assert!(rows(&turned.canvas).len() > 9);
    }

    #[test]
    fn a_group_flowing_sideways_too_wide_is_turned_top_down() {
        let source = "flowchart TB\n  subgraph Steps\n    direction LR\n    A[First step] --> B[Second step] --> C[Third step]\n  end";

        let sideways = render(source, WIDTH).unwrap();
        let turned = render(source, 30).unwrap();

        assert_eq!(rows(&sideways.canvas).len(), 7);
        assert!(rows(&turned.canvas).len() > 13);
    }

    #[test]
    fn a_rank_too_wide_is_folded_and_its_edges_share_a_trunk() {
        let source = "flowchart TB\n  A[Start] --> B[First option] & C[Second option] & D[Third option]";

        let one_row = render(source, WIDTH).unwrap();
        let folded = render(source, 40).unwrap();

        assert_eq!(rows(&one_row.canvas).len(), 9);
        assert_eq!(
            rows(&folded.canvas),
            [
                "                      ┌───────┐    ",
                "                      │ Start │    ",
                "                      └───┬───┘    ",
                "                          │        ",
                "       ┌─────────┬────────┤        ",
                "       ▾         │        ▾        ",
                "┌──────────────┐ │┌───────────────┐",
                "│ First option │ ││ Second option │",
                "└──────────────┘ │└───────────────┘",
                "                 │                 ",
                "                 │                 ",
                "                 ▾                 ",
                "              ┌──────────────┐     ",
                "              │ Third option │     ",
                "              └──────────────┘     ",
            ]
        );
    }

    #[test]
    fn a_folded_drawing_still_too_wide_is_folded_narrower() {
        let source = "flowchart TB\n  A[Start] --> B[First option] & C[Second option] & D[Third option]";

        let two_in_a_row = render(source, 37).unwrap();
        let one_in_a_row = render(source, 36).unwrap();

        let size = |drawing: &Drawing| (rows(&drawing.canvas)[0].chars().count(), rows(&drawing.canvas).len());
        assert_eq!(size(&two_in_a_row), (35, 15));
        assert_eq!(size(&one_in_a_row), (21, 21));
    }

    #[test]
    fn a_flowchart_too_dense_to_fold_is_packed_without_room_for_its_links() {
        let source = include_str!("../../corpus/flowchart/e2e--platform-dev-diagrams-layout-tests-life-choices.mmd");
        let diagram = Diagram::read(source).unwrap();

        let folded = drawn(&diagram, layout::place(&diagram, Fit::Folded { columns: 40 }), 80);
        let packed = render(source, 80);

        assert!(matches!(folded, Err(Error::TooWide { .. })), "{:?}", folded.map(|drawing| rows(&drawing.canvas)));
        assert!(packed.is_ok());
    }

    #[test]
    fn only_a_drawing_too_wide_or_with_an_edge_without_a_path_is_tried_tighter() {
        let no_path = Error::Unroutable { from: "A".to_owned(), to: "B".to_owned() };

        assert!(may_fit_tighter(&Err(Error::TooWide { width: 90, max_width: 80 })));
        assert!(may_fit_tighter(&Err(no_path)));
        assert!(!may_fit_tighter(&Err(Error::TooLarge { width: 60, height: 5_000 })));
        assert!(!may_fit_tighter(&render("flowchart LR\n  A --> B", WIDTH)));
    }

    #[test]
    fn a_long_word_is_broken_over_lines_unless_the_settings_cut_it() {
        let source = "flowchart LR\n  A[see state-diagram-new-arrow-florbs-and-more now]";

        let broken = render(source, WIDTH).unwrap();
        let cut = render_with(source, Settings::new(WIDTH).long_words(LongWords::Cut)).unwrap();

        assert_eq!(rows(&broken.canvas)[1..3], ["│ see state-diagram-new-arrow- │", "│     florbs-and-more now      │"]);
        assert_eq!(
            rows(&cut.canvas)[1..4],
            ["│              see               │", "│ state-diagram-new-arrow-florb⋯ │", "│              now               │"]
        );
    }

    #[test]
    fn a_drawing_wider_than_allowed_is_refused() {
        let source = "flowchart LR\n  A[A long first label] --> B[A long second label]";

        assert!(matches!(render(source, 20), Err(Error::TooWide { max_width: 20, .. })));
    }
}
