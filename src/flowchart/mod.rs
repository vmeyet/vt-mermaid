//! Flowcharts: mmdr's layout snapped to the grid, edges routed on it, then drawn.

mod attributes;
mod diagram;
mod draw;
mod edge_label;
mod invisible;
mod label;
#[cfg_attr(not(test), expect(dead_code, reason = "flowcharts are drawn with this layout once it beats the snapped one"))]
mod layout;
#[cfg(any(test, feature = "measure"))]
pub(crate) mod measure;
#[cfg_attr(not(test), expect(dead_code, reason = "flowcharts switch to this parser once it agrees with mermaid"))]
mod parse;
mod port;
mod route;
mod shape;
mod snap;
mod source;

use vt_canvas::{Canvas, Rect};

use crate::error::{Error, MAX_AREA, MAX_SOURCE_BYTES};
use diagram::Diagram;
use route::Route;
use snap::{MARGIN, Placement, Room};

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

/// Columns a folded drawing gets, in eighths of the width allowed: edges going round the blocks take the rest.
const FOLDS: [i32; 3] = [8, 7, 6];

/// Draws `source` in the first form that fits `max_width` and whose edges find a path: with the whole room the
/// layout leaves, then half of it, then none and its wide ranks folded; each time as written, then turned top-down
/// when it flows sideways.
pub(crate) fn render(source: &str, max_width: u16) -> Result<Drawing, Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Error::TooLong { bytes: source.len() });
    }
    let written = Diagram::read(source)?;
    let as_laid_out = drawn(&written, Room::Whole, max_width);
    if !may_fit_tighter(&as_laid_out) {
        return as_laid_out;
    }
    let turned = if written.flows_sideways() { Some(Diagram::read_top_down(source)?) } else { None };
    let folds = FOLDS.map(|eighths| Room::Folded { columns: i32::from(max_width) * eighths / 8 });
    let tighter = std::iter::once(Room::Half)
        .chain(folds)
        .flat_map(|room| std::iter::once(&written).chain(&turned).map(move |diagram| (diagram, room)));
    turned
        .iter()
        .map(|diagram| (diagram, Room::Whole))
        .chain(tighter)
        .fold(as_laid_out, |drawing, (diagram, room)| if may_fit_tighter(&drawing) { drawn(diagram, room, max_width) } else { drawing })
}

/// A drawing too wide, or one whose blocks leave an edge no path, may come out with the blocks placed another way.
fn may_fit_tighter(drawing: &Result<Drawing, Error>) -> bool {
    matches!(drawing, Err(Error::TooWide { .. } | Error::Unroutable { .. }))
}

fn drawn(diagram: &Diagram, room: Room, max_width: u16) -> Result<Drawing, Error> {
    let placement = snap::place(diagram, room);
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
    fn a_group_flowing_sideways_too_wide_is_turned_top_down() {
        let source = "flowchart TB\n  subgraph Steps\n    direction LR\n    A[First step] --> B[Second step] --> C[Third step]\n  end";

        let sideways = render(source, WIDTH).unwrap();
        let turned = render(source, 30).unwrap();

        assert_eq!(rows(&sideways.canvas).len(), 7);
        assert!(rows(&turned.canvas).len() > 13);
    }

    #[test]
    fn a_rank_too_wide_is_folded_into_rows() {
        let source = "flowchart TB\n  A[Start] --> B[First option] & C[Second option] & D[Third option]";

        let one_row = render(source, WIDTH).unwrap();
        let folded = render(source, 40).unwrap();

        assert_eq!(rows(&one_row.canvas).len(), 9);
        assert_eq!(
            rows(&folded.canvas),
            [
                " ┌───────┐                           ",
                " │ Start │                           ",
                " └┬┬───┬─┘                           ",
                "┌─┘│   └────────────────────┐        ",
                "│  │                        │        ",
                "│  ▾                        ▾        ",
                "│┌──────────────┐    ┌──────────────┐",
                "││ First option │    │ Third option │",
                "│└──────────────┘    └──────────────┘",
                "│                                    ",
                "└────────┐                           ",
                "         ▾                           ",
                " ┌───────────────┐                   ",
                " │ Second option │                   ",
                " └───────────────┘                   ",
            ]
        );
    }

    #[test]
    fn a_folded_drawing_whose_edges_go_past_the_width_is_folded_narrower() {
        let source = "flowchart TB\n  A[Start] --> B[First option] & C[Second option] & D[Third option]";

        let two_in_a_row = render(source, 38).unwrap();
        let one_in_a_row = render(source, 30).unwrap();

        let size = |drawing: &Drawing| (rows(&drawing.canvas)[0].chars().count(), rows(&drawing.canvas).len());
        assert_eq!(size(&two_in_a_row), (37, 15));
        assert_eq!(size(&one_in_a_row).1, 21);
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
    fn a_drawing_wider_than_allowed_is_refused() {
        let source = "flowchart LR\n  A[A long first label] --> B[A long second label]";

        assert!(matches!(render(source, 20), Err(Error::TooWide { max_width: 20, .. })));
    }
}
