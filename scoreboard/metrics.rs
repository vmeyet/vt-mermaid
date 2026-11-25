//! Metrics read from a tool's text output, the same way for every tool, and vt-mermaid's exact ones.

use serde::{Deserialize, Serialize};

use crate::expected::Expected;
use crate::tool::{Run, Status};

const CROSSINGS: &str = "┼╋╪╫╬┿╀╁╂╃╄╅╆╇╈╉╊";
const TOP_LEFT: &str = "┌╭┏╔";
const TOP_RIGHT: &str = "┐╮┓╗";
const BOTTOM_LEFT: &str = "└╰┗╚";
const BOTTOM_RIGHT: &str = "┘╯┛╝";
const VERTICAL: &str = "│┃║╎╏┆┇┊┋├┤┝┥┠┨┣┫╞╡╟╢╠╣┼╋╪╫╬";
const ARROWS: &str = "►▶▸▷→◄◀◂◁←▲▴△↑▼▾▽↓";

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
    pub(crate) violations: Vec<String>,
}

impl Metrics {
    pub(crate) fn of(run: &Run, max_width: u16, expected: Option<&Expected>, exact: Option<Exact>) -> Self {
        let drawn = (run.status == Status::Rendered).then(|| Drawn::of(&run.text, max_width, expected));
        let reason = (run.status != Status::Rendered).then(|| reason(&run.text));
        let score =
            expected.filter(|expected| !expected.node_labels.is_empty()).map(|expected| drawn.map_or(0, |drawn| drawn.score(expected)));
        Self { status: run.status, reason, drawn, score, exact }
    }
}

impl Drawn {
    fn of(text: &str, max_width: u16, expected: Option<&Expected>) -> Self {
        let rows = trimmed_rows(text);
        let grid: Vec<Vec<char>> = rows.iter().map(|row| row.chars().collect()).collect();
        let width = rows.iter().map(|row| vt_canvas::width(row)).max().unwrap_or(0);
        let all = rows.join("\n");
        let found = |labels: &[Vec<String>]| labels.iter().filter(|words| words.iter().all(|word| all.contains(word.as_str()))).count();
        let corners = count(&all, TOP_LEFT) + count(&all, TOP_RIGHT) + count(&all, BOTTOM_LEFT) + count(&all, BOTTOM_RIGHT);
        Self {
            width,
            rows: rows.len(),
            area: width * rows.len(),
            fits: width <= usize::from(max_width),
            node_labels: expected.map_or(0, |expected| found(&expected.node_labels)),
            edge_labels: expected.map_or(0, |expected| found(&expected.edge_labels)),
            arrows: count(&all, ARROWS),
            crossings: count(&all, CROSSINGS),
            bends: corners.saturating_sub(4 * boxes(&grid)),
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

/// Closed rectangles: a top-left corner whose row reaches a top-right corner, with sides down to the two bottom corners.
/// Their corners are frames, not edges bending, so they are taken out of the bend count.
fn boxes(grid: &[Vec<char>]) -> usize {
    let at = |row: usize, column: usize| grid.get(row).and_then(|cells| cells.get(column)).copied().unwrap_or(' ');
    let is = |row: usize, column: usize, glyphs: &str| glyphs.contains(at(row, column));
    let mut found = 0;
    for (top, cells) in grid.iter().enumerate() {
        let lefts = cells.iter().enumerate().filter(|(_, cell)| TOP_LEFT.contains(**cell)).map(|(left, _)| left);
        for left in lefts {
            let Some(right) = (left + 1..cells.len()).find(|&column| is(top, column, TOP_RIGHT) || is(top, column, TOP_LEFT)) else {
                continue;
            };
            if !is(top, right, TOP_RIGHT) {
                continue;
            }
            let side = |column: usize| (top + 1..grid.len()).find(|&row| !is(row, column, VERTICAL));
            if let (Some(bottom), Some(other)) = (side(left), side(right))
                && bottom == other
                && is(bottom, left, BOTTOM_LEFT)
                && is(bottom, right, BOTTOM_RIGHT)
            {
                found += 1;
            }
        }
    }
    found
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

        let drawn = Drawn::of(text, 80, Some(&expected(&["A", "B"], 1)));

        assert_eq!((drawn.bends, drawn.arrows, drawn.node_labels, drawn.rows, drawn.width), (1, 1, 2, 7, 9));
    }

    #[test]
    fn a_reason_loses_its_timestamp() {
        assert_eq!(reason("time=\"2026-11-25T17:37:11+01:00\" level=fatal msg=\"no\""), "level=fatal msg=\"no\"");
    }

    #[test]
    fn a_drawing_wider_than_allowed_does_not_fit() {
        assert!(!Drawn::of("A ──▸ B", 6, None).fits);
    }

    #[test]
    fn a_drawing_with_every_label_and_arrow_and_no_crossing_scores_full_marks() {
        let expected = expected(&["A", "B"], 1);

        assert_eq!(Drawn::of("A ──▸ B", 80, Some(&expected)).score(&expected), 100);
        assert_eq!(Drawn::of("A ──┼─ C", 80, Some(&expected)).score(&expected), 53);
    }
}
