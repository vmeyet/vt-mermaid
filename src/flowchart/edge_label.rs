//! Where each edge's label goes: on the longest straight run of its path that has room, else beside it.

use vt_canvas::{Point, Rect};

use super::diagram::Diagram;
use super::route::{Grid, Route, Use};

/// A label's top-left cell for each edge, `None` for an edge without a label or without room for it.
pub(super) fn place(diagram: &Diagram, routes: &[Route], grid: &Grid) -> Vec<Option<Rect>> {
    let mut grid = grid.clone();
    diagram
        .edges
        .iter()
        .zip(routes)
        .map(|(edge, route)| {
            let width = edge.label.as_ref()?.width;
            let rect = spot(&grid, route, width)?;
            for x in rect.x..rect.x + rect.width {
                grid.set(Point::new(x, rect.y), Use::Blocked);
            }
            Some(rect)
        })
        .collect()
}

fn spot(grid: &Grid, route: &Route, width: u16) -> Option<Rect> {
    let mut runs: Vec<Run> = route.corners().windows(2).map(|ends| Run::new(ends[0], ends[1])).collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.inner));
    let on = runs.iter().find_map(|run| run.on(grid, width));
    on.or_else(|| runs.iter().find_map(|run| run.beside(grid, width)))
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

    /// Across a horizontal run, with a line cell left on each side; across a vertical run, in its lower half, centred
    /// on the line when there is room, else slid sideways as long as it still covers the line.
    fn on(self, grid: &Grid, width: u16) -> Option<Rect> {
        if self.horizontal {
            let room = self.inner.checked_sub(width + 2)?;
            let rect = Rect::new(self.from.x + 2 + room / 2, self.from.y, width, 1);
            return cells(rect).all(|cell| grid.get(cell) == Use::Line { horizontal: true }).then_some(rect);
        }
        let line = Point::new(self.from.x, self.from.y + 1 + self.inner / 2);
        if self.inner < 2 || grid.get(line) != (Use::Line { horizontal: false }) {
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

    /// Next to the middle of the run: right of a vertical run, else left; above a horizontal run, else below.
    fn beside(self, grid: &Grid, width: u16) -> Option<Rect> {
        let middle = 1 + self.inner / 2;
        let spots = if self.horizontal {
            let x = (self.from.x + middle).checked_sub(width / 2);
            [x.zip(self.from.y.checked_sub(1)), x.map(|x| (x, self.from.y + 1))]
        } else {
            let y = self.from.y + middle;
            [Some((self.from.x + 2, y)), self.from.x.checked_sub(width + 1).map(|x| (x, y))]
        };
        spots
            .into_iter()
            .flatten()
            .map(|(x, y)| Rect::new(x, y, width, 1))
            .find(|rect| padded(*rect).is_some_and(|padded| cells(padded).all(|cell| grid.get(cell) == Use::Free)))
    }
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

        assert_eq!(spot(&grid, &route, 3), Some(Rect::new(5, 5, 3, 1)));
    }

    #[test]
    fn a_label_goes_across_a_vertical_run_in_its_lower_half() {
        let (grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 4));

        assert_eq!(spot(&grid, &route, 3), Some(Rect::new(9, 2, 3, 1)));
    }

    #[test]
    fn a_label_slides_sideways_to_keep_clear_of_a_neighbour() {
        let (mut grid, route) = grid_with_run(Point::new(10, 0), Point::new(10, 4));
        grid.set(Point::new(7, 2), Use::Blocked);

        assert_eq!(spot(&grid, &route, 3), Some(Rect::new(9, 2, 3, 1)));
        assert_eq!(spot(&grid, &route, 5), Some(Rect::new(9, 2, 5, 1)));
    }

    #[test]
    fn a_label_too_wide_for_a_horizontal_run_goes_above_it() {
        let (grid, route) = grid_with_run(Point::new(4, 5), Point::new(10, 5));

        assert_eq!(spot(&grid, &route, 6), Some(Rect::new(4, 4, 6, 1)));
    }

    #[test]
    fn a_label_with_no_free_cells_around_its_run_finds_no_room() {
        let (mut grid, route) = grid_with_run(Point::new(0, 5), Point::new(6, 5));
        for y in [4, 6] {
            for x in 0..30 {
                grid.set(Point::new(x, y), Use::Blocked);
            }
        }

        assert_eq!(spot(&grid, &route, 6), None);
    }
}
