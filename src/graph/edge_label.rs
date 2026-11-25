//! Where each edge's label goes: on the longest straight run of its path that has room, else beside it, at the
//! middle of the run before anywhere along it.

use vt_canvas::{Direction, Point, Rect};

use super::diagram::Diagram;
use super::route::{Grid, Route, Use, step_direction};
use crate::label::Label;

/// Cells along an end's straight run its text may stand beside, from the cell next to the node.
const NEAR_AN_END: usize = 4;

/// Where an edge's texts at its ends go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct EndLabels {
    pub(super) start: Option<Rect>,
    pub(super) end: Option<Rect>,
}

/// A label's top-left cell for each edge, `None` for an edge without a label or without room for it.
///
/// The room the layout kept for a label is free for that label only, until it is placed.
pub(super) fn place(diagram: &Diagram, routes: &[Route], grid: &Grid, kept: &[Option<Rect>]) -> Vec<Option<Rect>> {
    let mut grid = grid.clone();
    diagram
        .edges
        .iter()
        .zip(routes)
        .enumerate()
        .map(|(index, (edge, route))| {
            let width = edge.label.as_ref()?.width;
            grid.release(index);
            let rect = spot(&grid, route, width, kept[index])?;
            for x in rect.x..rect.x + rect.width {
                grid.set(Point::new(x, rect.y), Use::Blocked);
            }
            Some(rect)
        })
        .collect()
}

/// Each edge's texts at its ends, beside the straight run leaving each end, as near the node as there is room and
/// clear of the labels `placed` already; `None` where there is none.
pub(super) fn place_ends(diagram: &Diagram, routes: &[Route], grid: &Grid, placed: &[Option<Rect>]) -> Vec<EndLabels> {
    let mut grid = grid.clone();
    for rect in placed.iter().flatten() {
        block(&mut grid, *rect);
    }
    diagram
        .edges
        .iter()
        .zip(routes)
        .map(|(edge, route)| {
            let leaving: Vec<Point> = route.cells.iter().skip(1).copied().collect();
            let arriving: Vec<Point> = route.cells.iter().rev().copied().collect();
            let mut spot_beside = |label: Option<&Label>, run: &[Point]| {
                let rect = beside_end(&grid, run, label?.width)?;
                block(&mut grid, rect);
                Some(rect)
            };
            EndLabels { start: spot_beside(edge.start_label.as_ref(), &leaving), end: spot_beside(edge.end_label.as_ref(), &arriving) }
        })
        .collect()
}

/// Beside the straight run starting at `run`'s first cell, the cell next to the node: right of a vertical run, else
/// left; above a horizontal run, else below, reaching towards the node.
fn beside_end(grid: &Grid, run: &[Point], width: u16) -> Option<Rect> {
    let towards_node = step_direction(*run.get(1)?, run[0]);
    let straight = run.windows(2).take_while(|pair| step_direction(pair[1], pair[0]) == towards_node).count() + 1;
    let spots = run.iter().take(straight.min(NEAR_AN_END)).flat_map(|&cell| match towards_node {
        Direction::Up | Direction::Down => [Some((cell.x + 2, cell.y)), cell.x.checked_sub(width + 1).map(|x| (x, cell.y))],
        Direction::Right => {
            let x = (cell.x + 1).checked_sub(width);
            [x.zip(cell.y.checked_sub(1)), x.map(|x| (x, cell.y + 1))]
        }
        Direction::Left => [cell.y.checked_sub(1).map(|y| (cell.x, y)), Some((cell.x, cell.y + 1))],
    });
    spots
        .flatten()
        .map(|(x, y)| Rect::new(x, y, width, 1))
        .find(|rect| padded(*rect).is_some_and(|padded| cells(padded).all(|cell| grid.get(cell) == Use::Free)))
}

fn block(grid: &mut Grid, rect: Rect) {
    for cell in cells(rect) {
        grid.set(cell, Use::Blocked);
    }
}

/// The run through the room `kept` for the label comes first, then the longest.
fn spot(grid: &Grid, route: &Route, width: u16, kept: Option<Rect>) -> Option<Rect> {
    let mut runs: Vec<Run> = route.corners().windows(2).map(|ends| Run::new(ends[0], ends[1])).collect();
    runs.sort_by_key(|run| std::cmp::Reverse((kept.is_some_and(|kept| run.passes(kept)), run.inner)));
    let middle = || runs.iter().map(|run| (run, 0));
    let slid = || runs.iter().flat_map(|run| run.slides().map(move |slide| (run, slide)));
    let on = |(run, slide): (&Run, i32)| run.on(grid, width, slide);
    let beside = |(run, slide): (&Run, i32)| run.beside(grid, width, slide);
    middle().find_map(on).or_else(|| middle().find_map(beside)).or_else(|| slid().find_map(on)).or_else(|| slid().find_map(beside))
}

/// A straight piece of a path, its ends left out.
#[derive(Debug, Clone, Copy)]
struct Run {
    from: Point,
    horizontal: bool,
    /// Cells between its ends.
    inner: u16,
}

impl Run {
    fn new(start: Point, end: Point) -> Self {
        let horizontal = start.y == end.y;
        let from = Point::new(start.x.min(end.x), start.y.min(end.y));
        let length = if horizontal { start.x.abs_diff(end.x) } else { start.y.abs_diff(end.y) };
        Self { from, horizontal, inner: length.saturating_sub(1) }
    }

    /// Whether the run goes through `rect`, its ends left out.
    fn passes(self, rect: Rect) -> bool {
        let (x, y) = (self.from.x, self.from.y);
        if self.horizontal {
            y == rect.y && x < rect.x + rect.width && rect.x < x + self.inner + 1
        } else {
            (rect.x..rect.x + rect.width).contains(&x) && (y + 1..=y + self.inner).contains(&rect.y)
        }
    }

    /// Cells a label may move along the run from its middle, the nearest first, each way in turn.
    fn slides(self) -> impl Iterator<Item = i32> {
        (1..=i32::from(self.inner)).flat_map(|cells| [cells, -cells])
    }

    /// Across a horizontal run, with a cell left on each side; across a vertical run, in its lower half, centred on
    /// the line when there is room, else moved sideways as long as it still covers the line; `slide` cells along the
    /// run from there.
    fn on(self, grid: &Grid, width: u16, slide: i32) -> Option<Rect> {
        if self.horizontal {
            let room = self.inner.checked_sub(width + 2)?;
            let rect = Rect::new(self.from.x + 2 + slid(room / 2, slide, 0..=room)?, self.from.y, width, 1);
            // In the middle the run's ends keep the label clear; slid, it could touch a crossing.
            let clear = if slide == 0 { Some(rect) } else { padded(rect) };
            return clear.filter(|clear| cells(*clear).all(|cell| grid.get(cell) == Use::Line { horizontal: true })).map(|_| rect);
        }
        let last = self.inner.checked_sub(1)?;
        let line = Point::new(self.from.x, self.from.y + 1 + slid(self.inner / 2, slide, 0..=last)?);
        if grid.get(line) != (Use::Line { horizontal: false }) {
            return None;
        }
        let centred = line.x.saturating_sub(width / 2);
        let mut starts: Vec<u16> = ((line.x + 1).saturating_sub(width)..=line.x).collect();
        starts.sort_by_key(|start| start.abs_diff(centred));
        starts
            .into_iter()
            .map(|start| Rect::new(start, line.y, width, 1))
            .find(|rect| padded(*rect).is_some_and(|padded| cells(padded).all(|cell| cell == line || grid.get(cell) == Use::Free)))
    }

    /// Next to the run, `slide` cells along it from its middle: right of a vertical run, else left; above a
    /// horizontal run, else below.
    fn beside(self, grid: &Grid, width: u16, slide: i32) -> Option<Rect> {
        let along = slid(1 + self.inner / 2, slide, 1..=self.inner.max(1))?;
        let spots = if self.horizontal {
            let x = (self.from.x + along).checked_sub(width / 2);
            [x.zip(self.from.y.checked_sub(1)), x.map(|x| (x, self.from.y + 1))]
        } else {
            let y = self.from.y + along;
            [Some((self.from.x + 2, y)), self.from.x.checked_sub(width + 1).map(|x| (x, y))]
        };
        spots
            .into_iter()
            .flatten()
            .map(|(x, y)| Rect::new(x, y, width, 1))
            .find(|rect| padded(*rect).is_some_and(|padded| cells(padded).all(|cell| grid.get(cell) == Use::Free)))
    }
}

/// `middle` moved by `slide` cells, when that stays `within` the run.
fn slid(middle: u16, slide: i32, within: std::ops::RangeInclusive<u16>) -> Option<u16> {
    u16::try_from(i32::from(middle) + slide).ok().filter(|moved| within.contains(moved))
}

/// `rect` and one cell more on its left and right, so a label never touches another drawing.
fn padded(rect: Rect) -> Option<Rect> {
    Some(Rect::new(rect.x.checked_sub(1)?, rect.y, rect.width + 2, 1))
}

fn cells(rect: Rect) -> impl Iterator<Item = Point> {
    (rect.x..rect.x + rect.width).map(move |x| Point::new(x, rect.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid with one straight line from `from` to `to`, both ends blocked as a route leaves them.
    fn grid_with_run(from: Point, to: Point) -> (Grid, Route) {
        let mut grid = Grid { width: 30, height: 12, cells: vec![Use::Free; 30 * 12] };
        let horizontal = from.y == to.y;
        let cells: Vec<Point> = if horizontal {
            (from.x..=to.x).map(|x| Point::new(x, from.y)).collect()
        } else {
            (from.y..=to.y).map(|y| Point::new(from.x, y)).collect()
        };
        for cell in &cells {
            grid.set(*cell, Use::Line { horizontal });
        }
        grid.set(from, Use::Blocked);
        grid.set(to, Use::Blocked);
        (grid, Route { cells, end: to })
    }

    #[test]
    fn a_label_goes_across_a_long_enough_horizontal_run() {
        let (grid, route) = grid_with_run(Point::new(0, 5), Point::new(12, 5));

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(5, 5, 3, 1)));
    }

    #[test]
    fn a_label_goes_across_a_vertical_run_in_its_lower_half() {
        let (grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 4));

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(9, 2, 3, 1)));
    }

    #[test]
    fn a_label_slides_sideways_to_keep_clear_of_a_neighbour() {
        let (mut grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 4));
        grid.set(Point::new(7, 2), Use::Blocked);

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(9, 2, 3, 1)));
        assert_eq!(spot(&grid, &route, 5, None), Some(Rect::new(9, 2, 5, 1)));
    }

    #[test]
    fn a_label_too_wide_for_a_horizontal_run_goes_above_it() {
        let (grid, route) = grid_with_run(Point::new(4, 5), Point::new(10, 5));

        assert_eq!(spot(&grid, &route, 6, None), Some(Rect::new(4, 4, 6, 1)));
    }

    #[test]
    fn a_label_slides_along_a_horizontal_run_past_a_crossing_in_its_middle() {
        let (mut grid, route) = grid_with_run(Point::new(0, 5), Point::new(12, 5));
        grid.set(Point::new(6, 5), Use::Blocked);
        for y in [4, 6] {
            for x in 0..30 {
                grid.set(Point::new(x, y), Use::Blocked);
            }
        }

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(8, 5, 3, 1)));
    }

    #[test]
    fn a_label_slides_along_a_vertical_run_to_the_nearest_row_with_room() {
        let (mut grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 8));
        for y in [3, 4, 5] {
            for x in [8, 12] {
                grid.set(Point::new(x, y), Use::Blocked);
            }
        }

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(9, 6, 3, 1)));
    }

    #[test]
    fn a_label_goes_on_the_run_through_the_room_kept_for_it_before_a_longer_one() {
        let (mut grid, down) = grid_with_run(Point::new(10, 5), Point::new(10, 8));
        let (_, across) = grid_with_run(Point::new(10, 8), Point::new(25, 8));
        for cell in &across.cells[1..] {
            grid.set(*cell, Use::Line { horizontal: true });
        }
        let route = Route { cells: down.cells.iter().chain(&across.cells[1..]).copied().collect(), end: across.end };

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(16, 8, 3, 1)));
        assert_eq!(spot(&grid, &route, 3, Some(Rect::new(9, 7, 3, 1))), Some(Rect::new(9, 7, 3, 1)));
    }

    #[test]
    fn room_kept_for_another_label_is_not_free_until_released() {
        let (mut grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 4));
        grid.set(Point::new(9, 2), Use::Kept { edge: 7, horizontal: false });

        let mut released = grid.clone();
        released.release(7);

        assert_eq!(spot(&grid, &route, 3, None), Some(Rect::new(12, 2, 3, 1)));
        assert_eq!(spot(&released, &route, 3, None), Some(Rect::new(9, 2, 3, 1)));
    }

    #[test]
    fn a_label_goes_beside_a_run_with_no_cell_between_its_ends() {
        let (grid, route) = grid_with_run(Point::new(4, 5), Point::new(5, 5));

        assert_eq!(spot(&grid, &route, 2, None), Some(Rect::new(4, 4, 2, 1)));
    }

    #[test]
    fn a_label_with_no_free_cells_around_its_run_finds_no_room() {
        let (mut grid, route) = grid_with_run(Point::new(0, 5), Point::new(6, 5));
        for y in [4, 6] {
            for x in 0..30 {
                grid.set(Point::new(x, y), Use::Blocked);
            }
        }

        assert_eq!(spot(&grid, &route, 6, None), None);
    }
}
