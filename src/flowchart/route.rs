//! Edges as right-angled paths on the grid: a cheapest-path search that pays for each bend and crossing.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use vt_canvas::{Direction, Point, Rect};

use super::diagram::{Diagram, End};
use super::snap::Placement;
use crate::error::Error;

const STEP: u32 = 1;
const BEND: u32 = 4;
const CROSSING: u32 = 6;
/// Leaving or entering a node through a side other than the flow's.
const OFF_FLOW_SIDE: u32 = 4;
const UNREACHED: u32 = u32::MAX;
const HEADINGS: [Direction; 4] = [Direction::Up, Direction::Right, Direction::Down, Direction::Left];

/// An edge's cells, from the border cell it leaves its source through to the cell its arrow goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Route {
    pub(super) cells: Vec<Point>,
    /// The target's border cell next to the last cell.
    pub(super) end: Point,
}

impl Route {
    /// The direction the arrow points: from the last cell into the target.
    pub(super) fn arrival(&self) -> Direction {
        step_direction(self.cells.last().copied().unwrap_or(self.end), self.end)
    }

    /// The direction the edge leaves its source in.
    pub(super) fn departure(&self) -> Direction {
        match self.cells.as_slice() {
            [first, second, ..] => step_direction(*first, *second),
            _ => self.arrival(),
        }
    }

    /// The corners of the path: its ends and every cell where it turns.
    pub(super) fn corners(&self) -> Vec<Point> {
        let first = self.cells.first().copied();
        let last = self.cells.last().copied().filter(|_| self.cells.len() > 1);
        first.into_iter().chain(self.turns()).chain(last).collect()
    }

    /// How many times the path turns.
    #[cfg(any(test, feature = "measure"))]
    pub(super) fn bends(&self) -> usize {
        self.turns().count()
    }

    fn turns(&self) -> impl Iterator<Item = Point> + '_ {
        self.cells.windows(3).filter(|cells| step_direction(cells[0], cells[1]) != step_direction(cells[1], cells[2])).map(|cells| cells[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Use {
    Free,
    /// A node, a frame corner or title, an edge's turn, arrow or crossing.
    Blocked,
    /// One straight line runs through, horizontally or not; another may cross it at a right angle.
    Line {
        horizontal: bool,
    },
    /// A node's border cell: edges join it, never pass it.
    Border,
}

/// What each cell holds, for the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Grid {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) cells: Vec<Use>,
}

impl Grid {
    fn new(diagram: &Diagram, placement: &Placement) -> Self {
        let mut grid = Self::empty(placement.size.width, placement.size.height);
        for (group, rect) in diagram.groups.iter().zip(&placement.groups) {
            grid.frame(*rect, group.title.width);
        }
        for rect in &placement.nodes {
            grid.node(*rect);
        }
        grid
    }

    fn empty(width: u16, height: u16) -> Self {
        let (width, height) = (usize::from(width), usize::from(height));
        Self { width, height, cells: vec![Use::Free; width * height] }
    }

    pub(super) fn get(&self, at: Point) -> Use {
        self.index(at).map_or(Use::Blocked, |index| self.cells[index])
    }

    pub(super) fn set(&mut self, at: Point, cell: Use) {
        if let Some(index) = self.index(at) {
            self.cells[index] = cell;
        }
    }

    fn index(&self, at: Point) -> Option<usize> {
        let (x, y) = (usize::from(at.x), usize::from(at.y));
        (x < self.width && y < self.height).then_some(y * self.width + x)
    }

    fn fill(&mut self, rect: Rect, cell: Use) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                self.set(Point::new(x, y), cell);
            }
        }
    }

    fn node(&mut self, rect: Rect) {
        self.fill(rect, Use::Border);
        self.fill(Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2)), Use::Blocked);
    }

    /// A frame's sides may be crossed, its corners and title may not.
    fn frame(&mut self, rect: Rect, title_width: u16) {
        let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
        for x in rect.x..=right {
            self.set(Point::new(x, rect.y), Use::Line { horizontal: true });
            self.set(Point::new(x, bottom), Use::Line { horizontal: true });
        }
        for y in rect.y..=bottom {
            self.set(Point::new(rect.x, y), Use::Line { horizontal: false });
            self.set(Point::new(right, y), Use::Line { horizontal: false });
        }
        for corner in [Point::new(rect.x, rect.y), Point::new(right, rect.y), Point::new(rect.x, bottom), Point::new(right, bottom)] {
            self.set(corner, Use::Blocked);
        }
        self.fill(Rect::new(rect.x + 1, rect.y, title_width + 4, 1), Use::Blocked);
    }

    /// Lays `route` on the grid so later routes only cross it at a right angle, away from its turns and ends.
    fn add(&mut self, route: &Route) {
        let last = route.cells.len() - 1;
        for (index, &cell) in route.cells.iter().enumerate().skip(1) {
            let horizontal = route.cells[index - 1].y == cell.y;
            let turns = index < last && (route.cells[index + 1].y == cell.y) != horizontal;
            let next = match self.get(cell) {
                Use::Free if !turns && index < last => Use::Line { horizontal },
                _ => Use::Blocked,
            };
            self.set(cell, next);
        }
        for end in route.cells.first().into_iter().chain([&route.end]) {
            self.set(*end, Use::Blocked);
        }
    }
}

/// Routes every edge, shortest first, so long edges go round what short ones already drew.
///
/// Returns the routes in the edges' order and the grid they leave, for the labels.
pub(super) fn route(diagram: &Diagram, placement: &Placement) -> Result<(Vec<Route>, Grid), Error> {
    let mut grid = Grid::new(diagram, placement);
    let rect = |end: End| match end {
        End::Node(index) => placement.nodes[index],
        End::Group(index) => placement.groups[index],
    };
    let mut order: Vec<usize> = (0..diagram.edges.len()).collect();
    order.sort_by_key(|&index| distance(rect(diagram.edges[index].from), rect(diagram.edges[index].to)));
    let mut routes: Vec<Option<Route>> = vec![None; diagram.edges.len()];
    for index in order {
        let edge = &diagram.edges[index];
        let search = Search {
            grid: &grid,
            from: rect(edge.from),
            to: rect(edge.to),
            exit: diagram.flow_around(edge.from),
            entry: diagram.flow_around(edge.to),
        };
        let route = search
            .run()
            .ok_or_else(|| Error::Unroutable { from: diagram.name(edge.from).to_owned(), to: diagram.name(edge.to).to_owned() })?;
        grid.add(&route);
        routes[index] = Some(route);
    }
    Ok((routes.into_iter().flatten().collect(), grid))
}

/// One edge's search: from a border cell of `from`, out and round, into a border cell of `to`.
struct Search<'a> {
    grid: &'a Grid,
    from: Rect,
    to: Rect,
    /// The preferred way out of `from` and into `to`.
    exit: Direction,
    entry: Direction,
}

impl Search<'_> {
    fn run(&self) -> Option<Route> {
        let states = self.grid.width * self.grid.height * 4;
        let mut cost = vec![UNREACHED; states];
        let mut previous = vec![usize::MAX; states];
        let mut queue = BinaryHeap::new();
        let mut best: Option<(u32, usize, Point)> = None;
        for (cell, outward) in self.ports_of(self.from) {
            let state = self.state(cell, outward);
            cost[state] = side_cost(outward, self.exit) + off_centre(self.from, cell);
            queue.push(Reverse((cost[state] + self.estimate(cell), state)));
        }
        while let Some(Reverse((estimate, state))) = queue.pop() {
            if best.is_some_and(|(total, ..)| estimate >= total) {
                break;
            }
            let (cell, heading) = self.cell_of(state);
            for direction in HEADINGS {
                let Some(next) = neighbour(cell, direction).filter(|_| direction != opposite(heading)) else { continue };
                if let Some(penalty) = self.arrival(cell, heading, next, direction) {
                    let total = cost[state] + STEP + penalty;
                    if best.is_none_or(|(known, ..)| total < known) {
                        best = Some((total, state, next));
                    }
                    continue;
                }
                let Some(step) = self.step_cost(cell, heading, next, direction) else { continue };
                let next_state = self.state(next, direction);
                let next_cost = cost[state] + step;
                if next_cost < cost[next_state] {
                    cost[next_state] = next_cost;
                    previous[next_state] = state;
                    queue.push(Reverse((next_cost + self.estimate(next), next_state)));
                }
            }
        }
        best.map(|(_, state, end)| Route { cells: self.path(&previous, state), end })
    }

    /// What arriving costs when `next` is a free border cell of the target, entered from a free cell.
    ///
    /// The arrow goes on that cell: a line may turn there into `▾` or `▴` (`──▾` reads well), never into `▸` or `◂`,
    /// which a vertical line beside them would hide.
    fn arrival(&self, cell: Point, heading: Direction, next: Point, direction: Direction) -> Option<u32> {
        let inward = side(self.to, next) == Some(opposite(direction)) && self.joinable(next);
        let turn = match (direction == heading, direction) {
            (true, _) => Some(0),
            (false, Direction::Down | Direction::Up) => Some(BEND),
            (false, _) => None,
        };
        let cost = turn.filter(|_| inward && self.grid.get(cell) == Use::Free)?;
        Some(cost + side_cost(direction, self.entry) + off_centre(self.to, next))
    }

    /// What moving from `cell` to `next` costs, `None` where it may not go.
    fn step_cost(&self, cell: Point, heading: Direction, next: Point, direction: Direction) -> Option<u32> {
        let turning = direction != heading;
        if turning && self.grid.get(cell) != Use::Free {
            return None;
        }
        let horizontal = matches!(direction, Direction::Left | Direction::Right);
        let entering = match self.grid.get(next) {
            Use::Free => STEP,
            Use::Line { horizontal: line } if line != horizontal => STEP + CROSSING,
            _ => return None,
        };
        Some(entering + if turning { BEND } else { 0 })
    }

    /// The border cells of `rect` an edge may still join, each with the direction leading out.
    fn ports_of(&self, rect: Rect) -> impl Iterator<Item = (Point, Direction)> + '_ {
        let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
        let rows = (rect.x..=right).flat_map(move |x| [Point::new(x, rect.y), Point::new(x, bottom)]);
        let columns = (rect.y..=bottom).flat_map(move |y| [Point::new(rect.x, y), Point::new(right, y)]);
        rows.chain(columns).filter(|cell| self.joinable(*cell)).filter_map(move |cell| side(rect, cell).map(|outward| (cell, outward)))
    }

    /// A node's border or a frame's side no earlier edge joins.
    fn joinable(&self, cell: Point) -> bool {
        matches!(self.grid.get(cell), Use::Border | Use::Line { .. })
    }

    fn estimate(&self, cell: Point) -> u32 {
        let gap = |value: u16, start: u16, length: u16| {
            u32::from(start.saturating_sub(value)) + u32::from(value.saturating_sub(start + length - 1))
        };
        gap(cell.x, self.to.x, self.to.width) + gap(cell.y, self.to.y, self.to.height)
    }

    fn path(&self, previous: &[usize], last: usize) -> Vec<Point> {
        let mut cells: Vec<Point> = std::iter::successors(Some(last), |&state| Some(previous[state]).filter(|&state| state != usize::MAX))
            .map(|state| self.cell_of(state).0)
            .collect();
        cells.reverse();
        cells
    }

    fn state(&self, cell: Point, heading: Direction) -> usize {
        let heading = HEADINGS.iter().position(|&known| known == heading).unwrap_or(0);
        (usize::from(cell.y) * self.grid.width + usize::from(cell.x)) * 4 + heading
    }

    fn cell_of(&self, state: usize) -> (Point, Direction) {
        let index = state / 4;
        let heading = HEADINGS[state % 4];
        (Point::new(to_u16(index % self.grid.width), to_u16(index / self.grid.width)), heading)
    }
}

fn side_cost(direction: Direction, flow: Direction) -> u32 {
    if direction == flow { 0 } else { OFF_FLOW_SIDE }
}

/// Half the distance from `cell` to the middle of its side of `rect`, so edges join nodes near the middle.
fn off_centre(rect: Rect, cell: Point) -> u32 {
    let distance = |value: u16, start: u16, length: u16| (u32::from(value) * 2).abs_diff(u32::from(start) * 2 + u32::from(length) - 1);
    let offset = if cell.y == rect.y || cell.y == rect.y + rect.height - 1 {
        distance(cell.x, rect.x, rect.width)
    } else {
        distance(cell.y, rect.y, rect.height)
    };
    (offset + 2) / 4
}

/// The direction out of `rect` through `cell`, a border cell that is not a corner.
fn side(rect: Rect, cell: Point) -> Option<Direction> {
    let (right, bottom) = (rect.x + rect.width - 1, rect.y + rect.height - 1);
    let (across, down) = ((rect.x + 1..right).contains(&cell.x), (rect.y + 1..bottom).contains(&cell.y));
    if across && cell.y == rect.y {
        Some(Direction::Up)
    } else if across && cell.y == bottom {
        Some(Direction::Down)
    } else if down && cell.x == rect.x {
        Some(Direction::Left)
    } else if down && cell.x == right {
        Some(Direction::Right)
    } else {
        None
    }
}

fn neighbour(cell: Point, direction: Direction) -> Option<Point> {
    match direction {
        Direction::Up => cell.y.checked_sub(1).map(|y| Point::new(cell.x, y)),
        Direction::Right => cell.x.checked_add(1).map(|x| Point::new(x, cell.y)),
        Direction::Down => cell.y.checked_add(1).map(|y| Point::new(cell.x, y)),
        Direction::Left => cell.x.checked_sub(1).map(|x| Point::new(x, cell.y)),
    }
}

pub(super) fn step_direction(from: Point, to: Point) -> Direction {
    if to.x > from.x {
        Direction::Right
    } else if to.x < from.x {
        Direction::Left
    } else if to.y > from.y {
        Direction::Down
    } else {
        Direction::Up
    }
}

pub(super) const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::Up => Direction::Down,
        Direction::Right => Direction::Left,
        Direction::Down => Direction::Up,
        Direction::Left => Direction::Right,
    }
}

fn distance(from: Rect, to: Rect) -> u32 {
    let centre = |rect: Rect| (u32::from(rect.x) * 2 + u32::from(rect.width), u32::from(rect.y) * 2 + u32::from(rect.height));
    let ((x1, y1), (x2, y2)) = (centre(from), centre(to));
    x1.abs_diff(x2) + y1.abs_diff(y2)
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: Rect = Rect::new(2, 0, 7, 3);
    const BOTTOM: Rect = Rect::new(2, 8, 7, 3);

    fn grid(nodes: &[Rect]) -> Grid {
        let mut grid = Grid::empty(20, 12);
        for node in nodes {
            grid.node(*node);
        }
        grid
    }

    fn search(grid: &Grid, from: Rect, to: Rect) -> Option<Route> {
        Search { grid, from, to, exit: Direction::Down, entry: Direction::Down }.run()
    }

    #[test]
    fn an_edge_leaves_down_the_middle_and_enters_from_the_top() {
        let route = search(&grid(&[TOP, BOTTOM]), TOP, BOTTOM).unwrap();

        assert_eq!(route.corners(), [Point::new(5, 2), Point::new(5, 7)]);
        assert_eq!((route.departure(), route.arrival(), route.end), (Direction::Down, Direction::Down, Point::new(5, 8)));
    }

    #[test]
    fn an_edge_goes_round_a_node_in_its_way() {
        let middle = Rect::new(0, 4, 12, 3);

        let route = search(&grid(&[TOP, BOTTOM, middle]), TOP, BOTTOM).unwrap();

        assert!(route.cells.iter().all(|cell| cell.x >= 12 || cell.y < 4 || cell.y > 6));
    }

    #[test]
    fn an_edge_crosses_a_line_only_at_a_right_angle() {
        let mut crossed = grid(&[TOP, BOTTOM]);
        crossed.fill(Rect::new(0, 5, 20, 1), Use::Line { horizontal: true });
        let mut parallel = grid(&[TOP, BOTTOM]);
        parallel.fill(Rect::new(5, 3, 1, 5), Use::Line { horizontal: false });

        let across = search(&crossed, TOP, BOTTOM).unwrap();
        let beside = search(&parallel, TOP, BOTTOM).unwrap();

        assert!(across.cells.contains(&Point::new(5, 5)));
        assert!(!beside.cells.iter().any(|cell| cell.x == 5 && (3..8).contains(&cell.y)));
    }

    #[test]
    fn a_route_on_the_grid_blocks_its_turns_and_ends_and_lines_the_rest() {
        let mut grid = grid(&[TOP, BOTTOM]);
        let route = Route { cells: vec![Point::new(5, 2), Point::new(5, 3), Point::new(6, 3), Point::new(7, 3)], end: Point::new(8, 3) };

        grid.add(&route);

        let uses = [(5, 2), (5, 3), (6, 3), (7, 3), (8, 3)].map(|(x, y)| grid.get(Point::new(x, y)));
        assert_eq!(uses, [Use::Blocked, Use::Blocked, Use::Line { horizontal: true }, Use::Blocked, Use::Blocked]);
    }

    #[test]
    fn a_target_walled_in_cannot_be_reached() {
        let mut walled = grid(&[TOP, BOTTOM]);
        walled.fill(Rect::new(0, 6, 20, 1), Use::Blocked);

        assert_eq!(search(&walled, TOP, BOTTOM), None);
    }
}
