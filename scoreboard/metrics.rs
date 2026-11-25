//! Metrics read from a tool's text output, the same way for every tool, and vt-mermaid's exact ones.

use serde::{Deserialize, Serialize};

use crate::expected::{Expected, Kind, without_markup};
use crate::lines::{self, BOTTOM_LEFT, BOTTOM_RIGHT, CROSSINGS, Lines, TOP_LEFT, TOP_RIGHT};
use crate::tool::{Run, Status};

/// How one tool did on one input at one width.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Metrics {
    pub(crate) status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) drawn: Option<Drawn>,
    /// 0 to 100; none when the oracle has no node to check the drawing against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) score: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) exact: Option<Exact>,
}

/// What the text of a drawing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Drawn {
    pub(crate) width: usize,
    pub(crate) rows: usize,
    pub(crate) area: usize,
    pub(crate) fits: bool,
    pub(crate) node_labels: usize,
    pub(crate) edge_labels: usize,
    pub(crate) arrows: usize,
    pub(crate) crossings: usize,
    pub(crate) bends: usize,
}

/// vt-mermaid's own count of its drawing, from `vt_mermaid::measure`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Exact {
    pub(crate) crossings: usize,
    pub(crate) bends: usize,
    pub(crate) edge_cells: usize,
    #[serde(default)]
    pub(crate) beside_nodes: usize,
    pub(crate) violations: Vec<String>,
}

impl Metrics {
    pub(crate) fn of(run: &Run, max_width: u16, expected: Option<&Expected>, exact: Option<Exact>, kind: Kind) -> Self {
        let drawn = (run.status == Status::Rendered).then(|| Drawn::of(&run.text, max_width, expected, kind));
        let reason = (run.status != Status::Rendered).then(|| reason(&run.text));
        let score =
            expected.filter(|expected| !expected.node_labels.is_empty()).map(|expected| drawn.map_or(0, |drawn| drawn.score(expected)));
        Self { status: run.status, reason, drawn, score, exact }
    }
}

impl Drawn {
    fn of(text: &str, max_width: u16, expected: Option<&Expected>, kind: Kind) -> Self {
        let heads = kind.heads();
        let rows = trimmed_rows(text);
        let grid: Vec<Vec<char>> = rows.iter().map(|row| row.chars().collect()).collect();
        let width = rows.iter().map(|row| vt_canvas::width(row)).max().unwrap_or(0);
        let all = rows.join("\n");
        let shown = without_markup(&all);
        let found = |labels: &[Vec<String>]| labels.iter().filter(|words| words.iter().all(|word| shown.contains(word.as_str()))).count();
        let boxes = lines::boxes(&grid);
        let lines = Lines::of(&grid, &boxes, heads);
        let corners = count(&all, TOP_LEFT) + count(&all, TOP_RIGHT) + count(&all, BOTTOM_LEFT) + count(&all, BOTTOM_RIGHT);
        Self {
            width,
            rows: rows.len(),
            area: width * rows.len(),
            fits: width <= usize::from(max_width),
            node_labels: expected.map_or(0, |expected| found(&expected.node_labels)),
            edge_labels: expected.map_or(0, |expected| found(&expected.edge_labels)),
            arrows: count(&all, heads) + lines.shared_arrows,
            crossings: if kind.crossings_joined_all_round() {
                lines::joined_crossings(&grid)
            } else {
                count(&all, CROSSINGS).saturating_sub(lines.trunk_crossings)
            },
            bends: corners.saturating_sub(4 * boxes.len()),
        }
    }

    /// Half for labels, a fifth for fitting, the rest for arrowheads and crossings.
    fn score(self, expected: &Expected) -> u32 {
        let labels_expected = expected.node_labels.len() + expected.edge_labels.len();
        let labels = if labels_expected == 0 { 1.0 } else { ratio(self.node_labels + self.edge_labels, labels_expected) };
        let arrows = 1.0 - ratio(self.arrows.abs_diff(expected.arrows).min(expected.arrows.max(1)), expected.arrows.max(1));
        let crossings = 1.0 / (1.0 + ratio(self.crossings, 1));
        let fits = if self.fits { 1.0 } else { 0.0 };
        (50.0 * labels + 20.0 * fits + 15.0 * arrows + 15.0 * crossings).round() as u32
    }
}

fn ratio(part: usize, whole: usize) -> f64 {
    float(part) / float(whole)
}

/// Counts on the board stay far below `u32::MAX`, so the conversion is exact.
pub(crate) fn float(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// The error's first 120 characters, without the timestamp mermaid-ascii logs, so a rerun leaves the file as it was.
fn reason(error: &str) -> String {
    error.split(' ').filter(|word| !word.starts_with("time=\"")).collect::<Vec<_>>().join(" ").chars().take(120).collect()
}

/// Rows without trailing spaces, and without the blank rows above and below the drawing.
fn trimmed_rows(text: &str) -> Vec<&str> {
    let rows: Vec<&str> = text.lines().map(str::trim_end).collect();
    let first = rows.iter().position(|row| !row.is_empty()).unwrap_or(rows.len());
    let last = rows.iter().rposition(|row| !row.is_empty()).map_or(first, |last| last + 1);
    rows[first..last].to_vec()
}

fn count(text: &str, glyphs: &str) -> usize {
    text.chars().filter(|char| glyphs.contains(*char)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected(nodes: &[&str], arrows: usize) -> Expected {
        Expected { node_labels: nodes.iter().map(|label| vec![(*label).to_owned()]).collect(), edge_labels: Vec::new(), arrows }
    }

    #[test]
    fn a_box_corners_are_not_bends_but_an_edge_corner_is() {
        let text = "┌───┐\n│ A ├─┐\n└───┘ │\n      ▾\n    ┌───┐\n    │ B │\n    └───┘\n";

        let drawn = Drawn::of(text, 80, Some(&expected(&["A", "B"], 1)), Kind::Flowchart);

        assert_eq!((drawn.bends, drawn.arrows, drawn.node_labels, drawn.rows, drawn.width), (1, 1, 2, 7, 9));
    }

    #[test]
    fn edges_merged_into_one_line_count_an_arrow_each_and_their_trunk_is_no_crossing() {
        let text = "┌───┐\n│ A ├───┐\n└───┘   │\n┌───┐   │  ┌───┐\n│ B ├───┼─▸│ D │\n└───┘   │  └───┘\n┌───┐   │\n│ C ├───┘\n└───┘";

        let drawn = Drawn::of(text, 80, Some(&expected(&["A", "B", "C", "D"], 3)), Kind::Flowchart);

        assert_eq!((drawn.arrows, drawn.crossings), (3, 0));
    }

    #[test]
    fn a_label_is_found_whatever_markup_the_drawing_keeps() {
        let expected = expected(&["Markdown:", "bolder", "two"], 0);

        assert_eq!(Drawn::of("**Markdown**: <b>bold</b>er one<br/>two", 80, Some(&expected), Kind::Flowchart).node_labels, 3);
    }

    #[test]
    fn a_reason_loses_its_timestamp() {
        assert_eq!(reason("time=\"2026-11-25T17:37:11+01:00\" level=fatal msg=\"no\""), "level=fatal msg=\"no\"");
    }

    #[test]
    fn a_drawing_wider_than_allowed_does_not_fit() {
        assert!(!Drawn::of("A ──▸ B", 6, None, Kind::Flowchart).fits);
    }

    #[test]
    fn a_drawing_with_every_label_and_arrow_and_no_crossing_scores_full_marks() {
        let expected = expected(&["A", "B"], 1);

        assert_eq!(Drawn::of("A ──▸ B", 80, Some(&expected), Kind::Flowchart).score(&expected), 100);
        assert_eq!(Drawn::of("A ──┼─ C", 80, Some(&expected), Kind::Flowchart).score(&expected), 53);
    }
}
