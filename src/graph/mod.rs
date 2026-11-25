//! Graphs (flowcharts, state diagrams): laid out in layers on the grid, edges routed on it, then drawn.

pub(crate) mod diagram;
mod draw;
mod edge_label;
mod layout;
#[cfg(any(test, feature = "measure"))]
pub(crate) mod measure;
mod port;
mod route;
mod shape;

use vt_canvas::{Canvas, Rect};

use crate::Role;
use crate::error::{Error, MAX_AREA};
use crate::label::Wrap;
use crate::settings::Settings;
use diagram::Diagram;
use layout::{Fit, MARGIN, Placement};
use route::Route;

/// A drawn graph and where each part went, in canvas cells.
#[derive(Debug, Clone)]
#[cfg_attr(not(any(test, feature = "measure")), expect(dead_code, reason = "the parts are read by the measures"))]
pub(crate) struct Drawing {
    pub(crate) canvas: Canvas<Role>,
    pub(crate) nodes: Vec<Rect>,
    pub(crate) groups: Vec<Rect>,
    routes: Vec<Route>,
    labels: Vec<Option<Rect>>,
    /// How the nodes' text was cut into lines to fit.
    pub(crate) wrap: Wrap,
}

/// Columns a node's label is wrapped at when the flowchart is too wide as written, before any rank is folded.
const NARROW: usize = 20;
const NARROWEST: usize = 14;
/// Columns a folded layout gets, in eighths of the width allowed; with labels at their narrowest; then a packed one.
const FOLDED: [i32; 5] = [8, 7, 6, 5, 4];
const FOLDED_NARROWEST: [i32; 7] = [8, 7, 6, 5, 4, 3, 2];
const PACKED: [i32; 3] = [8, 6, 4];

/// Draws `written` in the first form that fits the width and whose edges find a path: laid out loose, then tight,
/// then tight with its labels wrapped narrower, then with its wide ranks folded narrower and narrower, the labels
/// wide then at their narrowest, then packed without room for the links passing a rank; each time as written, then
/// turned top-down when it flows sideways.
pub(crate) fn render(written: &Diagram, settings: Settings) -> Result<Drawing, Error> {
    let max_width = settings.max_width;
    let wide = Wrap::wide(settings.long_words);
    let columns = |eighths: i32| i32::from(max_width) * eighths / 8;
    let folded = FOLDED.map(|eighths| Fit::Folded { columns: columns(eighths) });
    let folded_narrowest = FOLDED_NARROWEST.map(|eighths| Fit::Folded { columns: columns(eighths) });
    let packed = PACKED.map(|eighths| Fit::Packed { columns: columns(eighths) });
    let ladder: [(Wrap, &[Fit]); 6] = [
        (wide, &[Fit::Loose, Fit::Tight]),
        (wide.narrowed(NARROW), &[Fit::Tight]),
        (wide.narrowed(NARROWEST), &[Fit::Tight]),
        (wide, &folded),
        (wide.narrowed(NARROWEST), &folded_narrowest),
        (wide, &packed),
    ];
    let mut drawing = Err(Error::TooWide { width: usize::MAX, max_width });
    for (wrap, fits) in ladder {
        if !may_fit_tighter(&drawing) {
            break;
        }
        let wrapped = written.wrapped(wrap);
        let turned = wrapped.flows_sideways().then(|| wrapped.turned_top_down());
        for (diagram, fit) in fits.iter().flat_map(|fit| std::iter::once(&wrapped).chain(&turned).map(move |diagram| (diagram, *fit))) {
            if !may_fit_tighter(&drawing) {
                break;
            }
            drawing = drawn(diagram, layout::place(diagram, fit), max_width);
        }
    }
    drawing
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
    let end_labels = edge_label::place_ends(diagram, &routed.routes, &routed.grid, &labels);
    let parts = draw::Parts {
        nodes: placement.nodes,
        groups: placement.groups,
        titles: routed.titles,
        dividers: placement.dividers,
        routes: routed.routes,
        labels,
        end_labels,
    };
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
    use super::measure::rows;
    use super::*;
    use crate::flowchart::{self, render};
    use crate::label::Label;
    use crate::settings::LongWords;

    /// Wide enough for every flowchart here.
    const WIDTH: u16 = 400;

    fn render_with(source: &str, settings: Settings) -> Result<Drawing, Error> {
        super::render(&flowchart::read(source)?, settings)
    }

    #[test]
    fn a_drawing_at_the_area_limit_is_accepted() {
        assert_eq!(check_area(MAX_AREA / 100, 100), Ok(()));
    }

    #[test]
    fn a_drawing_over_the_area_limit_is_refused() {
        assert_eq!(check_area(MAX_AREA / 100 + 1, 100), Err(Error::TooLarge { width: MAX_AREA / 100 + 1, height: 100 }));
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
        assert_eq!(size(&one_in_a_row), (22, 21));
    }

    #[test]
    fn a_flowchart_too_dense_to_fold_is_packed_without_room_for_its_links() {
        let source = include_str!("../../corpus/flowchart/e2e--platform-dev-diagrams-layout-tests-project-sox2.mmd");
        let diagram = flowchart::read(source).unwrap();

        let folded = drawn(&diagram, layout::place(&diagram, Fit::Folded { columns: 40 }), 80);
        let packed = render(source, 80);

        assert!(matches!(folded, Err(Error::TooWide { .. })), "{:?}", folded.map(|drawing| rows(&drawing.canvas)));
        assert!(packed.is_ok());
    }

    #[test]
    fn a_flowchart_too_wide_wraps_its_labels_narrower_before_it_folds() {
        let source = include_str!("../../corpus/flowchart/e2e--platform-dev-diagrams-layout-tests-life-choices.mmd");
        let diagram = flowchart::read(source).unwrap();

        let wide = drawn(&diagram, layout::place(&diagram, Fit::Tight), 100);
        let narrowed = render(source, 100).unwrap();

        assert!(matches!(wide, Err(Error::TooWide { .. })));
        assert_eq!(narrowed.wrap.columns, NARROWEST);
        assert!(rows(&narrowed.canvas).len() < 60 && flowchart::measure(source, 100).unwrap().crossings < 2);
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
        let source = "flowchart LR\n  A[One-word-too-long-to-wrap] --> B[A long second label]";

        assert!(matches!(render(source, 20), Err(Error::TooWide { max_width: 20, .. })));
    }

    #[test]
    fn the_texts_at_an_edge_s_ends_stand_beside_it_next_to_each_node() {
        let mut diagram = flowchart::read("flowchart TB\n  A[Customer] --> B[Ticket]").unwrap();
        diagram.edges[0].start_label = Some(Label::single_line("1"));
        diagram.edges[0].end_label = Some(Label::single_line("0..*"));

        let drawing = super::render(&diagram, Settings::new(WIDTH)).unwrap();

        assert_eq!(
            rows(&drawing.canvas),
            [
                "┌──────────┐",
                "│ Customer │",
                "└────┬─────┘",
                "     │ 1    ",
                "     │      ",
                "     ▾ 0..* ",
                " ┌────────┐ ",
                " │ Ticket │ ",
                " └────────┘ ",
            ]
        );
    }
}
